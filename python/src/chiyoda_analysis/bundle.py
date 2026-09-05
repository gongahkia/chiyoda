"""Read and verify Chiyoda run bundles without coupling analysis to the runtime."""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
from typing import Any


class BundleError(ValueError):
    """A run bundle is malformed or fails its integrity contract."""


_CURRENT_BUNDLE_VERSIONS = frozenset({"0.17", "0.18", "0.19", "0.20", "0.21", "0.22", "0.23", "0.24", "0.25", "0.26", "0.27", "0.28"})
_INFORMATION_DELIVERY_BUNDLE_VERSIONS = frozenset({"0.18", "0.19", "0.20", "0.21", "0.22", "0.23", "0.24", "0.25", "0.26", "0.27", "0.28"})
_QUEUE_METRIC_BUNDLE_VERSIONS = frozenset({"0.22", "0.23", "0.24", "0.25", "0.26", "0.27", "0.28"})
_RESOURCE_QUEUE_METRIC_BUNDLE_VERSIONS = frozenset({"0.23", "0.24", "0.25", "0.26", "0.27", "0.28"})
_QUEUE_ENTRY_EVENT_BUNDLE_VERSIONS = frozenset({"0.24", "0.25", "0.26", "0.27", "0.28"})
_MOVEMENT_METRIC_BUNDLE_VERSIONS = frozenset({"0.28"})
_REMAINING_AGENT_STATES = frozenset(
    {
        "moving",
        "waiting_to_depart",
        "waiting_at_waypoint",
        "waiting_for_route",
        "waiting_for_lift",
        "waiting_for_connector",
        "waiting_for_gate",
        "waiting_for_exit",
        "in_transit",
    }
)


def load_bundle(path: str | Path, *, verify: bool = True) -> dict[str, Any]:
    """Load a `run.json` bundle and optionally verify its SHA-256 digest.

    The Rust reference runtime hashes compact JSON in declaration order with the
    `bundle_hash` field set to an empty string. Python's standard JSON decoder
    preserves object order, allowing this reader to independently reproduce a
    bundle hash without importing the simulator.
    """

    bundle_path = Path(path)
    try:
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    except OSError as error:
        raise BundleError(f"cannot read {bundle_path}: {error}") from error
    except json.JSONDecodeError as error:
        raise BundleError(f"invalid JSON in {bundle_path}: {error}") from error
    if not isinstance(bundle, dict):
        raise BundleError("run bundle root must be a JSON object")
    if verify:
        _verify_hash(bundle)
    return bundle


def summarize(bundle: dict[str, Any]) -> dict[str, Any]:
    """Return a stable, analysis-friendly summary of a verified bundle."""

    metrics = bundle.get("metrics")
    scenario = bundle.get("scenario")
    if not isinstance(metrics, dict) or not isinstance(scenario, dict):
        raise BundleError("bundle does not contain versioned scenario and metrics objects")
    scenario_body = scenario.get("scenario")
    if not isinstance(scenario_body, dict):
        raise BundleError("bundle scenario body is malformed")
    evacuated_by_exit = _count_map(metrics, "evacuated_by_exit", "exit identifiers")
    remaining_by_state = _count_map(metrics, "remaining_by_state", "state identifiers")
    information_delivery = _information_delivery(metrics)
    queue_metrics = _queue_metrics(bundle, metrics)
    movement_metrics = _movement_metrics(bundle, metrics)
    clearance_time_s = _optional_time(metrics, "clearance_time_s")
    last_exit_time_s = _optional_time(metrics, "last_exit_time_s")
    _validate_exit_time_semantics(bundle, metrics, clearance_time_s, last_exit_time_s)
    _validate_information_delivery_semantics(bundle, scenario_body, information_delivery)
    _validate_metric_attribution(
        bundle,
        scenario_body,
        metrics,
        evacuated_by_exit,
        remaining_by_state,
    )
    return {
        "bundle_hash": bundle.get("bundle_hash"),
        "scenario_hash": bundle.get("scenario_hash"),
        "scenario": scenario_body.get("name"),
        "runtime_version": bundle.get("runtime_version"),
        "frames": len(bundle.get("trace", [])),
        "events": len(bundle.get("events", [])),
        "evacuated_by_exit": evacuated_by_exit,
        "remaining_by_state": remaining_by_state,
        "information_delivery": information_delivery,
        "queue_metrics": queue_metrics,
        "movement_metrics": movement_metrics,
        "clearance_time_s": clearance_time_s,
        "last_exit_time_s": last_exit_time_s,
        "metrics": metrics,
    }


