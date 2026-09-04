from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class ScenarioAssertionIssue:
    code: str
    message: str
    observed: Any = None
    expected: Any = None

    def to_dict(self) -> dict[str, Any]:
        payload = {"code": self.code, "message": self.message}
        if self.observed is not None:
            payload["observed"] = self.observed
        if self.expected is not None:
            payload["expected"] = self.expected
        return payload


@dataclass(frozen=True)
class ScenarioAssertionResult:
    ok: bool
    issues: list[ScenarioAssertionIssue]

    def to_dict(self) -> dict[str, Any]:
        return {
            "ok": self.ok,
            "issues": [issue.to_dict() for issue in self.issues],
        }


def evaluate_scenario_assertions(
    scenario: dict[str, Any], simulation: Any
) -> ScenarioAssertionResult:
    config = scenario.get("assertions", {}) or {}
    issues: list[ScenarioAssertionIssue] = []
    if not config:
        return ScenarioAssertionResult(ok=True, issues=[])

    evacuated = len(simulation.completed_agents)
    remaining = len(
        [
            agent
            for agent in simulation.agents
            if not getattr(agent, "has_evacuated", False)
            and not getattr(agent, "is_responder", False)
        ]
    )

    _check_count("evacuated", evacuated, config.get("evacuated"), issues)
    _check_count("remaining", remaining, config.get("remaining"), issues)
    _check_count(
        "impossible_floor_jumps",
        len(getattr(simulation, "impossible_floor_jumps", [])),
        config.get("impossible_floor_jumps"),
        issues,
    )

    if bool(config.get("no_impossible_floor_jumps", False)) and getattr(
        simulation, "impossible_floor_jumps", []
    ):
        issues.append(
            ScenarioAssertionIssue(
                "impossible_floor_jump",
                "one or more agents changed floor without a configured connector event",
                observed=getattr(simulation, "impossible_floor_jumps", []),
                expected=[],
            )
        )

    travel = config.get("travel_time_s")
    if travel is not None:
        times = [float(value) for value in getattr(simulation, "travel_times_s", [])]
        if not times:
            issues.append(
                ScenarioAssertionIssue(
                    "missing_travel_times",
                    "no evacuated-agent travel times were recorded",
                )
            )
        else:
            _check_range(
                "travel_time_s.min", min(times), travel.get("min"), None, issues
            )
            _check_range(
                "travel_time_s.max", max(times), None, travel.get("max"), issues
            )
            _check_range(
                "travel_time_s.mean",
                sum(times) / len(times),
                travel.get("mean_min"),
                travel.get("mean_max"),
                issues,
            )

    connectors = config.get("connector_usage", {}) or {}
    observed_connectors = dict(getattr(simulation, "connector_usage_cumulative", {}))
    for connector_id, expected in connectors.items():
        _check_count(
            f"connector_usage.{connector_id}",
            int(observed_connectors.get(str(connector_id), 0)),
            expected,
            issues,
        )

    exit_usage = config.get("exit_usage", {}) or {}
    if exit_usage:
        observed_exit_usage = _exit_usage(simulation)
        for exit_ref, expected in exit_usage.items():
            _check_count(
                f"exit_usage.{exit_ref}",
                int(observed_exit_usage.get(str(exit_ref), 0)),
                expected,
                issues,
            )

    cohort_exit_usage = config.get("cohort_exit_usage", {}) or {}
    if cohort_exit_usage:
        observed_cohort_usage = _cohort_exit_usage(simulation)
        for cohort, exits in cohort_exit_usage.items():
            for exit_ref, expected in (exits or {}).items():
                _check_count(
                    f"cohort_exit_usage.{cohort}.{exit_ref}",
                    int(
                        observed_cohort_usage.get(str(cohort), {}).get(str(exit_ref), 0)
                    ),
                    expected,
                    issues,
                )

    base_speed = config.get("agent_base_speed_mps")
    if base_speed is not None:
        speeds = [
            float(getattr(agent, "base_speed", 0.0))
            for agent in simulation.agents
            if not getattr(agent, "is_responder", False)
            and not getattr(agent, "is_hostile", False)
        ]
        if not speeds:
            issues.append(
                ScenarioAssertionIssue(
                    "missing_agent_speeds",
                    "no non-responder agent base speeds were recorded",
                )
            )
        else:
            _check_range(
                "agent_base_speed_mps.min",
                min(speeds),
                base_speed.get("min"),
                None,
                issues,
            )
            _check_range(
                "agent_base_speed_mps.max",
                max(speeds),
                None,
                base_speed.get("max"),
                issues,
            )
            _check_range(
                "agent_base_speed_mps.mean",
                sum(speeds) / len(speeds),
                base_speed.get("mean_min"),
                base_speed.get("mean_max"),
                issues,
            )

    release_step = config.get("agent_release_step")
    if release_step is not None:
        release_steps = [
            int(getattr(agent, "release_step", 0))
            for agent in simulation.agents
            if not getattr(agent, "is_responder", False)
            and not getattr(agent, "is_hostile", False)
        ]
        if not release_steps:
            issues.append(
                ScenarioAssertionIssue(
                    "missing_agent_release_steps",
                    "no non-responder agent release steps were recorded",
                )
            )
        else:
            _check_range(
                "agent_release_step.min",
                min(release_steps),
                release_step.get("min"),
                None,
                issues,
            )
            _check_range(
                "agent_release_step.max",
                max(release_steps),
                None,
                release_step.get("max"),
                issues,
            )
            _check_range(
                "agent_release_step.mean",
                sum(release_steps) / len(release_steps),
                release_step.get("mean_min"),
                release_step.get("mean_max"),
                issues,
            )

    behavioral = config.get("behavioral_plausibility")
    if behavioral is not None:
        _check_metric_rules(
            "behavioral_plausibility",
            _behavioral_plausibility_metrics(simulation),
            behavioral,
            issues,
        )

    hazard = config.get("hazard_plausibility")
    if hazard is not None:
        _check_metric_rules(
            "hazard_plausibility",
            _hazard_plausibility_metrics(simulation),
            hazard,
            issues,
        )

    vertical = config.get("vertical_transport")
    if vertical is not None:
        _check_metric_rules(
            "vertical_transport",
            _vertical_transport_metrics(simulation),
            vertical,
            issues,
        )

    hostile_llm = config.get("hostile_llm")
    if hostile_llm is not None:
        _check_metric_rules(
            "hostile_llm",
            _hostile_llm_metrics(simulation),
            hostile_llm,
            issues,
        )

    latest = (
        simulation.step_history[-1] if getattr(simulation, "step_history", []) else None
    )
    if latest is not None:
        _check_connector_map(
            "connector_flow",
            getattr(latest, "connector_flow", {}),
            config.get("connector_flow", {}),
            issues,
        )
        _check_connector_map(
            "connector_capacity",
            getattr(latest, "connector_capacity", {}),
            config.get("connector_capacity", {}),
            issues,
        )
        _check_connector_map(
            "connector_queue_length",
            getattr(latest, "connector_queue_length", {}),
            config.get("connector_queue_length", {}),
            issues,
        )

    exit_floors = config.get("exit_floors")
    if exit_floors is not None:
        expected = {str(value) for value in exit_floors}
        observed = {
            str(tuple(exit_.pos)[0])
            for agent in simulation.completed_agents
            for exit_ in simulation.exits
            if getattr(agent, "evacuated_via", None)
            == simulation.exit_labels.get(tuple(exit_.pos))
        }
        missing = sorted(expected - observed)
        if missing:
            issues.append(
                ScenarioAssertionIssue(
                    "missing_exit_floor",
                    "expected at least one evacuation through each configured floor",
                    observed=sorted(observed),
                    expected=sorted(expected),
                )
            )

    return ScenarioAssertionResult(ok=not issues, issues=issues)


