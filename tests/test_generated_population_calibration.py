from __future__ import annotations

import pytest

from chiyoda.scenarios.generated_calibration import (
    GeneratedPopulationCalibration,
    PopulationCalibrationCache,
    PopulationCalibrationConfig,
    PopulationCalibrationRecord,
    PopulationCalibrationRequest,
    PopulationCalibrationValidation,
    TemplatePopulationCalibrationGenerator,
    apply_generated_population_calibration,
    validate_generated_population_calibration,
)
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.runner import _materialize_variants, _prepare_scenario, load_study_config


def _request() -> PopulationCalibrationRequest:
    return PopulationCalibrationRequest(
        scenario_name="calibration_test",
        objective="fill_missing",
        prompt_style="conservative",
        allowed_targets=("parameter_priors", "scenario_metadata"),
        population_total=2,
        existing_cohorts=(
            {"name": "regulars", "count": 1, "base_speed": 1.31},
            {"name": "visitors", "count": 1},
        ),
        hazard_count=1,
        responder_count=0,
    )


def test_population_calibration_cache_round_trips_record(tmp_path):
    cache = PopulationCalibrationCache(tmp_path)
    request = _request()
    key = cache.key_for(request)
    calibration = GeneratedPopulationCalibration(
        parameter_priors={"regulars": {"base_rationality": 0.8}},
        confidence=0.7,
    )
    record = PopulationCalibrationRecord(
        cache_key=key,
        request=request,
        calibration=calibration,
        validation=PopulationCalibrationValidation(accepted=True),
    )

    cache.store(record)
    loaded = cache.load(key)

    assert loaded is not None
    assert loaded.cache_key == key
    assert loaded.calibration.parameter_priors["regulars"]["base_rationality"] == pytest.approx(0.8)
    assert loaded.validation.accepted


def test_generated_calibration_fills_missing_fields_without_overwriting_existing(tmp_path):
    scenario = {
        "name": "generated_prior_smoke",
        "layout": {"text": "XXXXX\nX@.EX\nXXXXX\n"},
        "population": {
            "total": 1,
            "cohorts": [
                {
                    "name": "regulars",
                    "count": 1,
                    "base_speed": 1.11,
                }
            ],
        },
        "generated_population_calibration": {
            "enabled": True,
            "provider": "template",
            "cache_path": str(tmp_path),
            "allowed_targets": ["parameter_priors", "scenario_metadata"],
        },
    }

    updated = apply_generated_population_calibration(scenario)
    cohort = updated["population"]["cohorts"][0]
    audit = updated["metadata"]["generated_population_calibration_audit"]

    assert cohort["base_speed"] == pytest.approx(1.11)
    assert cohort["base_rationality"] == pytest.approx(0.8)
    assert cohort["parameter_provenance"]["base_rationality"].startswith("generated:")
    assert audit["validation_status"] == "accepted"
    assert any(item.endswith("regulars.base_speed:existing_value") for item in audit["skipped"])
    cache_record = PopulationCalibrationCache(tmp_path).load(audit["cache_key"])
    assert cache_record is not None
    assert any(
        item.endswith("regulars.base_speed:existing_value")
        for item in cache_record.application["skipped"]
    )


def test_replay_generated_calibration_uses_existing_cache(tmp_path):
    base = {
        "name": "generated_replay_smoke",
        "layout": {"text": "XXXXX\nX@.EX\nXXXXX\n"},
        "population": {
            "total": 1,
            "cohorts": [{"name": "visitors", "count": 1}],
        },
        "generated_population_calibration": {
            "enabled": True,
            "provider": "template",
            "cache_path": str(tmp_path),
            "allowed_targets": ["parameter_priors", "scenario_metadata"],
        },
    }
    populated = apply_generated_population_calibration(base)
    replay = {
        **base,
        "generated_population_calibration": {
            **base["generated_population_calibration"],
            "provider": "replay",
            "cache_mode": "replay_only",
            "store_cache": False,
        },
    }

    replayed = apply_generated_population_calibration(replay)

    assert populated["population"]["cohorts"][0]["base_rationality"] == pytest.approx(0.6)
    assert replayed["population"]["cohorts"][0]["base_rationality"] == pytest.approx(0.6)
    assert replayed["metadata"]["generated_population_calibration_audit"]["cache_status"] == "hit"


