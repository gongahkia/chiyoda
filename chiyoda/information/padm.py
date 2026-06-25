"""Protective Action Decision Model stage hooks."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

PADM_RECEIVE = "receive"
PADM_UNDERSTAND = "understand"
PADM_PERSONALIZE = "personalize"
PADM_DECIDE = "decide"
PADM_STAGES = (PADM_RECEIVE, PADM_UNDERSTAND, PADM_PERSONALIZE, PADM_DECIDE)
PADM_COUNTER_ATTRS = {
    PADM_RECEIVE: "_padm_receive_count",
    PADM_UNDERSTAND: "_padm_understand_count",
    PADM_PERSONALIZE: "_padm_personalize_count",
    PADM_DECIDE: "_padm_decide_count",
}
PADM_TELEMETRY_FIELDS = {
    PADM_RECEIVE: "padm_receive",
    PADM_UNDERSTAND: "padm_understand",
    PADM_PERSONALIZE: "padm_personalize",
    PADM_DECIDE: "padm_decide",
}


@dataclass(frozen=True)
class PADMStageConfig:
    """Runtime gate for independently muting PADM stages."""

    enabled_stages: tuple[str, ...] = PADM_STAGES

    def __post_init__(self) -> None:
        normalized = tuple(_normalize_stage(stage) for stage in self.enabled_stages)
        unknown = sorted(set(normalized) - set(PADM_STAGES))
        if unknown:
            raise ValueError(f"Unknown PADM stage(s): {', '.join(unknown)}")
        object.__setattr__(self, "enabled_stages", normalized)

    @classmethod
    def from_enabled(
        cls, stages: tuple[str, ...] | list[str] | None
    ) -> PADMStageConfig:
        if stages is None:
            return cls()
        return cls(tuple(stages))

    @classmethod
    def with_muted(cls, *stages: str) -> PADMStageConfig:
        muted = {_normalize_stage(stage) for stage in stages}
        return cls(tuple(stage for stage in PADM_STAGES if stage not in muted))

    def is_enabled(self, stage: str) -> bool:
        return _normalize_stage(stage) in set(self.enabled_stages)


def padm_stage_enabled(config: PADMStageConfig | None, stage: str) -> bool:
    if config is None:
        return True
    return config.is_enabled(stage)


def record_padm_stage(agent: Any, stage: str) -> int:
    attr = PADM_COUNTER_ATTRS[_normalize_stage(stage)]
    value = int(getattr(agent, attr, 0)) + 1
    setattr(agent, attr, value)
    return value


def padm_counter_values(agent: Any) -> dict[str, int]:
    return {
        PADM_TELEMETRY_FIELDS[stage]: int(getattr(agent, attr, 0))
        for stage, attr in PADM_COUNTER_ATTRS.items()
    }


def _normalize_stage(stage: str) -> str:
    normalized = str(stage).strip().lower()
    if normalized not in PADM_COUNTER_ATTRS:
        raise ValueError(f"Unknown PADM stage: {stage}")
    return normalized