def _count_map(metrics: dict[str, Any], field: str, subject: str) -> dict[str, int]:
    counts = metrics.get(field, {})
    if not isinstance(counts, dict) or any(
        not isinstance(identifier, str)
        or not isinstance(count, int)
        or isinstance(count, bool)
        or count < 0
        for identifier, count in counts.items()
    ):
        raise BundleError(f"metrics.{field} must map {subject} to counts")
    return counts


def _optional_time(metrics: dict[str, Any], field: str) -> float | None:
    value = metrics.get(field)
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise BundleError(f"metrics.{field} must be a finite non-negative number or null")
    return float(value)


def _information_delivery(metrics: dict[str, Any]) -> dict[str, dict[str, int | str]]:
    delivery = metrics.get("information_delivery", {})
    if not isinstance(delivery, dict):
        raise BundleError("metrics.information_delivery must map intervention identifiers to counts")
    normalized: dict[str, dict[str, int | str]] = {}
    for intervention, value in delivery.items():
        if not isinstance(intervention, str) or not isinstance(value, dict):
            raise BundleError("metrics.information_delivery is malformed")
        kind = value.get("kind")
        received = value.get("received_agents")
        accepted = value.get("accepted_agents")
        if (
            kind not in {"message", "countermeasure"}
            or isinstance(received, bool)
            or not isinstance(received, int)
            or received < 0
            or isinstance(accepted, bool)
            or not isinstance(accepted, int)
            or accepted < 0
            or accepted > received
        ):
            raise BundleError("metrics.information_delivery contains invalid intervention counts")
        normalized[intervention] = {
            "kind": kind,
            "received_agents": received,
            "accepted_agents": accepted,
        }
    return normalized


def _queue_metrics(bundle: dict[str, Any], metrics: dict[str, Any]) -> dict[str, Any]:
    """Validate current discrete queue telemetry without treating it as observed flow."""

    if bundle.get("bundle_version") not in _QUEUE_METRIC_BUNDLE_VERSIONS:
        return {}
    queue_metrics = metrics.get("queue_metrics")
    expected_fields = {"lift", "connector", "gate", "exit"}
    if bundle.get("bundle_version") in _RESOURCE_QUEUE_METRIC_BUNDLE_VERSIONS:
        expected_fields.add("by_resource")
    if not isinstance(queue_metrics, dict) or set(queue_metrics) != expected_fields:
        raise BundleError("current metrics.queue_metrics must cover lift, connector, gate, and exit")
    total_agents, _ = _current_agent_counts(metrics)
    legacy_fields = {
        "lift": "queued_for_lift_agents",
        "connector": "queued_for_connector_agents",
        "gate": "queued_for_gate_agents",
        "exit": "queued_for_exit_agents",
    }
    normalized: dict[str, Any] = {}
    for resource, legacy_field in legacy_fields.items():
        value = queue_metrics[resource]
        if not isinstance(value, dict):
            raise BundleError(f"metrics.queue_metrics.{resource} must be an object")
        ever_queued = value.get("ever_queued_agents")
        cumulative_wait = value.get("cumulative_wait_agent_seconds")
        peak_waiting = value.get("peak_waiting_agents")
        if (
            isinstance(ever_queued, bool)
            or not isinstance(ever_queued, int)
            or ever_queued < 0
            or ever_queued > total_agents
            or isinstance(cumulative_wait, bool)
            or not isinstance(cumulative_wait, (int, float))
            or not math.isfinite(cumulative_wait)
            or cumulative_wait < 0
            or isinstance(peak_waiting, bool)
            or not isinstance(peak_waiting, int)
            or peak_waiting < 0
            or peak_waiting > ever_queued
            or metrics.get(legacy_field) != ever_queued
        ):
            raise BundleError(f"metrics.queue_metrics.{resource} is invalid or disagrees with its exposure count")
        normalized[resource] = {
            "ever_queued_agents": ever_queued,
            "cumulative_wait_agent_seconds": float(cumulative_wait),
            "peak_waiting_agents": peak_waiting,
        }
    if bundle.get("bundle_version") in _RESOURCE_QUEUE_METRIC_BUNDLE_VERSIONS:
        normalized["by_resource"] = _queue_resource_breakdown(
            queue_metrics["by_resource"], normalized, _queue_resource_ids(bundle)
        )
    if bundle.get("bundle_version") in _QUEUE_ENTRY_EVENT_BUNDLE_VERSIONS:
        _queue_entry_events(bundle, normalized)
    return normalized