def _check_count(
    name: str, observed: int, expected: Any, issues: list[ScenarioAssertionIssue]
) -> None:
    if expected is None:
        return
    if isinstance(expected, dict):
        if "eq" in expected and observed != int(expected["eq"]):
            issues.append(_issue(name, observed, expected))
        if "min" in expected and observed < int(expected["min"]):
            issues.append(_issue(name, observed, expected))
        if "max" in expected and observed > int(expected["max"]):
            issues.append(_issue(name, observed, expected))
        return
    if observed != int(expected):
        issues.append(_issue(name, observed, expected))


def _check_range(
    name: str,
    observed: float,
    minimum: Any,
    maximum: Any,
    issues: list[ScenarioAssertionIssue],
) -> None:
    if minimum is not None and observed < float(minimum):
        issues.append(_issue(name, observed, {"min": minimum}))
    if maximum is not None and observed > float(maximum):
        issues.append(_issue(name, observed, {"max": maximum}))


def _check_connector_map(
    name: str,
    observed: dict[str, Any],
    expected: dict[str, Any],
    issues: list[ScenarioAssertionIssue],
) -> None:
    for connector_id, rule in (expected or {}).items():
        _check_count(
            f"{name}.{connector_id}",
            int(observed.get(str(connector_id), 0)),
            rule,
            issues,
        )


