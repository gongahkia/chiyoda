from __future__ import annotations

from scripts.profile_large_scenario import profile_scenario
from scripts.run_toy_calibrations import run as run_toy_calibrations


def test_toy_calibration_runner_reports_assertions():
    result = run_toy_calibrations(["scenarios/validation_elevator_queue.yaml"])

    assert result["ok"] is True
    assert result["scenarios"][0]["connector_usage"]["elevator_queue"] >= 3


def test_profile_large_scenario_smoke():
    result = profile_scenario(
        "scenarios/validation_elevator_queue.yaml",
        max_steps=20,
        population_total=3,
        top_n=5,
    )

    assert result["steps"] <= 20
    assert result["navigator_graph_nodes"] > 0
    assert result["telemetry_steps"] > 0
    assert "run" in result["top_functions_cumtime"]