def _movement_metrics(bundle: dict[str, Any], metrics: dict[str, Any]) -> dict[str, int | float]:
    """Validate local-clearance audit telemetry without relabeling it as physical data."""

    if bundle.get("bundle_version") not in _MOVEMENT_METRIC_BUNDLE_VERSIONS:
        return {}
    value = metrics.get("movement_metrics")
    expected_fields = {
        "agents_with_local_clearance_adjustments",
        "local_clearance_adjustment_steps",
        "cumulative_local_clearance_adjustment_m",
        "maximum_local_clearance_adjustment_m",
    }
    if not isinstance(value, dict) or set(value) != expected_fields:
        raise BundleError("current metrics.movement_metrics must contain complete local-clearance telemetry")
    total_agents, _ = _current_agent_counts(metrics)
    adjusted_agents = value["agents_with_local_clearance_adjustments"]
    adjustment_steps = value["local_clearance_adjustment_steps"]
    cumulative_adjustment = value["cumulative_local_clearance_adjustment_m"]
    maximum_adjustment = value["maximum_local_clearance_adjustment_m"]
    if (
        isinstance(adjusted_agents, bool)
        or not isinstance(adjusted_agents, int)
        or adjusted_agents < 0
        or adjusted_agents > total_agents
        or isinstance(adjustment_steps, bool)
        or not isinstance(adjustment_steps, int)
        or adjustment_steps < adjusted_agents
        or isinstance(cumulative_adjustment, bool)
        or not isinstance(cumulative_adjustment, (int, float))
        or not math.isfinite(cumulative_adjustment)
        or cumulative_adjustment < 0
        or isinstance(maximum_adjustment, bool)
        or not isinstance(maximum_adjustment, (int, float))
        or not math.isfinite(maximum_adjustment)
        or maximum_adjustment < 0
        or maximum_adjustment > cumulative_adjustment
    ):
        raise BundleError("metrics.movement_metrics is invalid")
    if adjustment_steps == 0 and (
        adjusted_agents != 0 or cumulative_adjustment != 0 or maximum_adjustment != 0
    ):
        raise BundleError("zero local-clearance adjustments must have zero-valued telemetry")
    return {
        "agents_with_local_clearance_adjustments": adjusted_agents,
        "local_clearance_adjustment_steps": adjustment_steps,
        "cumulative_local_clearance_adjustment_m": float(cumulative_adjustment),
        "maximum_local_clearance_adjustment_m": float(maximum_adjustment),
    }


