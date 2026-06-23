from __future__ import annotations

import time
from types import SimpleNamespace

import numpy as np
import pandas as pd
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.navigation.social_force import adjusted_step
from chiyoda.navigation.spatial_index import SpatialIndex
from chiyoda.studies.benchmark import (
    BenchmarkSpec,
    _leaderboard,
    benchmark_score,
    benchmark_spec_v1,
    generate_reference_trajectories,
)


def test_benchmark_spec_v1_schema_and_scenarios():
    spec = benchmark_spec_v1()
    schema = BenchmarkSpec.json_schema()

    assert spec.suite == "v1"
    assert {scenario.name for scenario in spec.scenarios} == {
        "transit_cbrn",
        "transit_shooter",
        "transit_mixed",
    }
    assert "scenarios" in schema["required"]


def test_benchmark_score_rewards_lower_risk_metrics():
    strong = benchmark_score(
        {
            "mean_travel_time_s": 5.0,
            "p95_hazard_exposure": 0.1,
            "equity_time_gap_s": 1.0,
            "harmful_convergence_index_induced": 0.1,
        }
    )
    weak = benchmark_score(
        {
            "mean_travel_time_s": 20.0,
            "p95_hazard_exposure": 3.0,
            "equity_time_gap_s": 5.0,
            "harmful_convergence_index_induced": 2.0,
        }
    )

    assert strong > weak


def test_leaderboard_reports_bootstrap_ci_and_scenario_breakdown():
    frame = pd.DataFrame(
        [
            {"scenario": "a", "seed": 1, "benchmark_score": 10.0},
            {"scenario": "b", "seed": 1, "benchmark_score": 20.0},
            {"scenario": "a", "seed": 2, "benchmark_score": 30.0},
            {"scenario": "b", "seed": 2, "benchmark_score": 40.0},
        ]
    )

    leaderboard = _leaderboard(frame, "policyhash", suite="v1")
    entry = leaderboard["entries"][0]

    assert entry["mean_score"] == 25.0
    assert entry["score_ci_low"] <= entry["mean_score"] <= entry["score_ci_high"]
    assert entry["seeds_used"] == [1, 2]
    assert entry["bootstrap_n"] == 1000
    assert entry["tier"] == "smoke"
    assert {row["scenario"] for row in entry["scenario_breakdown"]} == {"a", "b"}
    assert all(
        row["score_ci_low"] <= row["mean_score"] <= row["score_ci_high"]
        for row in entry["scenario_breakdown"]
    )


def test_leaderboard_marks_twenty_seed_submission_official():
    frame = pd.DataFrame(
        [
            {"scenario": "a", "seed": seed, "benchmark_score": float(seed)}
            for seed in range(20)
        ]
    )

    leaderboard = _leaderboard(frame, "policyhash", suite="v1")

    assert leaderboard["entries"][0]["tier"] == "official"


def test_benchmark_cli_group_is_available():
    result = CliRunner().invoke(cli, ["benchmark", "--help"])

    assert result.exit_code == 0
    assert "submit" in result.output


def test_reference_trajectory_generation(tmp_path):
    output = generate_reference_trajectories(output_file=tmp_path / "reference.parquet")
    frame = pd.read_parquet(output)

    assert not frame.empty
    assert {"transit_cbrn", "transit_shooter", "transit_mixed"} <= set(
        frame["scenario"]
    )


def test_optional_numba_kernel_perf_smoke():
    pos = np.array([5.0, 5.0])
    desired = np.array([0.12, 0.0])
    neighbors = np.array([[5.5, 5.0], [4.7, 5.1], [8.0, 8.0]], dtype=float)
    walls = [[4.0, 5.0], [7.0, 5.0]]
    start = time.perf_counter()
    for _ in range(250):
        step = adjusted_step(pos, desired, neighbors, walls, 0.1, counter_flow=True)
    elapsed_s = time.perf_counter() - start

    assert np.all(np.isfinite(step))
    assert elapsed_s < 2.0

    index = SpatialIndex()
    index.update(
        [
            SimpleNamespace(id=idx, pos=np.array([float(idx), 0.0, 0.0]))
            for idx in range(64)
        ]
    )
    assert index.local_density(np.array([10.0, 0.0, 0.0]), radius=2.0) > 0.0
