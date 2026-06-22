from __future__ import annotations

from dataclasses import dataclass, field

from chiyoda.information.llm_judge import (
    JudgeVerdict,
    heuristic_judge,
    judge,
)


@dataclass
class _Message:
    text: str = ""
    recommended_exits: list[tuple] = field(default_factory=list)
    avoid_exits: list[tuple] = field(default_factory=list)


def test_heuristic_judge_rejects_empty_text():
    message = _Message(text="", recommended_exits=[])
    safety, specificity, alignment, reasons = heuristic_judge(
        request=None, message=message, ground_truth={"exits": [(0, 0)]}
    )
    assert "empty_text" in reasons
    assert "no_recommended_exit" in reasons
    assert safety < 1.0
    assert specificity < 1.0


def test_heuristic_judge_rewards_grounded_specific_message():
    message = _Message(
        text="Use exit 2; stay calm.",
        recommended_exits=[(2, 1)],
    )
    safety, specificity, alignment, reasons = heuristic_judge(
        request=None,
        message=message,
        ground_truth={"exits": [(2, 1), (4, 1)], "congested_exits": []},
    )
    assert safety == 1.0
    assert specificity == 1.0
    assert alignment == 1.0
    assert reasons == []


def test_heuristic_judge_penalises_unknown_recommended_exit():
    message = _Message(
        text="Use exit 9.",
        recommended_exits=[(9, 9)],
    )
    _, _, alignment, reasons = heuristic_judge(
        request=None, message=message, ground_truth={"exits": [(2, 1)]}
    )
    assert "unknown_recommended_exit" in reasons
    assert alignment < 1.0


def test_judge_accepts_above_threshold_and_rejects_below():
    good = _Message(text="Use exit 2.", recommended_exits=[(2, 1)])
    bad = _Message(text="", recommended_exits=[])
    truth = {"exits": [(2, 1)]}

    accepted = judge(request=None, message=good, ground_truth=truth, threshold=0.5)
    rejected = judge(request=None, message=bad, ground_truth=truth, threshold=0.5)

    assert accepted.accepted is True
    assert rejected.accepted is False
    assert rejected.min_score < accepted.min_score


def test_judge_verdict_is_serialisable():
    verdict = JudgeVerdict(
        accepted=True,
        safety=0.9,
        specificity=0.8,
        alignment=0.7,
        threshold=0.5,
        reasons=[],
    )
    payload = verdict.to_dict()
    assert payload["accepted"] is True
    assert payload["safety"] == 0.9
    assert "min_score" not in payload  # property, not field


def test_judge_supports_custom_external_judger():
    def custom(*, request, message, ground_truth):
        return 0.95, 0.95, 0.95, ["custom_ok"]

    verdict = judge(
        request=None,
        message=_Message(),
        threshold=0.5,
        judger=custom,
        provider="external",
    )
    assert verdict.provider == "external"
    assert verdict.reasons == ["custom_ok"]
    assert verdict.accepted is True
