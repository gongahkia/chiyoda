from __future__ import annotations

import json

import pandas as pd
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.studies import run_counterfactual_pair
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


def test_run_counterfactual_pair_emits_delta_payload(tmp_path):
    scenario_path = tmp_path / "scenario.yaml"
    scenario_path.write_text(_counterfactual_scenario_yaml())

    result = run_counterfactual_pair(
        scenario_path,
        repetitions=1,
        bootstrap_samples=20,
    )
    delta = result["causal_delta"]

    assert result["baseline"].metadata["study_name"].endswith("_no_intervention")
    assert result["treated"].metadata["study_name"].endswith("_treated")
    assert delta["metadata"]["baseline_variant"] == "no_intervention"
    assert delta["metadata"]["treated_variant"] == "treated"
    assert delta["interventions"][0]["intervention"]["policy"] == "global_broadcast"
    assert {row["metric"] for row in delta["metrics"]} >= {
        "agents_evacuated",
        "harmful_convergence_index",
    }
    assert all("ci_lower" in row and "ci_upper" in row for row in delta["metrics"])


def test_run_counterfactual_cli_writes_both_bundles_and_json(tmp_path):
    scenario_path = tmp_path / "scenario.yaml"
    scenario_path.write_text(_counterfactual_scenario_yaml())
    output_dir = tmp_path / "out"

    result = CliRunner().invoke(
        cli,
        [
            "run",
            str(scenario_path),
            "-o",
            str(output_dir),
            "--counterfactual",
            "--counterfactual-repetitions",
            "1",
            "--counterfactual-bootstrap-samples",
            "20",
            "--table-format",
            "csv",
        ],
    )

    assert result.exit_code == 0, result.output
    assert (output_dir / "no_intervention" / "metadata.json").exists()
    assert (output_dir / "treated" / "metadata.json").exists()
    payload = json.loads((output_dir / "causal_delta.json").read_text())
    assert payload["metadata"]["baseline_variant"] == "no_intervention"
    assert payload["interventions"][0]["intervention"]["policy"] == "global_broadcast"


def test_study_config_exposes_treatment_assignments():
    config = StudyConfig(
        name="assignment",
        scenario_file="scenario.yaml",
        seeds=[1, 2],
        treatment_assignments={1: "baseline", 2: "treated"},
    )

    assert config.treatment_assignments == {1: "baseline", 2: "treated"}


def _counterfactual_scenario_yaml() -> str:
    return """
name: causal_counterfactual_toy
layout:
  floors:
    - id: "0"
      z: 0.0
      text: |
        XXXXXX
        X@..EX
        XXXXXX
population:
  total: 1
simulation:
  max_steps: 3
  random_seed: 11
information:
  mode: none
interventions:
  policy: global_broadcast
  start_step: 0
  interval_steps: 1
  budget_per_interval: 1
  message_radius: 20.0
  credibility: 0.95
"""
