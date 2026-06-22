#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import validate_scenario_config


DEFAULT_SCENARIOS = [
    "scenarios/validation_corridor.yaml",
    "scenarios/validation_wuppertal_bottleneck.yaml",
    "scenarios/validation_multifloor_connectors.yaml",
    "scenarios/validation_elevator_queue.yaml",
]


def run(paths: list[str]) -> dict:
    manager = ScenarioManager()
    rows = []
    for path in paths:
        scenario = manager.load_config(path)
        validation = validate_scenario_config(scenario, manager=manager)
        sim = manager.build_simulation(scenario)
        sim.run()
        assertions = evaluate_scenario_assertions(scenario, sim)
        rows.append(
            {
                "scenario": path,
                "validation_ok": not validation.has_errors,
                "assertions_ok": assertions.ok,
                "assertion_issues": [issue.to_dict() for issue in assertions.issues],
                "agents_total": len(
                    [
                        agent
                        for agent in sim.agents
                        if not getattr(agent, "is_responder", False)
                    ]
                ),
                "agents_evacuated": len(sim.completed_agents),
                "max_travel_time_s": (
                    max(sim.travel_times_s) if sim.travel_times_s else None
                ),
                "mean_travel_time_s": (
                    sum(sim.travel_times_s) / len(sim.travel_times_s)
                    if sim.travel_times_s
                    else None
                ),
                "connector_usage": dict(getattr(sim, "connector_usage_cumulative", {})),
                "impossible_floor_jumps": list(
                    getattr(sim, "impossible_floor_jumps", [])
                ),
            }
        )
    return {
        "ok": all(row["validation_ok"] and row["assertions_ok"] for row in rows),
        "scenarios": rows,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run toy calibration/regression scenarios."
    )
    parser.add_argument("scenarios", nargs="*", default=DEFAULT_SCENARIOS)
    parser.add_argument("-o", "--output", default="out/toy_calibrations.json")
    args = parser.parse_args()

    result = run(args.scenarios)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, default=str) + "\n")
    print(json.dumps(result, indent=2, default=str))
    if not result["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
