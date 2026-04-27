from __future__ import annotations

from chiyoda.information.llm import (
    GeneratedEvacuationMessage,
    HazardSnapshot,
    LLMGenerationRecord,
    LLMMessageCache,
    LLMMessageRequest,
    OpenAIResponsesGenerator,
    TemplateLLMGenerator,
    ValidationResult,
    build_prompt_instructions,
    load_openai_api_key,
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
        prompt_style="safety",
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


def test_prompt_style_changes_cache_key_and_instructions():
    safety = _request()
    minimal = LLMMessageRequest(
        policy=safety.policy,
        step=safety.step,
        target=safety.target,
        selected_reason=safety.selected_reason,
        objective=safety.objective,
        exits=safety.exits,
        hazards=safety.hazards,
        congested_exits=safety.congested_exits,
        recipients_estimate=safety.recipients_estimate,
        mean_local_density=safety.mean_local_density,
        mean_hazard_load=safety.mean_hazard_load,
        prompt_style="minimal",
    )
    cache = LLMMessageCache("/tmp/unused")

    assert cache.key_for(safety) != cache.key_for(minimal)
    assert "reducing hazard exposure" in build_prompt_instructions("safety")
    assert "smallest possible instruction" in build_prompt_instructions("minimal")


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


def test_validator_rejects_conflicting_and_congested_exit_guidance():
    message = GeneratedEvacuationMessage(
        text="Use exit 10 1 and avoid exit 10 1.",
        recommended_exits=[(10, 1)],
        avoid_exits=[(10, 1)],
        confidence=0.7,
    )
    result = validate_generated_message(
        message,
        known_exits=[(10, 1), (1, 1)],
        known_hazards=[],
        base_radius=8.0,
        max_radius=20.0,
        base_credibility=0.9,
        congested_exits=[(10, 1)],
    )

    assert not result.accepted
    assert "conflicting_exit:(10, 1)" in result.reasons
    assert "congested_recommendation:(10, 1)" in result.reasons


def test_validator_rejects_vague_and_low_confidence_guidance():
    message = GeneratedEvacuationMessage(
        text="Stay calm",
        recommended_exits=[(10, 1)],
        confidence=0.1,
    )
    result = validate_generated_message(
        message,
        known_exits=[(10, 1)],
        known_hazards=[],
        base_radius=8.0,
        max_radius=20.0,
        base_credibility=0.9,
    )

    assert not result.accepted
    assert "vague_guidance" in result.reasons
    assert "low_confidence:0.10" in result.reasons


def test_openai_api_key_loader_accepts_hyphenated_env_file(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.delenv("OPENAI-API-KEY", raising=False)
    monkeypatch.delenv("OPEN-AI-API-KEY", raising=False)
    env_file = tmp_path / ".env"
    env_file.write_text("OPENAI-API-KEY='test-key'\n")

    assert load_openai_api_key(env_file) == "test-key"


def test_openai_generator_parses_mocked_responses(monkeypatch):
    class FakeResponse:
        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, tb):
            return False

        def read(self):
            return (
                b'{"id":"resp_test","output":[{"content":[{"text":"{'
                b'\\"text\\":\\"Use exit\\",'
                b'\\"recommended_exits\\":[[10,1]],'
                b'\\"avoid_exits\\":[[1,1]],'
                b'\\"hazard_positions\\":[[3.0,3.0]],'
                b'\\"confidence\\":0.7,'
                b'\\"abstain\\":false}"}]}]}'
            )

    def fake_urlopen(api_request, timeout):
        assert api_request.headers["Authorization"].endswith("test-key")
        return FakeResponse()

    monkeypatch.setattr("chiyoda.information.llm.urlrequest.urlopen", fake_urlopen)
    message = OpenAIResponsesGenerator(model="test-model", api_key="test-key").generate(
        _request(),
        "cache-key",
    )

    assert message.provider == "openai"
    assert message.model == "test-model"
    assert message.text == "Use exit"
    assert message.recommended_exits == [(10, 1)]
    assert message.avoid_exits == [(1, 1)]
