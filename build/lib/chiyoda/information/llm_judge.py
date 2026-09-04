"""Optional LLM-as-judge validator.

A second-pass scoring hook that takes a generation record (the request,
the message produced by the policy provider, and ground-truth context
from the simulation) and produces ``(safety, specificity, alignment)``
scores in ``[0, 1]``. If the minimum score falls below the configured
threshold, the verdict's ``accepted`` flag is ``False`` and the caller
should drop the message.

This module is intentionally callable both with an external provider
(another LLM call) and with the bundled heuristic scorer
:func:`heuristic_judge`, which is deterministic and used in tests.

The verdict is dataclass-serializable via :meth:`JudgeVerdict.to_dict`
so callers can fold it into the existing ``llm_calls`` study export
schema without modifying the core ``LLMGenerationRecord`` dataclass.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import asdict, dataclass, field
from typing import Any, Protocol

from chiyoda.information.llm import attack_pattern_reasons


@dataclass
class JudgeVerdict:
    accepted: bool
    safety: float
    specificity: float
    alignment: float
    threshold: float
    reasons: list[str] = field(default_factory=list)
    provider: str = "heuristic"

    @property
    def min_score(self) -> float:
        return min(self.safety, self.specificity, self.alignment)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class JudgeProtocol(Protocol):
    def __call__(
        self, *, request: Any, message: Any, ground_truth: dict[str, Any]
    ) -> tuple[float, float, float, list[str]]:
        """Return ``(safety, specificity, alignment, reasons)``."""


def _clip(value: float) -> float:
    if value < 0.0:
        return 0.0
    if value > 1.0:
        return 1.0
    return float(value)


def heuristic_judge(
    *, request: Any, message: Any, ground_truth: dict[str, Any]
) -> tuple[float, float, float, list[str]]:
    """Deterministic stand-in for an LLM judge.

    Heuristic signals:

    - **safety**: penalises empty text, panic words, and contradicting
      hazard positions reported by the simulation.
    - **specificity**: rewards recommending at least one concrete exit
      and avoiding generic phrasing like "go somewhere safe".
    - **alignment**: rewards recommended exits that actually exist in
      ``ground_truth['exits']`` and penalises recommendations that
      coincide with a known congested exit.
    """
    reasons: list[str] = []

    text = (getattr(message, "text", "") or "").lower().strip()
    recommended = list(getattr(message, "recommended_exits", []) or [])
    avoid = list(getattr(message, "avoid_exits", []) or [])

    safety = 1.0
    if not text:
        safety -= 0.7
        reasons.append("empty_text")
    panic_terms = ("run", "panic", "die", "trapped")
    if any(term in text for term in panic_terms):
        safety -= 0.4
        reasons.append("panic_terms")
    attack_reasons = attack_pattern_reasons(text)
    if attack_reasons:
        safety -= 0.5
        reasons.extend(attack_reasons)

    specificity = 1.0
    if not recommended:
        specificity -= 0.6
        reasons.append("no_recommended_exit")
    if "somewhere safe" in text or "safe area" in text:
        specificity -= 0.3
        reasons.append("vague_phrasing")

    alignment = 1.0
    truth_exits = {tuple(exit_) for exit_ in ground_truth.get("exits", [])}
    if truth_exits and recommended:
        unknown = [
            tuple(exit_) for exit_ in recommended if tuple(exit_) not in truth_exits
        ]
        if unknown:
            alignment -= 0.5
            reasons.append("unknown_recommended_exit")
    congested = {tuple(exit_) for exit_ in ground_truth.get("congested_exits", [])}
    if congested and any(tuple(exit_) in congested for exit_ in recommended):
        alignment -= 0.4
        reasons.append("recommended_congested_exit")
    if avoid and ground_truth.get("hazards"):
        # avoiding hazards is good
        alignment = min(1.0, alignment + 0.1)

    return _clip(safety), _clip(specificity), _clip(alignment), reasons


def judge(
    *,
    request: Any,
    message: Any,
    ground_truth: dict[str, Any] | None = None,
    threshold: float = 0.4,
    judger: Callable[..., tuple[float, float, float, list[str]]] | None = None,
    provider: str = "heuristic",
) -> JudgeVerdict:
    """Score a generation and return a :class:`JudgeVerdict`.

    Pass ``judger=`` to call an external provider; defaults to
    :func:`heuristic_judge`. Threshold is applied to ``min(safety,
    specificity, alignment)``.
    """
    truth = ground_truth or {}
    score_fn = judger or heuristic_judge
    safety, specificity, alignment, reasons = score_fn(
        request=request, message=message, ground_truth=truth
    )
    verdict = JudgeVerdict(
        accepted=min(safety, specificity, alignment) >= threshold,
        safety=safety,
        specificity=specificity,
        alignment=alignment,
        threshold=threshold,
        reasons=reasons,
        provider=provider,
    )
    return verdict
