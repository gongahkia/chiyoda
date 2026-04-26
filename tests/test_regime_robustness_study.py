from __future__ import annotations

from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.runner import _materialize_variants, _prepare_scenario, load_study_config


def test_regime_robustness_matrix_shape_and_exports():
    config = load_study_config("scenarios/study_regime_robustness.yaml")
    variants = _materialize_variants(config)

    assert len(variants) == 45
    assert len(config.seeds) == 20
    assert config.export.include_figures is False
    assert config.export.table_formats == ["parquet"]

    hazards = set()
    familiarity = set()
    policies = set()
    for variant in variants:
        parts = variant.name.split("__")
        assert len(parts) == 3
        hazard, familiar, policy = parts
        hazards.add(hazard.replace("hazard_", ""))
        familiarity.add(familiar.replace("familiarity_", ""))
        policies.add(policy)

    assert hazards == {"low", "medium", "high"}
    assert familiarity == {"low", "mixed", "high"}
    assert policies == {
        "no_intervention",
        "static_beacon",
        "global_broadcast",
        "entropy_targeted",
        "bottleneck_avoidance",
    }


def test_regime_robustness_variants_build_scenarios():
    config = load_study_config("scenarios/study_regime_robustness.yaml")
    variants = _materialize_variants(config)
    manager = ScenarioManager()

    for variant in variants:
        scenario = _prepare_scenario(manager, config.scenario_file, variant, seed=42)
        assert scenario["simulation"]["random_seed"] == 42
        assert sum(cohort["count"] for cohort in scenario["population"]["cohorts"]) == 120
        assert len(scenario["hazards"]) == 1
        assert scenario["interventions"]["policy"] in {
            "none",
            "static_beacon",
            "global_broadcast",
            "entropy_targeted",
            "bottleneck_avoidance",
        }