def _check_metric_rules(
    prefix: str,
    observed: dict[str, float],
    expected: Any,
    issues: list[ScenarioAssertionIssue],
) -> None:
    for metric, rule in (expected or {}).items():
        name = f"{prefix}.{metric}"
        value = observed.get(str(metric))
        if value is None or not math.isfinite(value):
            issues.append(
                ScenarioAssertionIssue(
                    f"missing_assertion_metric:{name}",
                    f"{name} was not available",
                    observed=value,
                    expected=rule,
                )
            )
            continue
        if isinstance(rule, dict):
            if "eq" in rule and value != float(rule["eq"]):
                issues.append(_issue(name, value, rule))
            _check_range(name, value, rule.get("min"), rule.get("max"), issues)
        elif value != float(rule):
            issues.append(_issue(name, value, rule))


def _behavioral_plausibility_metrics(simulation: Any) -> dict[str, float]:
    steps = list(getattr(simulation, "step_history", []) or [])
    metrics: dict[str, float] = {
        "completed_agents": float(len(getattr(simulation, "completed_agents", []))),
        "evacuation_completion_fraction": _evacuation_completion_fraction(simulation),
        "exit_imbalance": _exit_imbalance(simulation),
        "stuck_agent_steps": 0.0,
        "peak_cell_occupancy": 0.0,
        "mean_density": 0.0,
        "max_mean_density": 0.0,
        "mean_agent_speed_mps": 0.0,
        "max_agent_speed_mps": 0.0,
        "mean_local_density": 0.0,
        "max_local_density": 0.0,
        "max_bottleneck_queue_length": 0.0,
        "mean_bottleneck_dwell_s": 0.0,
        "max_bottleneck_dwell_s": 0.0,
    }
    if not steps:
        return metrics

    densities: list[float] = []
    speeds: list[float] = []
    local_densities: list[float] = []
    bottleneck_dwell: list[float] = []
    for step in steps:
        metrics["peak_cell_occupancy"] = max(
            metrics["peak_cell_occupancy"], _step_peak_occupancy(step)
        )
        densities.append(float(getattr(step, "mean_density", 0.0)))
        for agent in getattr(step, "agents", []) or []:
            speed = float(getattr(agent, "speed", 0.0))
            density = float(getattr(agent, "local_density", 0.0))
            speeds.append(speed)
            local_densities.append(density)
            if speed <= 0.05:
                metrics["stuck_agent_steps"] += 1.0
        for bottleneck in (getattr(step, "bottlenecks", {}) or {}).values():
            metrics["max_bottleneck_queue_length"] = max(
                metrics["max_bottleneck_queue_length"],
                float(getattr(bottleneck, "queue_length", 0)),
            )

    for samples in (getattr(simulation, "bottleneck_dwell_samples", {}) or {}).values():
        bottleneck_dwell.extend(float(value) for value in samples)

    metrics["mean_density"] = _mean(densities)
    metrics["max_mean_density"] = max(densities) if densities else 0.0
    metrics["mean_agent_speed_mps"] = _mean(speeds)
    metrics["max_agent_speed_mps"] = max(speeds) if speeds else 0.0
    metrics["mean_local_density"] = _mean(local_densities)
    metrics["max_local_density"] = max(local_densities) if local_densities else 0.0
    metrics["mean_bottleneck_dwell_s"] = _mean(bottleneck_dwell)
    metrics["max_bottleneck_dwell_s"] = (
        max(bottleneck_dwell) if bottleneck_dwell else 0.0
    )
    return metrics


