from __future__ import annotations

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
    scenario: dict[str, Any], simulation
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


def _exit_usage(simulation) -> dict[str, int]:
    aliases: dict[str, int] = {}
    label_to_cell = {label: cell for cell, label in simulation.exit_labels.items()}
    for label, count in getattr(simulation, "exit_flow_cumulative", {}).items():
        cell = label_to_cell.get(label)
        for alias in _exit_aliases(label, cell):
            aliases[alias] = aliases.get(alias, 0) + int(count)
    return aliases


def _cohort_exit_usage(simulation) -> dict[str, dict[str, int]]:
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
