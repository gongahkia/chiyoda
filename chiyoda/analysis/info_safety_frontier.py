"""Static information-safety frontier checks."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

import numpy as np

from chiyoda.scenarios.manager import ScenarioManager

SAFE = "safe"
BORDERLINE = "borderline"
HARMFUL = "harmful"


@dataclass(frozen=True)
class InfoSafetyVerdict:
    scenario_name: str
    verdict: str
    score: float
    entropy_reduction_potential: float
    convergence_pressure: float
    queue_pressure: float
    exposure_pressure: float
    reasons: list[str]
    tags: list[str]

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def check_info_safety_scenario(path: str | Path) -> InfoSafetyVerdict:
    manager = ScenarioManager()
    scenario = manager.load_config(str(path))
    simulation = manager.build_simulation(scenario)
    return check_info_safety(scenario, simulation)


def check_info_safety(scenario: dict[str, Any], simulation) -> InfoSafetyVerdict:
    sc = scenario.get("scenario", scenario)
    name = str(sc.get("name", Path(str(sc.get("_source_file", "scenario"))).stem))
    metadata = sc.get("metadata", {}) or {}
    tags = [str(tag) for tag in metadata.get("info_safety_tags", []) or []]

    active_agents = [
        agent
        for agent in simulation.agents
        if not getattr(agent, "is_responder", False)
        and not getattr(agent, "is_hostile", False)
    ]
    population = max(1, len(active_agents))
    exits = max(1, len(simulation.exits))
    walkable = max(1, _walkable_cell_count(simulation.layout))
    mean_familiarity = float(
        np.mean([getattr(agent, "familiarity", 0.5) for agent in active_agents])
    )

    info_cfg = sc.get("information", {}) or {}
    observation_radius = float(info_cfg.get("observation_radius", 5.0))
    beacon_radius = float(info_cfg.get("beacon_radius", 8.0))
    gossip_radius = float(info_cfg.get("gossip_radius", 2.0))
    reach = min(1.0, (observation_radius + beacon_radius + gossip_radius) / 18.0)
    entropy_reduction_potential = _clip((1.0 - mean_familiarity) * reach)

    bottleneck_ratio = min(1.0, len(simulation.bottleneck_zones) / (exits * 3.0))
    density_ratio = min(1.0, population / (walkable * 0.45))
    queue_pressure = _clip(0.6 * bottleneck_ratio + 0.4 * density_ratio)

    hazard_terms = []
    for hazard in simulation.hazards:
        severity = float(getattr(hazard, "severity", 0.0))
        radius = float(getattr(hazard, "radius", 0.0))
        hazard_terms.append(severity * max(1.0, radius) / 4.0)
    exposure_pressure = _clip(sum(hazard_terms))

    exit_load = min(1.0, population / (exits * 40.0))
    hostile_bonus = _hostile_convergence_bonus(sc)
    intervention_bonus = _intervention_convergence_bonus(sc)
    convergence_pressure = _clip(exit_load + hostile_bonus + intervention_bonus)

    score = _clip(
        entropy_reduction_potential
        * convergence_pressure
        * (0.55 * queue_pressure + 0.45 * exposure_pressure)
    )
    reasons = _reasons(
        entropy_reduction_potential,
        convergence_pressure,
        queue_pressure,
        exposure_pressure,
        hostile_bonus,
        intervention_bonus,
    )
    if score >= 0.18 or (
        entropy_reduction_potential >= 0.45
        and convergence_pressure >= 0.45
        and exposure_pressure >= 0.25
    ):
        verdict = HARMFUL
    elif score >= 0.07 or (
        entropy_reduction_potential >= 0.35
        and (queue_pressure >= 0.35 or exposure_pressure >= 0.2)
    ):
        verdict = BORDERLINE
    else:
        verdict = SAFE

    return InfoSafetyVerdict(
        scenario_name=name,
        verdict=verdict,
        score=score,
        entropy_reduction_potential=entropy_reduction_potential,
        convergence_pressure=convergence_pressure,
        queue_pressure=queue_pressure,
        exposure_pressure=exposure_pressure,
        reasons=reasons,
        tags=tags,
    )


def _walkable_cell_count(layout) -> int:
    count = 0
    for floor in layout.floors.values():
        for row in floor.grid:
            count += sum(1 for cell in row if cell != "X")
    return count


def _hostile_convergence_bonus(scenario: dict[str, Any]) -> float:
    channels = scenario.get("hostile_channels", []) or []
    bonus = 0.0
    for channel in channels:
        objective = str(channel.get("objective", ""))
        plausibility = float(
            channel.get("plausibility", channel.get("credibility", 0.5))
        )
        budget = float(channel.get("budget", 1.0))
        if objective in {"decoy-exit", "responder-spoof", "gossip-poison"}:
            bonus += min(0.45, 0.12 * budget) * plausibility
        elif objective == "panic-induce":
            bonus += min(0.3, 0.08 * budget) * plausibility
    return _clip(bonus)


def _intervention_convergence_bonus(scenario: dict[str, Any]) -> float:
    interventions = scenario.get("interventions", {}) or {}
    policy = str(interventions.get("policy", ""))
    if policy in {"global", "static", "beacon", "responder_relay"}:
        return 0.18
    if policy in {"entropy_targeted", "density_aware", "exposure_aware"}:
        return 0.08
    return 0.0


def _reasons(
    entropy_reduction_potential: float,
    convergence_pressure: float,
    queue_pressure: float,
    exposure_pressure: float,
    hostile_bonus: float,
    intervention_bonus: float,
) -> list[str]:
    reasons: list[str] = []
    if entropy_reduction_potential >= 0.45:
        reasons.append("high_entropy_reduction_potential")
    if convergence_pressure >= 0.45:
        reasons.append("high_convergence_pressure")
    if queue_pressure >= 0.35:
        reasons.append("queue_pressure")
    if exposure_pressure >= 0.2:
        reasons.append("hazard_exposure_pressure")
    if hostile_bonus > 0:
        reasons.append("hostile_channel_convergence")
    if intervention_bonus > 0:
        reasons.append("broadcast_policy_convergence")
    if not reasons:
        reasons.append("no_static_harmful_regime_signal")
    return reasons


def _clip(value: float) -> float:
    return min(1.0, max(0.0, float(value)))