def _hazard_plausibility_metrics(simulation: Any) -> dict[str, float]:
    hazards = list(getattr(simulation, "hazards", []) or [])
    snapshots = [
        hazard.snapshot()
        for hazard in hazards
        if hasattr(hazard, "snapshot") and callable(hazard.snapshot)
    ]
    imported = [
        snapshot
        for snapshot in snapshots
        if bool(snapshot.get("imported_field", False))
    ]
    loads: list[float] = []
    exposures: list[float] = []
    for step in getattr(simulation, "step_history", []) or []:
        for agent in getattr(step, "agents", []) or []:
            loads.append(float(getattr(agent, "hazard_load", 0.0)))
            exposures.append(float(getattr(agent, "hazard_exposure", 0.0)))
    if not loads:
        loads = [
            float(getattr(agent, "current_hazard_load", 0.0))
            for agent in getattr(simulation, "agents", [])
        ]
    if not exposures:
        exposures = [
            float(getattr(agent, "hazard_exposure", 0.0))
            for agent in getattr(simulation, "agents", [])
        ]
    return {
        "hazard_count": float(len(hazards)),
        "imported_hazard_count": float(len(imported)),
        "stylized_hazard_count": float(len(hazards) - len(imported)),
        "max_hazard_radius_m": max(
            (float(snapshot.get("radius", 0.0)) for snapshot in snapshots),
            default=0.0,
        ),
        "max_hazard_severity": max(
            (float(snapshot.get("severity", 0.0)) for snapshot in snapshots),
            default=0.0,
        ),
        "max_imported_gas_concentration": max(
            (
                float(snapshot.get("max_gas_concentration", 0.0))
                for snapshot in imported
            ),
            default=0.0,
        ),
        "min_imported_visibility_m": min(
            (float(snapshot.get("min_visibility_m", 0.0)) for snapshot in imported),
            default=0.0,
        ),
        "max_obscuration_percent_m": max(
            (
                float(snapshot.get("max_obscuration_percent_m", 0.0))
                for snapshot in imported
            ),
            default=0.0,
        ),
        "mean_agent_hazard_load": _mean(loads),
        "max_agent_hazard_load": max(loads) if loads else 0.0,
        "mean_agent_hazard_exposure": _mean(exposures),
        "max_agent_hazard_exposure": max(exposures) if exposures else 0.0,
    }


def _vertical_transport_metrics(simulation: Any) -> dict[str, float]:
    connectors = list(
        getattr(getattr(simulation, "layout", None), "connectors", []) or []
    )
    vertical = [
        connector
        for connector in connectors
        if str(getattr(connector, "type", "")).lower()
        in {"stairs", "ramp", "elevator", "escalator"}
    ]
    elevators = [
        connector
        for connector in connectors
        if str(getattr(connector, "type", "")).lower() == "elevator"
    ]
    usage = {
        str(key): int(value)
        for key, value in (
            getattr(simulation, "connector_usage_cumulative", {}) or {}
        ).items()
    }
    metrics = {
        "connector_count": float(len(connectors)),
        "vertical_connector_count": float(len(vertical)),
        "elevator_count": float(len(elevators)),
        "vertical_connector_usage": float(
            sum(
                usage.get(str(getattr(connector, "id", "")), 0)
                for connector in vertical
            )
        ),
        "elevator_usage": float(
            sum(
                usage.get(str(getattr(connector, "id", "")), 0)
                for connector in elevators
            )
        ),
        "max_connector_queue_length": 0.0,
        "max_connector_capacity_used": 0.0,
        "max_elevator_queue_length": 0.0,
        "max_elevator_capacity_used": 0.0,
    }
    elevator_ids = {str(getattr(connector, "id", "")) for connector in elevators}
    for step in getattr(simulation, "step_history", []) or []:
        queue = getattr(step, "connector_queue_length", {}) or {}
        used = getattr(step, "connector_capacity_used", {}) or {}
        for connector_id, value in queue.items():
            queue_value = float(value)
            metrics["max_connector_queue_length"] = max(
                metrics["max_connector_queue_length"], queue_value
            )
            if str(connector_id) in elevator_ids:
                metrics["max_elevator_queue_length"] = max(
                    metrics["max_elevator_queue_length"], queue_value
                )
        for connector_id, value in used.items():
            used_value = float(value)
            metrics["max_connector_capacity_used"] = max(
                metrics["max_connector_capacity_used"], used_value
            )
            if str(connector_id) in elevator_ids:
                metrics["max_elevator_capacity_used"] = max(
                    metrics["max_elevator_capacity_used"], used_value
                )
    return metrics


