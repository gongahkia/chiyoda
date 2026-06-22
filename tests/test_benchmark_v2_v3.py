from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import pandas as pd
import pytest

from chiyoda.studies.benchmark import (
    BenchmarkSpec,
    _spec_for_suite,
    benchmark_spec_v2,
    benchmark_spec_v3,
)
from chiyoda.studies.models import StudyBundle


def test_benchmark_spec_v2_registration():
    spec = benchmark_spec_v2()
    assert spec.suite == "v2"
    assert {scenario.name for scenario in spec.scenarios} == {
        "wildfire_wui",
        "transit_shooter",
    }
    assert spec.scoring_rule == "composite_v1"
    assert _spec_for_suite("v2") == spec


def test_benchmark_spec_v3_registration():
    spec = benchmark_spec_v3()
    assert spec.suite == "v3"
    assert {scenario.name for scenario in spec.scenarios} == {
        "flood_urban",
        "quake_aftershock",
    }
    assert _spec_for_suite("v3") == spec


def test_benchmark_spec_v2_v3_schema_validity():
    schema = BenchmarkSpec.json_schema()
    for spec in (benchmark_spec_v2(), benchmark_spec_v3()):
        payload = spec.to_dict()
        for key in schema["required"]:
            assert key in payload


def test_benchmark_spec_v2_v3_seed_reproducibility():
    spec_a = benchmark_spec_v2()
    spec_b = benchmark_spec_v2()
    assert spec_a.seeds == spec_b.seeds
    assert benchmark_spec_v3().seeds == [42, 137]


def test_submit_policy_accepts_non_v1_suite(monkeypatch, tmp_path):
    import chiyoda.studies.benchmark as benchmark_module

    class StubSimulation:
        completed_agents = [
            SimpleNamespace(travel_time_s=1.0),
            SimpleNamespace(travel_time_s=3.0),
        ]

        def run(self):
            return None

    class StubManager:
        def load_config(self, scenario_file):
            return {"name": Path(scenario_file).stem, "simulation": {}}

        def build_simulation(self, config):
            return StubSimulation()

    class StubAnalytics:
        def calculate_performance_metrics(self, simulation):
            return {
                "mean_travel_time_s": 1.0,
                "p95_hazard_exposure": 0.0,
                "harmful_convergence_index_induced": 0.0,
            }

    monkeypatch.setattr(benchmark_module, "ScenarioManager", StubManager)
    monkeypatch.setattr(benchmark_module, "SimulationAnalytics", StubAnalytics)
    monkeypatch.setattr(
        benchmark_module,
        "write_leaderboard_site",
        lambda leaderboard, output_file: output_file,
    )

    result = benchmark_module.submit_policy(
        policy_path=None, suite="v2", output_dir=tmp_path
    )

    assert result["leaderboard"]["suite"] == "v2"
    assert result["leaderboard"]["entries"][0]["run_count"] == 4
    assert result["manifest"]["suite"] == "v2"
    assert (tmp_path / "benchmark_runs.csv").exists()


def test_benchmark_spec_artifacts_exist():
    for suite in ("v2", "v3"):
        path = Path(f"docs/benchmark/benchmark_spec_{suite}.json")
        assert path.exists(), f"missing spec artifact: {path}"
        payload = json.loads(path.read_text())
        assert payload["suite"] == suite


def test_unknown_suite_raises():
    with pytest.raises(ValueError):
        _spec_for_suite("v999")


def test_composite_v1_causal_delta_is_positive_for_better_intervention():
    from chiyoda.studies.benchmark import composite_v1_causal

    treated = {
        "mean_travel_time_s": 5.0,
        "p95_hazard_exposure": 0.2,
        "equity_time_gap_s": 1.0,
        "harmful_convergence_index_induced": 0.1,
    }
    baseline = {
        "mean_travel_time_s": 30.0,
        "p95_hazard_exposure": 2.0,
        "equity_time_gap_s": 8.0,
        "harmful_convergence_index_induced": 1.5,
    }
    result = composite_v1_causal(treated, baseline)
    assert result["composite_v1_treated"] > result["composite_v1_no_intervention"]
    assert result["delta_vs_no_intervention"] > 0.0


def test_composite_v1_causal_delta_is_zero_when_arms_match():
    from chiyoda.studies.benchmark import composite_v1_causal

    metrics = {
        "mean_travel_time_s": 10.0,
        "p95_hazard_exposure": 0.5,
        "equity_time_gap_s": 2.0,
        "harmful_convergence_index_induced": 0.5,
    }
    result = composite_v1_causal(metrics, metrics)
    assert result["delta_vs_no_intervention"] == 0.0


def test_composite_v1_causal_uses_matched_pair_bundles():
    from chiyoda.studies.benchmark import composite_v1_causal

    baseline = _bundle(
        {
            1: {
                "mean_travel_time_s": 30.0,
                "p95_hazard_exposure": 2.0,
                "equity_time_gap_s": 8.0,
                "harmful_convergence_index_induced": 1.5,
            },
            2: {
                "mean_travel_time_s": 20.0,
                "p95_hazard_exposure": 1.0,
                "equity_time_gap_s": 4.0,
                "harmful_convergence_index_induced": 1.0,
            },
        }
    )
    treated = _bundle(
        {
            1: {
                "mean_travel_time_s": 5.0,
                "p95_hazard_exposure": 0.2,
                "equity_time_gap_s": 1.0,
                "harmful_convergence_index_induced": 0.1,
            },
            2: {
                "mean_travel_time_s": 7.0,
                "p95_hazard_exposure": 0.4,
                "equity_time_gap_s": 1.5,
                "harmful_convergence_index_induced": 0.2,
            },
        }
    )

    result = composite_v1_causal(treated, baseline)

    assert result["composite_v1_treated"] > result["composite_v1_no_intervention"]
    assert result["delta_vs_no_intervention"] > 0.0


def _bundle(rows_by_seed: dict[int, dict[str, float]]) -> StudyBundle:
    summary = pd.DataFrame(
        [
            {
                "study_name": "synthetic",
                "scenario_name": "toy",
                "variant_name": "variant",
                "seed": seed,
                "run_id": f"seed_{seed}",
                "record_type": "run",
                **metrics,
            }
            for seed, metrics in rows_by_seed.items()
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
