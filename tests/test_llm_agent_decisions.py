from __future__ import annotations

import json

from chiyoda.agents.base import INTENTION_EVACUATE
from chiyoda.information.decisions import (
    GeneratedAgentDecision,
    LLMDecisionCache,
    LLMDecisionRecord,
    LLMDecisionRequest,
    TemplateDecisionGenerator,
    validate_agent_decision,
)
from chiyoda.information.llm import HazardSnapshot, ValidationResult
from chiyoda.scenarios.manager import ScenarioManager


def _request() -> LLMDecisionRequest:
    return LLMDecisionRequest(
        step=1,
        agent_id=7,
        current_intent=INTENTION_EVACUATE,
        objective="bounded_agent_decision",
        known_exits=[(10, 1), (1, 1)],
        congested_exits=[(1, 1)],
        hazards=[
            HazardSnapshot(position=(3.0, 3.0), kind="SMOKE", radius=2.0, severity=0.5)
        ],
        local_density=0.2,
        hazard_load=0.1,
        entropy=0.4,
    )


def test_llm_decision_cache_round_trips_record(tmp_path):
    cache = LLMDecisionCache(tmp_path)
    request = _request()
    key = cache.key_for(request)
    decision = GeneratedAgentDecision(
        intent=INTENTION_EVACUATE,
        target_exit=(10, 1),
        rationale="known_exit_available",
        confidence=0.7,
    )
    cache.store(
        LLMDecisionRecord(
            cache_key=key,
            request=request,
            decision=decision,
            validation=ValidationResult(accepted=True),
        )
    )

    loaded = cache.load(key)

    assert loaded is not None
    assert loaded.cache_key == key
    assert loaded.decision.target_exit == (10, 1)
    assert loaded.validation.accepted


def test_llm_decision_validator_rejects_unbounded_action():
    result = validate_agent_decision(
        GeneratedAgentDecision(
            intent="TELEPORT",
            target_exit=(99, 99),
            trust_delta=0.9,
            rationale="bad",
            confidence=0.9,
        ),
        request=_request(),
        min_confidence=0.2,
        max_trust_delta=0.2,
    )

    assert not result.accepted
    assert "unsupported_intent:TELEPORT" in result.reasons
    assert "unknown_target_exit:(99, 99)" in result.reasons
    assert "unsafe_trust_delta:0.9" in result.reasons


def test_template_decision_avoids_congested_exit():
    decision = TemplateDecisionGenerator().generate(_request(), "cache-key")

    assert decision.intent == INTENTION_EVACUATE
    assert decision.target_exit == (10, 1)
    assert decision.provider == "deterministic"


def test_scenario_llm_decisions_emit_telemetry(tmp_path):
    scenario = ScenarioManager().load_config("scenarios/example.yaml")
    scenario["llm_decisions"] = {
        "enabled": True,
        "provider": "template",
        "cache_path": str(tmp_path / "cache"),
        "interval_steps": 1,
        "agent_budget_per_interval": 2,
    }
    scenario["simulation"]["max_steps"] = 2
    sim = ScenarioManager().build_simulation(scenario)
    sim.run()

    assert sim.agent_decision_events
    assert sim.agent_decision_events[0].validation_status == "accepted"
    assert sim.agent_decision_events[0].selected_intent in {
        "EVACUATE",
        "EXPLORE",
        "FOLLOW",
    }
    assert list((tmp_path / "cache").glob("*.json"))
    assert json.loads(next((tmp_path / "cache").glob("*.json")).read_text())[
        "validation"
    ]["accepted"]
