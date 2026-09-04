"""Read and verify Chiyoda run bundles without coupling analysis to the runtime."""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
from typing import Any


class BundleError(ValueError):
    """A run bundle is malformed or fails its integrity contract."""


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
    clearance_time_s = _optional_time(metrics, "clearance_time_s")
    last_exit_time_s = _optional_time(metrics, "last_exit_time_s")
    _validate_exit_time_semantics(bundle, metrics, clearance_time_s, last_exit_time_s)
    _validate_information_delivery_semantics(bundle, scenario_body, information_delivery)
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


def _validate_exit_time_semantics(
    bundle: dict[str, Any],
    metrics: dict[str, Any],
    clearance_time_s: float | None,
    last_exit_time_s: float | None,
) -> None:
    """Apply the versioned clearance distinction without rejecting older bundles."""

    if bundle.get("bundle_version") not in {"0.17", "0.18", "0.19"}:
        return
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
    if bundle.get("bundle_version") not in {"0.18", "0.19"}:
        return
    messages = scenario.get("messages", [])
    countermeasures = scenario.get("countermeasures", [])
    if not isinstance(messages, list) or not isinstance(countermeasures, list):
        raise BundleError("0.18 scenario must contain messages and countermeasures arrays")
    expected: dict[str, str] = {}
    for kind, interventions in (("message", messages), ("countermeasure", countermeasures)):
        for intervention in interventions:
            if not isinstance(intervention, dict) or not isinstance(intervention.get("id"), str):
                raise BundleError("0.18 scenario contains a malformed information intervention")
            expected[intervention["id"]] = kind
    if set(delivery) != set(expected):
        raise BundleError("0.18 information_delivery must cover every declared intervention")
    for intervention, expected_kind in expected.items():
        if delivery[intervention]["kind"] != expected_kind:
            raise BundleError("0.18 information_delivery kind disagrees with the scenario")


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