def test_generated_calibration_rejects_disallowed_target():
    base_request = _request()
    request = PopulationCalibrationRequest(
        scenario_name=base_request.scenario_name,
        objective=base_request.objective,
        prompt_style=base_request.prompt_style,
        allowed_targets=("scenario_metadata",),
        population_total=base_request.population_total,
        existing_cohorts=base_request.existing_cohorts,
        hazard_count=base_request.hazard_count,
        responder_count=base_request.responder_count,
    )
    config = PopulationCalibrationConfig(
        enabled=True,
        allowed_targets=("scenario_metadata",),
    )
    calibration = GeneratedPopulationCalibration(
        parameter_priors={"regulars": {"base_rationality": 0.8}},
        confidence=0.7,
    )

    result = validate_generated_population_calibration(calibration, request, config)

    assert not result.accepted
    assert "disallowed_target:parameter_priors" in result.reasons


def test_generated_cohort_mix_only_materializes_when_no_authored_cohorts(tmp_path):
    scenario = {
        "name": "generated_cohort_mix",
        "layout": {"text": "XXXXX\nX@.EX\nXXXXX\n"},
        "population": {"total": 4, "cohorts": []},
        "generated_population_calibration": {
            "enabled": True,
            "provider": "template",
            "cache_path": str(tmp_path),
            "allowed_targets": ["cohort_mix", "parameter_priors", "scenario_metadata"],
        },
    }

    updated = apply_generated_population_calibration(scenario)

    assert sum(cohort["count"] for cohort in updated["population"]["cohorts"]) == 4
    assert {cohort["calibration_status"] for cohort in updated["population"]["cohorts"]} == {
        "generated_heuristic_prior"
    }


def test_scenario_manager_applies_generated_population_before_building_agents(tmp_path):
    scenario = {
        "name": "manager_generated_prior_smoke",
        "layout": {"text": "XXXXX\nX@.EX\nXXXXX\n"},
        "population": {
            "total": 1,
            "cohorts": [{"name": "visitors", "count": 1}],
        },
        "generated_population_calibration": {
            "enabled": True,
            "provider": "template",
            "cache_path": str(tmp_path),
            "allowed_targets": ["parameter_priors", "scenario_metadata"],
        },
    }

    sim = ScenarioManager().build_simulation(scenario)

    assert sim.agents[0].base_rationality == pytest.approx(0.6)
    assert sim.agents[0].base_speed == pytest.approx(1.2)


def test_generated_population_calibration_study_pairs_template_and_replay():
    config = load_study_config("scenarios/study_generated_population_calibration.yaml")
    variants = _materialize_variants(config)
    manager = ScenarioManager()

    assert [variant.name for variant in variants] == [
        "template_missing_priors",
        "replay_missing_priors",
        "template_missing_cohort_mix",
    ]

    template = _prepare_scenario(manager, config.scenario_file, variants[0], seed=42)
    replay = _prepare_scenario(manager, config.scenario_file, variants[1], seed=42)
    cohort_mix = _prepare_scenario(manager, config.scenario_file, variants[2], seed=42)

    assert template["generated_population_calibration"]["provider"] == "template"
    assert replay["generated_population_calibration"]["provider"] == "replay"
    assert template["generated_population_calibration"]["cache_path"] == replay["generated_population_calibration"]["cache_path"]
    assert cohort_mix["generated_population_calibration"]["allowed_targets"][0] == "cohort_mix"
    assert cohort_mix["population"]["cohorts"] == []
