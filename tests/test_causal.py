from __future__ import annotations

import json

import pandas as pd
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.studies.causal import CounterfactualEstimator
from chiyoda.studies.models import StudyBundle
from chiyoda.studies.schema import StudyConfig


def _bundle(values: dict[int, float]) -> StudyBundle:
    summary = pd.DataFrame(
        [
            {
                "study_name": "synthetic",
                "scenario_name": "toy",
                "variant_name": "run",
                "seed": seed,
                "run_id": f"seed_{seed}",
                "record_type": "run",
                "mean_travel_time_s": value,
            }
            for seed, value in values.items()
        ]
    )
    empty = pd.DataFrame()
    return StudyBundle(
        metadata={},
        summary=summary,
        steps=empty,
        cells=empty,
        agent_steps=empty,
        agents=empty,
        bottlenecks=empty,
        dwell_samples=empty,
        exits=empty,
        hazards=empty,
    )


def test_counterfactual_estimator_matched_pair_ate():
    baseline = _bundle({1: 10.0, 2: 12.0, 3: 14.0})
    treated = _bundle({1: 8.0, 2: 9.0, 3: 13.0})

    result = CounterfactualEstimator(bootstrap_samples=100, random_seed=1).compare(
        baseline,
        treated,
        metrics=["mean_travel_time_s"],
    )
    row = result.iloc[0]

    assert row["n_pairs"] == 3
    assert row["ate"] == -2.0
    assert row["seed_sensitivity_min"] <= -2.0 <= row["seed_sensitivity_max"]
    assert row["e_value"] > 1.0


def test_causal_compare_cli_emits_json(tmp_path):
    baseline_dir = tmp_path / "baseline"
    treated_dir = tmp_path / "treated"
    _bundle({1: 10.0, 2: 12.0}).export(baseline_dir, table_formats=("csv",))
    _bundle({1: 9.0, 2: 10.0}).export(treated_dir, table_formats=("csv",))

    result = CliRunner().invoke(
        cli,
        [
            "causal-compare",
            str(baseline_dir),
            str(treated_dir),
            "--metric",
            "mean_travel_time_s",
            "--bootstrap-samples",
            "20",
        ],
    )
    payload = json.loads(result.output)

    assert result.exit_code == 0
    assert payload[0]["metric"] == "mean_travel_time_s"
    assert payload[0]["ate"] == -1.5


def test_study_config_exposes_treatment_assignments():
    config = StudyConfig(
        name="assignment",
        scenario_file="scenario.yaml",
        seeds=[1, 2],
        treatment_assignments={1: "baseline", 2: "treated"},
    )

    assert config.treatment_assignments == {1: "baseline", 2: "treated"}
