from __future__ import annotations

from chiyoda.information.llm import (
    GeneratedEvacuationMessage,
    HazardSnapshot,
    LLMGenerationRecord,
    LLMMessageCache,
    LLMMessageRequest,
    TemplateLLMGenerator,
    ValidationResult,
    validate_generated_message,
)


def _request() -> LLMMessageRequest:
    return LLMMessageRequest(
        policy="llm_guidance",
        step=10,
        target=(5.0, 5.0),
        selected_reason="test",
        objective="reduce_entropy_without_bottlenecking",
        exits=[(10, 1), (1, 1)],
        hazards=[HazardSnapshot(position=(3.0, 3.0), kind="GAS", radius=2.0, severity=0.8)],
        congested_exits=[(1, 1)],
        recipients_estimate=4,
        mean_local_density=0.3,
        mean_hazard_load=0.1,
    )


def test_llm_message_cache_round_trips_record(tmp_path):
    cache = LLMMessageCache(tmp_path)
    request = _request()
    key = cache.key_for(request)
    message = GeneratedEvacuationMessage(
        text="Use the east exit.",
        recommended_exits=[(10, 1)],
        avoid_exits=[(1, 1)],
        hazard_positions=[(3.0, 3.0)],
        confidence=0.7,
    )
    record = LLMGenerationRecord(
        cache_key=key,
        request=request,
        message=message,
        validation=ValidationResult(accepted=True),
    )

    cache.store(record)
    loaded = cache.load(key)

    assert loaded is not None
    assert loaded.cache_key == key
    assert loaded.message.recommended_exits == [(10, 1)]
    assert loaded.validation.accepted


def test_template_generator_avoids_congested_exit():
    request = _request()
    message = TemplateLLMGenerator().generate(request, "cache-key")

    assert message.provider == "deterministic"
    assert message.recommended_exits == [(10, 1)]
    assert message.avoid_exits == [(1, 1)]


def test_validator_rejects_invented_exit_and_hazard():
    message = GeneratedEvacuationMessage(
        recommended_exits=[(99, 99)],
        hazard_positions=[(50.0, 50.0)],
        confidence=0.9,
    )
    result = validate_generated_message(
        message,
        known_exits=[(10, 1)],
        known_hazards=[HazardSnapshot(position=(3.0, 3.0), kind="GAS", radius=2.0, severity=0.8)],
        base_radius=8.0,
        max_radius=20.0,
        base_credibility=0.9,
    )

    assert not result.accepted
    assert any(reason.startswith("invented_exit") for reason in result.reasons)
    assert any(reason.startswith("invented_hazard") for reason in result.reasons)


def test_validator_rejects_overconfident_message():
    message = GeneratedEvacuationMessage(
        recommended_exits=[(10, 1)],
        credibility=1.0,
    )
    result = validate_generated_message(
        message,
        known_exits=[(10, 1)],
        known_hazards=[],
        base_radius=8.0,
        max_radius=20.0,
        base_credibility=0.8,
    )

    assert not result.accepted
    assert "unsafe_credibility:1.0" in result.reasons
