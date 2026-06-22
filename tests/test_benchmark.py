from __future__ import annotations

import pandas as pd
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.studies.benchmark import (
    BenchmarkSpec,
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