def _queue_resource_ids(bundle: dict[str, Any]) -> dict[str, set[str]]:
    """Derive the exact current queueable-resource sets from canonical IR."""

    canonical = bundle.get("scenario")
    scenario = canonical.get("scenario") if isinstance(canonical, dict) else None
    if not isinstance(scenario, dict):
        raise BundleError("current bundle must contain a canonical scenario for queue attribution")
    connectors = scenario.get("connectors")
    gates = scenario.get("gates")
    exits = scenario.get("exits")
    if not isinstance(connectors, list) or not isinstance(gates, list) or not isinstance(exits, list):
        raise BundleError("current canonical scenario lacks resource declarations for queue attribution")

    resource_ids = {"lifts": set(), "connectors": set(), "gates": set(), "exits": set()}
    for connector in connectors:
        if not isinstance(connector, dict) or len(connector) != 1:
            raise BundleError("current canonical connector declaration is malformed")
        kind, properties = next(iter(connector.items()))
        if kind not in {"Stair", "Ramp", "Escalator", "Lift"} or not isinstance(properties, dict):
            raise BundleError("current canonical connector kind is malformed")
        identifier = properties.get("id")
        if not isinstance(identifier, str) or not identifier:
            raise BundleError("current canonical connector has an invalid identifier")
        if kind == "Lift":
            resource_ids["lifts"].add(identifier)
        elif properties.get("capacity_per_s") is not None:
            resource_ids["connectors"].add(identifier)
    for gate in gates:
        if not isinstance(gate, dict) or not isinstance(gate.get("id"), str) or not gate["id"]:
            raise BundleError("current canonical gate has an invalid identifier")
        resource_ids["gates"].add(gate["id"])
    for exit_ in exits:
        if not isinstance(exit_, dict) or not isinstance(exit_.get("id"), str) or not exit_["id"]:
            raise BundleError("current canonical exit has an invalid identifier")
        if exit_.get("capacity_per_s") is not None:
            resource_ids["exits"].add(exit_["id"])
    return resource_ids


def _queue_resource_breakdown(
    value: Any, aggregate: dict[str, Any], expected_ids: dict[str, set[str]]
) -> dict[str, dict[str, dict[str, int | float]]]:
    """Validate queue-resource attribution without interpreting it as field data."""

    expected_fields = {
        "lifts": "lift",
        "connectors": "connector",
        "gates": "gate",
        "exits": "exit",
    }
    if not isinstance(value, dict) or set(value) != set(expected_fields):
        raise BundleError("current metrics.queue_metrics.by_resource has invalid resource groups")

    normalized: dict[str, dict[str, dict[str, int | float]]] = {}
    for collection, aggregate_name in expected_fields.items():
        resources = value[collection]
        if not isinstance(resources, dict) or not all(isinstance(identifier, str) for identifier in resources):
            raise BundleError(f"metrics.queue_metrics.by_resource.{collection} must map identifiers")
        if set(resources) != expected_ids[collection]:
            raise BundleError(
                f"metrics.queue_metrics.by_resource.{collection} identifiers disagree with the scenario"
            )
        cumulative_wait = 0.0
        exposures = 0
        normalized_resources: dict[str, dict[str, int | float]] = {}
        for identifier, telemetry in resources.items():
            if not isinstance(telemetry, dict) or set(telemetry) != {
                "ever_queued_agents",
                "cumulative_wait_agent_seconds",
                "peak_waiting_agents",
            }:
                raise BundleError(
                    f"metrics.queue_metrics.by_resource.{collection}.{identifier} is malformed"
                )
            ever_queued = telemetry["ever_queued_agents"]
            cumulative = telemetry["cumulative_wait_agent_seconds"]
            peak_waiting = telemetry["peak_waiting_agents"]
            aggregate_telemetry = aggregate[aggregate_name]
            if (
                not isinstance(ever_queued, int)
                or isinstance(ever_queued, bool)
                or not isinstance(peak_waiting, int)
                or isinstance(peak_waiting, bool)
                or not isinstance(cumulative, (int, float))
                or isinstance(cumulative, bool)
                or ever_queued < 0
                or peak_waiting < 0
                or peak_waiting > ever_queued
                or peak_waiting > aggregate_telemetry["peak_waiting_agents"]
                or not math.isfinite(cumulative)
                or cumulative < 0
            ):
                raise BundleError(
                    f"metrics.queue_metrics.by_resource.{collection}.{identifier} is invalid"
                )
            exposures += ever_queued
            cumulative_wait = round(cumulative_wait + float(cumulative), 9)
            normalized_resources[identifier] = {
                "ever_queued_agents": ever_queued,
                "cumulative_wait_agent_seconds": float(cumulative),
                "peak_waiting_agents": peak_waiting,
            }
        if (
            cumulative_wait != aggregate[aggregate_name]["cumulative_wait_agent_seconds"]
            or exposures < aggregate[aggregate_name]["ever_queued_agents"]
        ):
            raise BundleError(
                f"metrics.queue_metrics.by_resource.{collection} disagrees with its aggregate"
            )
        normalized[collection] = normalized_resources
    return normalized


