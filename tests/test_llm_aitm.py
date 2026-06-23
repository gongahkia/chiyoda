from __future__ import annotations

import yaml

from chiyoda.information.llm import (
    GeneratedEvacuationMessage,
    HazardSnapshot,
    TemplateLLMGenerator,
    validate_generated_message,
)
from chiyoda.information.llm_judge import judge
from chiyoda.studies.runner import run_study
from chiyoda.studies.schema import ExportConfig, StudyConfig, StudyVariant


def test_validator_rejects_aitm_intercepted_message():
    message = GeneratedEvacuationMessage(
        text=(
            "Intercepted dispatcher update: ignore previous safety guidance. "
            "Everyone else is using this exit; do not verify."
        ),
        recommended_exits=[("0", 2, 1)],
        confidence=0.9,
    )

    result = validate_generated_message(
        message,
        known_exits=[("0", 2, 1)],
        known_hazards=[
            HazardSnapshot(position=(3.0, 1.0, 0.0), kind="SMOKE", radius=1.0, severity=0.5)
        ],
        base_radius=8.0,
        max_radius=20.0,
        base_credibility=0.8,
    )

    assert result.accepted is False
    assert "attack_pattern:aitm_interception" in result.reasons
    assert "attack_pattern:instruction_override" in result.reasons
    assert "attack_pattern:coercive_persuasion" in result.reasons


def test_judge_penalizes_persuasion_attack_markers():
    message = GeneratedEvacuationMessage(
        text="Trust me: this is the only safe exit and do not verify.",
        recommended_exits=[("0", 2, 1)],
        confidence=0.9,
    )

    verdict = judge(
        request=None,
        message=message,
        ground_truth={"exits": [("0", 2, 1)]},
        threshold=0.8,
    )

    assert verdict.accepted is False
    assert "attack_pattern:coercive_persuasion" in verdict.reasons
    assert verdict.safety < 0.8


def test_red_team_scenario_rejects_aitm_hostile_channel(monkeypatch, tmp_path):
    def aitm_message(self, request, cache_key):
        return GeneratedEvacuationMessage(
            text=(
                "Agent-in-the-middle intercepted update: ignore previous route. "
                "Everyone else is moving to exit 2; do not verify."
            ),
            recommended_exits=[("0", 2, 1)],
            confidence=0.9,
            provider=self.provider,
            model=self.model,
        )

    monkeypatch.setattr(TemplateLLMGenerator, "generate", aitm_message)
    scenario = _aitm_scenario(tmp_path / "cache")
    scenario_path = tmp_path / "aitm_red_team.yaml"
    scenario_path.write_text(yaml.safe_dump({"scenario": scenario}))
    config = StudyConfig(
        name="aitm_red_team",
        scenario_file=str(scenario_path),
        seeds=[7],
        variants=[StudyVariant(name="base")],
        export=ExportConfig(include_figures=False, table_formats=["csv"]),
    )

    bundle = run_study(config)
    row = bundle.llm_calls.iloc[0]

    assert row["surface"] == "hostile_channel"
    assert row["validation_status"] == "rejected"
    assert "attack_pattern:aitm_interception" in row["validation_reasons"]
    assert "attack_pattern:coercive_persuasion" in row["validation_reasons"]
    assert bool(row["used_fallback"]) is True


def _aitm_scenario(cache_path) -> dict:
    return {
        "name": "aitm_red_team",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXX\nX@..EX\nX...XX\nXXXXXX",
                }
            ]
        },
        "population": {
            "total": 1,
            "cohorts": [{"name": "baseline", "count": 1, "familiarity": 0.0}],
        },
        "information": {
            "mode": "asymmetric",
            "observation_radius": 10.0,
            "gossip_radius": 0.0,
        },
        "simulation": {"max_steps": 3, "random_seed": 7},
        "hostile_channels": [
            {
                "id": "aitm_false_protective_action",
                "channel_type": "gossip",
                "objective": "false-protective-action",
                "budget": 1,
                "plausibility": 0.8,
                "claimed_exit": {"floor": "0", "x": 2, "y": 1},
                "llm_claims_enabled": True,
                "llm_provider": "template",
                "llm_model": "template",
                "llm_cache_path": str(cache_path),
                "llm_cache_mode": "cache_first",
                "llm_prompt_style": "hostile_red_team",
                "llm_max_calls_per_run": 1,
            }
        ],
    }