def _hostile_llm_metrics(simulation: Any) -> dict[str, float]:
    hostile_events = list(getattr(simulation, "hostile_channel_events", []) or [])
    llm_calls = list(getattr(simulation, "llm_call_audit", []) or [])
    decision_events = list(getattr(simulation, "agent_decision_events", []) or [])
    return {
        "hostile_channel_count": float(
            len(getattr(simulation, "hostile_channels", []) or [])
        ),
        "hostile_channel_event_count": float(len(hostile_events)),
        "hostile_channel_recipients": float(
            sum(int(getattr(event, "recipients", 0)) for event in hostile_events)
        ),
        "mean_hostile_credibility": _mean(
            [float(getattr(event, "credibility", 0.0)) for event in hostile_events]
        ),
        "llm_call_count": float(len(llm_calls)),
        "llm_accepted_count": _llm_status_count(llm_calls, "accepted"),
        "llm_rejected_count": _llm_status_count(llm_calls, "rejected"),
        "llm_fallback_count": float(
            sum(1 for call in llm_calls if bool(call.get("used_fallback", False)))
        ),
        "llm_cache_hit_count": float(
            sum(1 for call in llm_calls if str(call.get("cache_status", "")) == "hit")
        ),
        "llm_cache_miss_count": float(
            sum(1 for call in llm_calls if str(call.get("cache_status", "")) == "miss")
        ),
        "llm_budget_blocked_count": float(
            sum(
                1
                for call in llm_calls
                if str(call.get("cache_status", "")) == "budget_exceeded"
                or str(call.get("provider", "")) == "budget_guard"
            )
        ),
        "agent_llm_decision_count": float(len(decision_events)),
        "agent_llm_accepted_count": float(
            sum(
                1
                for event in decision_events
                if str(getattr(event, "validation_status", "")) == "accepted"
            )
        ),
        "agent_llm_rejected_count": float(
            sum(
                1
                for event in decision_events
                if str(getattr(event, "validation_status", "")) == "rejected"
            )
        ),
        "agent_llm_fallback_count": float(
            sum(
                1
                for event in decision_events
                if bool(getattr(event, "used_fallback", False))
            )
        ),
    }


def _llm_status_count(calls: list[dict[str, Any]], status: str) -> float:
    return float(
        sum(1 for call in calls if str(call.get("validation_status", "")) == status)
    )


def _step_peak_occupancy(step: Any) -> float:
    peaks: list[float] = []
    grid = getattr(step, "occupancy_grid", None)
    if grid is not None:
        peaks.append(float(grid.max()))
    for grids in (getattr(step, "floor_grids", {}) or {}).values():
        floor_grid = grids.get("occupancy_grid") if isinstance(grids, dict) else None
        if floor_grid is not None:
            peaks.append(float(floor_grid.max()))
    return max(peaks) if peaks else 0.0


def _evacuation_completion_fraction(simulation: Any) -> float:
    evacuatable = [
        agent
        for agent in getattr(simulation, "agents", [])
        if not getattr(agent, "is_responder", False)
        and not getattr(agent, "is_hostile", False)
    ]
    if not evacuatable:
        return 1.0
    evacuated = sum(
        1 for agent in evacuatable if getattr(agent, "has_evacuated", False)
    )
    return float(evacuated / len(evacuatable))


def _exit_imbalance(simulation: Any) -> float:
    counts = [
        int(value)
        for value in (getattr(simulation, "exit_flow_cumulative", {}) or {}).values()
    ]
    total = sum(counts)
    if total <= 0 or len(counts) < 2:
        return 0.0
    shares = [count / total for count in counts]
    return float(max(shares) - min(shares))


def _mean(values: list[float]) -> float:
    return float(sum(values) / len(values)) if values else 0.0


def _exit_usage(simulation: Any) -> dict[str, int]:
    aliases: dict[str, int] = {}
    label_to_cell = {label: cell for cell, label in simulation.exit_labels.items()}
    for label, count in getattr(simulation, "exit_flow_cumulative", {}).items():
        cell = label_to_cell.get(label)
        for alias in _exit_aliases(label, cell):
            aliases[alias] = aliases.get(alias, 0) + int(count)
    return aliases


def _cohort_exit_usage(simulation: Any) -> dict[str, dict[str, int]]:
    label_to_cell = {label: cell for cell, label in simulation.exit_labels.items()}
    usage: dict[str, dict[str, int]] = {}
    for agent in simulation.completed_agents:
        cohort = str(getattr(agent, "cohort_name", "unknown"))
        label = getattr(agent, "evacuated_via", None)
        if label is None:
            continue
        cell = label_to_cell.get(label)
        usage.setdefault(cohort, {})
        for alias in _exit_aliases(label, cell):
            usage[cohort][alias] = usage[cohort].get(alias, 0) + 1
    return usage


def _exit_aliases(label: str, cell: Any) -> list[str]:
    aliases = [str(label)]
    if cell is not None:
        floor, x, y = tuple(cell)
        aliases.append(f"{floor}:{x},{y}")
    return aliases


def _issue(name: str, observed: Any, expected: Any) -> ScenarioAssertionIssue:
    return ScenarioAssertionIssue(
        code=f"assertion_failed:{name}",
        message=f"{name} assertion failed",
        observed=observed,
        expected=expected,
    )