def _queue_entry_events(bundle: dict[str, Any], queue_metrics: dict[str, Any]) -> None:
    """Cross-check current queue exposure telemetry against its event audit trail."""

    events = bundle.get("events")
    if not isinstance(events, list):
        raise BundleError("current bundle events must be a list for queue-entry audit")
    by_resource = queue_metrics["by_resource"]
    event_collections = {
        "queue_entered_lift": ("lift", "lifts"),
        "queue_entered_connector": ("connector", "connectors"),
        "queue_entered_gate": ("gate", "gates"),
        "queue_entered_exit": ("exit", "exits"),
    }
    entries: dict[str, dict[str, set[str]]] = {
        collection: {} for _, collection in event_collections.values()
    }
    for event in events:
        if not isinstance(event, dict):
            continue
        event_spec = event_collections.get(event.get("kind"))
        if event_spec is None:
            continue
        _, collection = event_spec
        time_s = event.get("time_s")
        subject = event.get("subject")
        resource_id = event.get("detail")
        if (
            not isinstance(time_s, (int, float))
            or isinstance(time_s, bool)
            or not math.isfinite(time_s)
            or time_s < 0
            or not isinstance(subject, str)
            or not subject
            or not isinstance(resource_id, str)
            or not resource_id
        ):
            raise BundleError("queue-entry event is malformed")
        if resource_id not in by_resource[collection]:
            raise BundleError("queue_entered event names an unknown queue resource")
        resource_entries = entries[collection].setdefault(resource_id, set())
        if subject in resource_entries:
            raise BundleError("queue_entered event repeats one agent/resource pair")
        resource_entries.add(subject)
    for kind, collection in (value for value in event_collections.values()):
        aggregate = queue_metrics[kind]
        all_agents: set[str] = set()
        for resource_id, telemetry in by_resource[collection].items():
            resource_agents = entries[collection].get(resource_id, set())
            if len(resource_agents) != telemetry["ever_queued_agents"]:
                raise BundleError("queue_entered events disagree with resource queue telemetry")
            all_agents.update(resource_agents)
        if len(all_agents) != aggregate["ever_queued_agents"]:
            raise BundleError("queue_entered events disagree with aggregate queue telemetry")


def _validate_exit_time_semantics(
    bundle: dict[str, Any],
    metrics: dict[str, Any],
    clearance_time_s: float | None,
    last_exit_time_s: float | None,
) -> None:
    """Apply the versioned clearance distinction without rejecting older bundles."""

    if bundle.get("bundle_version") not in _CURRENT_BUNDLE_VERSIONS:
        return
    total_agents, evacuated_agents = _current_agent_counts(metrics)
    if (clearance_time_s is not None) != (evacuated_agents == total_agents):
        raise BundleError("current clearance_time_s must be present exactly for a fully evacuated run")
    if (last_exit_time_s is not None) != (evacuated_agents > 0):
        raise BundleError("current last_exit_time_s must be present exactly when an agent evacuated")
    if clearance_time_s is not None and clearance_time_s != last_exit_time_s:
        raise BundleError("current clearance_time_s must equal last_exit_time_s")


