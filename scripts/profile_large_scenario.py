#!/usr/bin/env python3
from __future__ import annotations

import argparse
import cProfile
import io
import json
import pstats
import sys
import time
import tracemalloc
from copy import deepcopy
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from chiyoda.scenarios.manager import ScenarioManager


def profile_scenario(
    scenario_file: str,
    *,
    max_steps: int | None = None,
    population_total: int | None = None,
    top_n: int = 25,
) -> dict:
    manager = ScenarioManager()
    scenario = deepcopy(manager.load_config(scenario_file))
    if max_steps is not None:
        scenario.setdefault("simulation", {})["max_steps"] = int(max_steps)
    if population_total is not None:
        population = scenario.setdefault("population", {})
        population["total"] = int(population_total)
        if population.get("cohorts"):
            population["cohorts"] = []

    sim = manager.build_simulation(scenario)
    profiler = cProfile.Profile()
    tracemalloc.start()
    started = time.perf_counter()
    profiler.enable()
    sim.run()
    profiler.disable()
    elapsed_s = time.perf_counter() - started
    current_bytes, peak_bytes = tracemalloc.get_traced_memory()
    tracemalloc.stop()

    stream = io.StringIO()
    pstats.Stats(profiler, stream=stream).strip_dirs().sort_stats(
        "cumtime"
    ).print_stats(top_n)
    top_functions = stream.getvalue()

    cells_rows = sum(
        len(step.agents)
        + sum(
            int((grids["occupancy_grid"] > 0).sum())
            for grids in step.floor_grids.values()
        )
        for step in sim.step_history
    )
    graph_nodes = (
        sim.navigator.graph.number_of_nodes() if sim.navigator is not None else 0
    )
    graph_edges = (
        sim.navigator.graph.number_of_edges() if sim.navigator is not None else 0
    )

    return {
        "scenario": scenario_file,
        "elapsed_s": elapsed_s,
        "steps": sim.current_step,
        "agents": len(sim.agents),
        "active_or_completed_agents": len(sim.completed_agents)
        + len(sim._active_agents()),
        "navigator_graph_nodes": graph_nodes,
        "navigator_graph_edges": graph_edges,
        "density_updates": sim.current_step + 1,
        "telemetry_steps": len(sim.step_history),
        "telemetry_agent_rows": sum(len(step.agents) for step in sim.step_history),
        "telemetry_active_cell_rows_est": cells_rows,
        "connector_usage": dict(getattr(sim, "connector_usage_cumulative", {})),
        "peak_memory_mb": peak_bytes / (1024 * 1024),
        "current_memory_mb": current_bytes / (1024 * 1024),
        "top_functions_cumtime": top_functions,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Profile a Chiyoda scenario run.")
    parser.add_argument("scenario_file")
    parser.add_argument("-o", "--output", default="out/profile_large_scenario.json")
    parser.add_argument("--max-steps", type=int)
    parser.add_argument("--population-total", type=int)
    parser.add_argument("--top-n", type=int, default=25)
    args = parser.parse_args()

    result = profile_scenario(
        args.scenario_file,
        max_steps=args.max_steps,
        population_total=args.population_total,
        top_n=args.top_n,
    )
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, default=str) + "\n")
    print(json.dumps(result, indent=2, default=str))


if __name__ == "__main__":
    main()
