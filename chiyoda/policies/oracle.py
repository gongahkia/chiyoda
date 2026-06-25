from __future__ import annotations

import json
from collections.abc import Callable
from hashlib import sha256
from pathlib import Path
from typing import Any

import pandas as pd

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.benchmark import _leaderboard, _spec_for_suite, benchmark_score

PolicySelector = Callable[[dict[str, Any]], dict[str, Any]]


def oracle_policy_for_scenario(scenario: dict[str, Any]) -> dict[str, Any]:
    hazards = scenario.get("hazards", []) or []
    hostile_channels = scenario.get("hostile_channels", []) or []
    if hostile_channels:
        return _global_broadcast("rumor_control", credibility=0.9)
    if hazards:
        hazard_types = {str(hazard.get("type", "")).upper() for hazard in hazards}
        if hazard_types & {"CRUSH", "FLOOD", "EARTHQUAKE", "AFTERSHOCK"}:
            return _density_aware()
        return _global_broadcast("hazard_guidance", credibility=0.88)
    return {"policy": "none"}


def evaluate_baseline(
    *,
    baseline: str,
    suite: str = "v1",
    output_dir: str | Path = "out/baseline_eval",
    policy_selector: PolicySelector | None = None,
) -> dict[str, Any]:
    selector = policy_selector or oracle_policy_for_scenario
    spec = _spec_for_suite(suite)
    manager = ScenarioManager()
    analytics = SimulationAnalytics()
    rows: list[dict[str, Any]] = []

    for scenario_spec in spec.scenarios:
        for seed in spec.seeds:
            scenario = manager.load_config(scenario_spec.scenario_file)
            policy = selector(scenario)
            if policy:
                scenario["interventions"] = policy
            scenario.setdefault("simulation", {})
            scenario["simulation"]["random_seed"] = int(seed)
            simulation = manager.build_simulation(scenario)
            simulation.run()
            metrics = analytics.calculate_performance_metrics(simulation)
            metrics["equity_time_gap_s"] = _equity_time_gap(simulation)
            metrics["benchmark_score"] = benchmark_score(metrics)
            rows.append(
                {
                    "suite": suite,
                    "baseline": baseline,
                    "scenario": scenario_spec.name,
                    "seed": int(seed),
                    "policy": json.dumps(policy, sort_keys=True),
                    **metrics,
                }
            )

    frame = pd.DataFrame(rows)
    policy_hash = _hash_json({"baseline": baseline, "suite": suite})
    leaderboard = _leaderboard(frame, policy_hash, suite=suite)
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    frame.to_csv(out / "baseline_runs.csv", index=False)
    (out / "leaderboard.json").write_text(json.dumps(leaderboard, indent=2) + "\n")
    (out / "baseline_summary.json").write_text(
        json.dumps(
            {
                "baseline": baseline,
                "suite": suite,
                "policy_hash": policy_hash,
                "leaderboard": leaderboard,
            },
            indent=2,
        )
        + "\n"
    )
    return {"leaderboard": leaderboard, "output_dir": str(out), "runs": frame}


def _global_broadcast(message_type: str, *, credibility: float) -> dict[str, Any]:
    return {
        "policy": "global_broadcast",
        "start_step": 0,
        "interval_steps": 10,
        "budget_per_interval": 1,
        "message_type": message_type,
        "message_radius": 40.0,
        "credibility": float(credibility),
    }


def _density_aware() -> dict[str, Any]:
    return {
        "policy": "density_aware",
        "start_step": 0,
        "interval_steps": 5,
        "budget_per_interval": 1,
        "message_radius": 8.0,
        "credibility": 0.86,
    }


def _equity_time_gap(simulation) -> float:
    values = [float(agent.travel_time_s) for agent in simulation.completed_agents]
    if not values:
        return 0.0
    return float(max(values) - min(values))


def _hash_json(value: Any) -> str:
    return sha256(
        json.dumps(value, sort_keys=True, default=str).encode("utf-8")
    ).hexdigest()[:16]