def _validate_information_delivery_semantics(
    bundle: dict[str, Any],
    scenario: dict[str, Any],
    delivery: dict[str, dict[str, int | str]],
) -> None:
    if bundle.get("bundle_version") not in _INFORMATION_DELIVERY_BUNDLE_VERSIONS:
        return
    messages = scenario.get("messages", [])
    countermeasures = scenario.get("countermeasures", [])
    if not isinstance(messages, list) or not isinstance(countermeasures, list):
        raise BundleError("current scenario must contain messages and countermeasures arrays")
    expected: dict[str, str] = {}
    for kind, interventions in (("message", messages), ("countermeasure", countermeasures)):
        for intervention in interventions:
            if not isinstance(intervention, dict) or not isinstance(intervention.get("id"), str):
                raise BundleError("current scenario contains a malformed information intervention")
            expected[intervention["id"]] = kind
    if set(delivery) != set(expected):
        raise BundleError("current information_delivery must cover every declared intervention")
    for intervention, expected_kind in expected.items():
        if delivery[intervention]["kind"] != expected_kind:
            raise BundleError("current information_delivery kind disagrees with the scenario")


def _validate_metric_attribution(
    bundle: dict[str, Any],
    scenario: dict[str, Any],
    metrics: dict[str, Any],
    evacuated_by_exit: dict[str, int],
    remaining_by_state: dict[str, int],
) -> None:
    """Mirror the current runtime's non-time metric invariants when available."""

    if bundle.get("bundle_version") not in _CURRENT_BUNDLE_VERSIONS:
        return
    total_agents, evacuated_agents = _current_agent_counts(metrics)
    if evacuated_by_exit:
        exits = scenario.get("exits", [])
        if not isinstance(exits, list) or any(
            not isinstance(exit_, dict) or not isinstance(exit_.get("id"), str) for exit_ in exits
        ):
            raise BundleError("current scenario must contain exits with string identifiers")
        exit_ids = {exit_["id"] for exit_ in exits}
        if not set(evacuated_by_exit).issubset(exit_ids):
            raise BundleError("metrics.evacuated_by_exit contains an unknown exit")
        if sum(evacuated_by_exit.values()) != evacuated_agents:
            raise BundleError("metrics.evacuated_by_exit must total evacuated_agents")
    if remaining_by_state:
        if not set(remaining_by_state).issubset(_REMAINING_AGENT_STATES):
            raise BundleError("metrics.remaining_by_state contains an unknown agent state")
        if sum(remaining_by_state.values()) != total_agents - evacuated_agents:
            raise BundleError("metrics.remaining_by_state must total non-evacuated agents")


def _current_agent_counts(metrics: dict[str, Any]) -> tuple[int, int]:
    total_agents = metrics.get("total_agents")
    evacuated_agents = metrics.get("evacuated_agents")
    if (
        isinstance(total_agents, bool)
        or not isinstance(total_agents, int)
        or total_agents < 0
        or isinstance(evacuated_agents, bool)
        or not isinstance(evacuated_agents, int)
        or evacuated_agents < 0
        or evacuated_agents > total_agents
    ):
        raise BundleError("current metrics must contain valid total_agents and evacuated_agents counts")
    return total_agents, evacuated_agents


def _verify_hash(bundle: dict[str, Any]) -> None:
    supplied = bundle.get("bundle_hash")
    if not isinstance(supplied, str) or len(supplied) != 64:
        raise BundleError("bundle_hash must be a SHA-256 hexadecimal digest")
    unsigned = dict(bundle)
    unsigned["bundle_hash"] = ""
    payload = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    actual = hashlib.sha256(payload).hexdigest()
    if actual != supplied:
        raise BundleError(f"bundle integrity check failed: expected {supplied}, calculated {actual}")
