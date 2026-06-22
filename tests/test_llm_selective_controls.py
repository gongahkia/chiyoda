from __future__ import annotations

import json

import pytest
import yaml

from chiyoda.information.llm import (
    AnthropicMessagesGenerator,
    LLMMessageRequest,
    HazardSnapshot,
)
from chiyoda.scenarios.generated_calibration import (
    apply_generated_population_calibration,
)
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.runner import run_study
from chiyoda.studies.schema import ExportConfig, StudyConfig, StudyVariant


def _request() -> LLMMessageRequest:
    return LLMMessageRequest(
        policy="llm_guidance",
        step=1,
        target=(1.0, 1.0),
        selected_reason="test",
        objective="bounded",
        exits=[(3, 1), (1, 1)],
        hazards=[
            HazardSnapshot(
                position=(2.0, 2.0, 0.0), kind="GAS", radius=1.0, severity=0.5
            )
        ],
        congested_exits=[(1, 1)],
        recipients_estimate=2,
        mean_local_density=0.1,
        mean_hazard_load=0.2,
    )


def _scenario() -> dict:
    return {
        "name": "llm_selective_smoke",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXX\nXR@EX\nX.@.X\nXXXXX\n",
                }
            ]
        },
        "population": {"total": 2},
        "responders": [{"count": 2, "release_step": 0}],
        "hazards": [
            {
                "type": "GAS",
                "location": [2.0, 2.0, 0.0],
                "radius": 0.0,
                "severity": 0.4,
                "spread_rate": 0.1,
            }
        ],
        "simulation": {"max_steps": 2, "dt": 0.1, "random_seed": 1},
    }


def test_anthropic_message_generator_parses_mocked_response(monkeypatch):
    class FakeResponse:
        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, tb):
            return False

        def read(self):
            return json.dumps(
                {
                    "id": "msg_test",
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(
                                {
                                    "text": "Use exit",
                                    "recommended_exits": [[3, 1]],
                                    "avoid_exits": [[1, 1]],
                                    "hazard_positions": [[2.0, 2.0, 0.0]],
                                    "confidence": 0.7,
                                    "abstain": False,
                                }
                            ),
                        }
                    ],
                    "usage": {"input_tokens": 12, "output_tokens": 8},
                }
            ).encode("utf-8")

    def fake_urlopen(api_request, timeout):
        headers = {key.lower(): value for key, value in api_request.header_items()}
        body = json.loads(api_request.data.decode("utf-8"))
        assert headers["x-api-key"] == "test-key"
        assert headers["anthropic-version"] == "2023-06-01"
        assert body["max_tokens"] == 500
        assert body["messages"][0]["role"] == "user"
        return FakeResponse()

    monkeypatch.setattr("chiyoda.information.llm.urlrequest.urlopen", fake_urlopen)

    message = AnthropicMessagesGenerator(
        model="claude-test", api_key="test-key"
    ).generate(
        _request(),
        "cache-key",
    )

    assert message.provider == "anthropic"
    assert message.text == "Use exit"
    assert message.recommended_exits == [(3, 1)]
    assert message.raw_response["usage"]["input_tokens"] == 12


def test_persona_conditioned_population_generation_is_bounded(tmp_path):
    scenario = {
        "name": "persona_population",
        "layout": {"floors": [{"id": "0", "z": 0.0, "text": "XXXXX\nX@.EX\nXXXXX\n"}]},
        "population": {"total": 5, "cohorts": []},
        "generated_population_calibration": {
            "enabled": True,
            "provider": "template",
            "cache_path": str(tmp_path),
            "allowed_targets": ["cohort_mix", "parameter_priors", "scenario_metadata"],
            "personas": ["regular wheelchair", "visitor family"],
        },
    }

    updated = apply_generated_population_calibration(scenario)
    cohorts = updated["population"]["cohorts"]
    audit = updated["metadata"]["generated_population_calibration_audit"]

    assert sum(cohort["count"] for cohort in cohorts) == 5
    assert {cohort["persona_condition"] for cohort in cohorts} == {
        "regular wheelchair",
        "visitor family",
    }
    assert any(cohort["mobility_class"] == "wheelchair" for cohort in cohorts)
    assert audit["validation_status"] == "accepted"
    assert audit["persona_conditions"] == ["regular wheelchair", "visitor family"]


def test_llm_guidance_budget_guard_blocks_cache_miss(tmp_path):
    scenario = _scenario()
    scenario["interventions"] = {
        "policy": "llm_guidance",
        "llm_provider": "template",
        "llm_cache_path": str(tmp_path / "cache"),
        "llm_max_calls_per_run": 0,
        "interval_steps": 1,
        "message_radius": 10.0,
    }
    sim = ScenarioManager().build_simulation(scenario)
    sim.run()

    assert sim.intervention_events
    assert sim.intervention_events[0].cache_status == "budget_exceeded"
    assert sim.intervention_events[0].used_fallback is True
    assert any(row["provider"] == "budget_guard" for row in sim.llm_call_audit)


def test_llm_responder_coordination_uses_responder_targets(tmp_path):
    scenario = _scenario()
    scenario["interventions"] = {
        "policy": "llm_responder_coordination",
        "llm_provider": "template",
        "llm_cache_path": str(tmp_path / "cache"),
        "interval_steps": 1,
        "message_radius": 10.0,
    }
    sim = ScenarioManager().build_simulation(scenario)
    sim.run()

    assert sim.intervention_events
    event = sim.intervention_events[0]
    assert event.policy == "llm_responder_coordination"
    assert event.selected_reason == "llm_responder_coordination_entropy_field"
    assert sim.llm_call_audit[0]["surface"] == "intervention"


def test_study_bundle_surfaces_llm_call_audit(tmp_path):
    scenario = _scenario()
    scenario["interventions"] = {
        "policy": "llm_guidance",
        "llm_provider": "template",
        "llm_cache_path": str(tmp_path / "cache"),
        "interval_steps": 1,
        "message_radius": 10.0,
    }
    scenario_path = tmp_path / "scenario.yaml"
    scenario_path.write_text(yaml.safe_dump({"scenario": scenario}))
    config = StudyConfig(
        name="llm_calls_study",
        scenario_file=str(scenario_path),
        seeds=[1],
        variants=[StudyVariant(name="base")],
        export=ExportConfig(include_figures=False, table_formats=["csv"]),
    )

    bundle = run_study(config)

    assert not bundle.llm_calls.empty
    assert {"surface", "provider", "cache_key", "cache_status"}.issubset(
        bundle.llm_calls.columns
    )
