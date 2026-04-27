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


def test_llm_extension_pilot_is_optional_and_replayable():
    config = load_study_config("scenarios/study_llm_extension.yaml")
    variants = _materialize_variants(config)
    manager = ScenarioManager()

    assert len(variants) == 4
    assert len(config.seeds) == 3
    assert config.export.include_figures is False

    scenario = _prepare_scenario(manager, config.scenario_file, variants[-1], seed=42)
    interventions = scenario["interventions"]
    assert interventions["policy"] == "llm_guidance"
    assert interventions["llm_provider"] == "template"
    assert interventions["llm_cache_path"] == "out/llm_cache/template"
    assert interventions["llm_max_radius"] == 8.0


def test_openai_llm_pilot_has_cache_populate_and_replay_variants():
    config = load_study_config("scenarios/study_llm_openai_pilot.yaml")
    variants = _materialize_variants(config)
    manager = ScenarioManager()

    assert len(variants) == 4
    assert config.seeds == [42]
    assert config.export.include_figures is False

    openai = _prepare_scenario(manager, config.scenario_file, variants[2], seed=42)
    replay = _prepare_scenario(manager, config.scenario_file, variants[3], seed=42)

    assert openai["interventions"]["policy"] == "llm_guidance"
    assert openai["interventions"]["llm_provider"] == "openai"
    assert openai["interventions"]["llm_cache_mode"] == "cache_first"
    assert replay["interventions"]["llm_provider"] == "replay"
    assert replay["interventions"]["llm_cache_mode"] == "replay_only"
    assert openai["interventions"]["llm_cache_path"] == replay["interventions"]["llm_cache_path"]


def test_llm_medium_study_has_prompt_and_validator_ablations():
    config = load_study_config("scenarios/study_llm_medium.yaml")
    variants = _materialize_variants(config)
    manager = ScenarioManager()

    assert len(variants) == 8
    assert len(config.seeds) == 10
    assert config.export.include_figures is False

    prompt_styles = set()
    validator_profiles = set()
    providers = set()
    for variant in variants:
        scenario = _prepare_scenario(manager, config.scenario_file, variant, seed=42)
        interventions = scenario.get("interventions", {})
        if interventions.get("policy") == "llm_guidance":
            prompt_styles.add(interventions["llm_prompt_style"])
            validator_profiles.add(interventions["llm_validator_profile"])
            providers.add(interventions["llm_provider"])

    assert {"state_only", "safety", "entropy"}.issubset(prompt_styles)
    assert {"standard", "strict", "lenient"}.issubset(validator_profiles)
    assert {"template", "openai", "replay"}.issubset(providers)
