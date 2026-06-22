"""Adversarial information channels and source credibility updates."""

from __future__ import annotations

from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from enum import Enum
from typing import Any
from uuid import uuid4

import numpy as np

from chiyoda.information.field import ExitBelief, HazardBelief


class AttackerObjective(str, Enum):
    DECOY_EXIT = "decoy-exit"
    PANIC_INDUCE = "panic-induce"
    RESPONDER_SPOOF = "responder-spoof"
    GOSSIP_POISON = "gossip-poison"


@dataclass
class BeliefRevisionConfig:
    prior_alpha: float = 2.0
    prior_beta: float = 2.0
    forgetting_factor: float = 0.95
    evidence_weight: float = 1.0
    min_credibility: float = 0.05
    max_credibility: float = 0.95


@dataclass
class SourceCredibilityState:
    source_id: str
    alpha: float
    beta: float
    last_updated_step: int = 0

    @property
    def credibility(self) -> float:
        denom = self.alpha + self.beta
        if denom <= 0:
            return 0.5
        return self.alpha / denom


@dataclass
class ProvenanceRecord:
    record_id: str
    source_id: str
    timestamp_s: float
    step: int
    channel_type: str
    objective: str
    claimed_exit: tuple | None = None
    claimed_hazard: tuple | None = None
    observed_outcome: bool | None = None
    credibility_after: float | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "record_id": self.record_id,
            "source_id": self.source_id,
            "timestamp_s": self.timestamp_s,
            "step": self.step,
            "channel_type": self.channel_type,
            "objective": self.objective,
            "claimed_exit": (
                list(self.claimed_exit) if self.claimed_exit is not None else None
            ),
            "claimed_hazard": (
                list(self.claimed_hazard) if self.claimed_hazard is not None else None
            ),
            "observed_outcome": self.observed_outcome,
            "credibility_after": self.credibility_after,
        }


@dataclass
class BeliefRevisionModel:
    config: BeliefRevisionConfig = field(default_factory=BeliefRevisionConfig)
    sources: dict[str, SourceCredibilityState] = field(default_factory=dict)
    provenance: list[ProvenanceRecord] = field(default_factory=list)

    def source_credibility(self, source_id: str) -> float:
        state = self._state(source_id)
        return float(
            np.clip(
                state.credibility,
                self.config.min_credibility,
                self.config.max_credibility,
            )
        )

    def record_claim(
        self,
        *,
        source_id: str,
        timestamp_s: float,
        step: int,
        channel_type: str,
        objective: str,
        claimed_exit: Sequence[Any] | None = None,
        claimed_hazard: Sequence[Any] | None = None,
    ) -> ProvenanceRecord:
        record = ProvenanceRecord(
            record_id=str(uuid4()),
            source_id=str(source_id),
            timestamp_s=float(timestamp_s),
            step=int(step),
            channel_type=str(channel_type),
            objective=str(objective),
            claimed_exit=_tuple_or_none(claimed_exit),
            claimed_hazard=_tuple_or_none(claimed_hazard),
        )
        self.provenance.append(record)
        return record

    def update_source(
        self,
        source_id: str,
        supported: bool,
        *,
        step: int = 0,
        weight: float | None = None,
    ) -> float:
        state = self._state(source_id)
        cfg = self.config
        state.alpha = (
            cfg.prior_alpha + (state.alpha - cfg.prior_alpha) * cfg.forgetting_factor
        )
        state.beta = (
            cfg.prior_beta + (state.beta - cfg.prior_beta) * cfg.forgetting_factor
        )
        evidence = cfg.evidence_weight if weight is None else float(weight)
        if supported:
            state.alpha += evidence
        else:
            state.beta += evidence
        state.last_updated_step = int(step)
        return self.source_credibility(source_id)

    def observe_record(
        self, record_id: str, supported: bool, *, step: int = 0
    ) -> float:
        record = next(
            (item for item in self.provenance if item.record_id == record_id), None
        )
        if record is None:
            raise KeyError(record_id)
        credibility = self.update_source(record.source_id, supported, step=step)
        record.observed_outcome = bool(supported)
        record.credibility_after = credibility
        return credibility

    def pending_records(self) -> Iterable[ProvenanceRecord]:
        return (record for record in self.provenance if record.observed_outcome is None)

    def _state(self, source_id: str) -> SourceCredibilityState:
        if source_id not in self.sources:
            self.sources[source_id] = SourceCredibilityState(
                source_id=source_id,
                alpha=float(self.config.prior_alpha),
                beta=float(self.config.prior_beta),
            )
        return self.sources[source_id]


@dataclass
class HostileChannelConfig:
    """Hostile-channel configuration.

    Persona-targeting fields (``target_persona``) follow the
    cohort/mobility/age homophily code path in
    ``chiyoda/information/propagation.py``. See arxiv 2511.04697 for the
    persona-driven misinformation framing this mirrors.
    """

    id: str = "hostile_channel"
    channel_type: str = "gossip"
    objective: AttackerObjective = AttackerObjective.DECOY_EXIT
    budget: int = 1
    start_step: int = 0
    interval_steps: int = 1
    plausibility: float = 0.65
    radius: float = 6.0
    target_cohort: str | None = None
    target_persona: dict[str, Any] | None = None
    source_id: str = "attacker"
    claimed_exit: tuple | None = None
    claimed_hazard: tuple | None = None
    enabled: bool = True

    @classmethod
    def from_mapping(cls, payload: dict[str, Any]) -> HostileChannelConfig:
        objective = AttackerObjective(
            str(payload.get("objective", AttackerObjective.DECOY_EXIT.value))
        )
        return cls(
            id=str(payload.get("id", "hostile_channel")),
            channel_type=str(
                payload.get("channel_type", payload.get("type", "gossip"))
            ),
            objective=objective,
            budget=max(0, int(payload.get("budget", 1))),
            start_step=int(payload.get("start_step", 0)),
            interval_steps=max(1, int(payload.get("interval_steps", 1))),
            plausibility=float(payload.get("plausibility", 0.65)),
            radius=float(payload.get("radius", 6.0)),
            target_cohort=(
                None
                if payload.get("target_cohort") is None
                else str(payload["target_cohort"])
            ),
            target_persona=_normalize_persona(payload.get("target_persona")),
            source_id=str(payload.get("source_id", "attacker")),
            claimed_exit=_tuple_or_none(payload.get("claimed_exit")),
            claimed_hazard=_tuple_or_none(payload.get("claimed_hazard")),
            enabled=bool(payload.get("enabled", True)),
        )


@dataclass
class HostileChannelEvent:
    step: int
    time_s: float
    channel_id: str
    channel_type: str
    objective: str
    source_id: str
    recipients: int
    credibility: float
    claimed_exit: tuple | None = None
    claimed_hazard: tuple | None = None


class HostileChannel:
    def __init__(self, config: HostileChannelConfig) -> None:
        self.config = config
        self.emitted = 0

    def should_fire(self, simulation) -> bool:
        cfg = self.config
        if not cfg.enabled or self.emitted >= cfg.budget:
            return False
        if simulation.current_step < cfg.start_step:
            return False
        return (simulation.current_step - cfg.start_step) % cfg.interval_steps == 0

    def execute(self, simulation) -> HostileChannelEvent | None:
        if not self.should_fire(simulation):
            return None
        recipients = self._select_recipients(simulation)
        if not recipients:
            return None
        remaining = self.config.budget - self.emitted
        recipients = recipients[:remaining]
        claim = self._build_claim(simulation)
        objective = self.config.objective.value
        credibility_values = []
        for agent in recipients:
            source_id = self._source_id(agent)
            credibility = self._effective_credibility(agent, source_id)
            credibility_values.append(credibility)
            if claim.get("exit") is not None:
                agent.beliefs.exit_beliefs[claim["exit"]] = ExitBelief(
                    position=claim["exit"],
                    exists_prob=min(
                        0.99, max(0.01, self.config.plausibility * credibility + 0.2)
                    ),
                    congestion_est=0.0,
                    freshness=0.0,
                    source_credibility=credibility,
                    hop_count=0,
                )
            if claim.get("hazard") is not None:
                agent.beliefs.hazard_beliefs.append(
                    HazardBelief(
                        position=claim["hazard"],
                        severity_est=min(
                            1.0, 0.4 + self.config.plausibility * credibility
                        ),
                        radius_est=max(1.0, self.config.radius),
                        freshness=0.0,
                        source_credibility=credibility,
                        hop_count=0,
                    )
                )
                agent.beliefs.general_danger_level = max(
                    agent.beliefs.general_danger_level, 0.8
                )
            agent.belief_revision.record_claim(
                source_id=source_id,
                timestamp_s=simulation.time_s,
                step=simulation.current_step,
                channel_type=self.config.channel_type,
                objective=objective,
                claimed_exit=claim.get("exit"),
                claimed_hazard=claim.get("hazard"),
            )
            if hasattr(agent, "update_intention"):
                agent.update_intention(simulation)
        self.emitted += len(recipients)
        return HostileChannelEvent(
            step=int(simulation.current_step),
            time_s=float(simulation.time_s),
            channel_id=self.config.id,
            channel_type=self.config.channel_type,
            objective=objective,
            source_id=self.config.source_id,
            recipients=len(recipients),
            credibility=(
                float(np.mean(credibility_values)) if credibility_values else 0.0
            ),
            claimed_exit=claim.get("exit"),
            claimed_hazard=claim.get("hazard"),
        )

    def _select_recipients(self, simulation) -> list[Any]:
        persona = self.config.target_persona or {}
        active = [
            agent
            for agent in simulation._active_agents()
            if hasattr(agent, "beliefs")
            and (
                self.config.target_cohort is None
                or getattr(agent, "cohort_name", None) == self.config.target_cohort
            )
            and _persona_match(agent, persona)
        ]
        if not active:
            return []
        if self.config.objective == AttackerObjective.GOSSIP_POISON:
            active.sort(
                key=lambda item: getattr(item, "credibility", 0.5), reverse=True
            )
            return active
        if (
            self.config.objective == AttackerObjective.PANIC_INDUCE
            and simulation.hazards
        ):
            hazard_pos = np.array(simulation.hazards[0].pos, dtype=float)
            active.sort(key=lambda item: float(np.linalg.norm(item.pos - hazard_pos)))
            return active
        return active

    def _build_claim(self, simulation) -> dict[str, tuple | None]:
        if self.config.objective in {
            AttackerObjective.DECOY_EXIT,
            AttackerObjective.RESPONDER_SPOOF,
            AttackerObjective.GOSSIP_POISON,
        }:
            claimed_exit = self.config.claimed_exit
            if claimed_exit is None:
                claimed_exit = _default_false_exit(simulation)
            return {"exit": tuple(claimed_exit), "hazard": None}
        claimed_hazard = self.config.claimed_hazard
        if claimed_hazard is None:
            claimed_hazard = _default_false_hazard(simulation)
        return {"exit": None, "hazard": tuple(claimed_hazard)}

    def _source_id(self, agent) -> str:
        if self.config.objective == AttackerObjective.RESPONDER_SPOOF:
            return "responder:spoofed"
        if self.config.objective == AttackerObjective.GOSSIP_POISON:
            return f"agent:{agent.id}:poisoned"
        return self.config.source_id

    def _effective_credibility(self, agent, source_id: str) -> float:
        trust = agent.belief_revision.source_credibility(source_id)
        return float(np.clip(self.config.plausibility * (0.5 + trust), 0.01, 0.99))


def create_hostile_channels(
    payload: Sequence[dict[str, Any]] | None,
) -> list[HostileChannel]:
    return [
        HostileChannel(HostileChannelConfig.from_mapping(item))
        for item in (payload or [])
    ]


def evaluate_pending_provenance(agent, simulation, vision_radius: float) -> None:
    if not hasattr(agent, "belief_revision"):
        return
    true_exits = {tuple(exit_.pos) for exit_ in simulation.exits}
    true_hazards = [
        tuple(float(value) for value in hazard.pos) for hazard in simulation.hazards
    ]
    for record in list(agent.belief_revision.pending_records()):
        supported = None
        if record.claimed_exit is not None:
            claim_world = simulation.layout.world_position(record.claimed_exit)
            if _distance(agent.pos, claim_world) <= vision_radius:
                supported = tuple(record.claimed_exit) in true_exits
        if supported is None and record.claimed_hazard is not None:
            if _distance(agent.pos, record.claimed_hazard) <= vision_radius:
                supported = any(
                    _distance(record.claimed_hazard, hazard_pos) < 3.0
                    for hazard_pos in true_hazards
                )
        if supported is not None:
            agent.belief_revision.observe_record(
                record.record_id, supported, step=simulation.current_step
            )


def _default_false_exit(simulation) -> tuple:
    floor_id = simulation.layout.primary_floor_id
    return (
        floor_id,
        max(0, simulation.layout.width - 2),
        max(0, simulation.layout.height - 2),
    )


def _default_false_hazard(simulation) -> tuple:
    floor_id = simulation.layout.primary_floor_id
    z = float(simulation.layout.floor_z(floor_id))
    return (
        max(0.5, simulation.layout.width / 2.0),
        max(0.5, simulation.layout.height / 2.0),
        z,
    )


def _tuple_or_none(value: Sequence[Any] | None) -> tuple | None:
    if value is None:
        return None
    if isinstance(value, dict):
        if {"floor", "x", "y"}.issubset(value):
            return (str(value["floor"]), int(value["x"]), int(value["y"]))
        if {"x", "y", "z"}.issubset(value):
            return (float(value["x"]), float(value["y"]), float(value["z"]))
    return tuple(value)


def _distance(a: Sequence[Any], b: Sequence[Any]) -> float:
    return float(np.linalg.norm(_point3(a) - _point3(b)))


def _normalize_persona(value: Any) -> dict[str, Any] | None:
    if not value:
        return None
    if not isinstance(value, dict):
        return None
    normalized: dict[str, Any] = {}
    if "cohort" in value:
        normalized["cohort"] = str(value["cohort"])
    if "mobility" in value:
        normalized["mobility"] = str(value["mobility"]).lower()
    if "age_band" in value:
        normalized["age_band"] = str(value["age_band"]).lower()
    return normalized or None


def _persona_match(agent: Any, persona: dict[str, Any]) -> bool:
    if not persona:
        return True
    cohort = persona.get("cohort")
    if cohort is not None and getattr(agent, "cohort_name", None) != cohort:
        return False
    mobility = persona.get("mobility")
    if mobility is not None:
        agent_mobility = str(
            getattr(agent, "mobility", getattr(agent, "mobility_class", "")) or ""
        ).lower()
        if agent_mobility != mobility:
            return False
    age_band = persona.get("age_band")
    if age_band is not None:
        agent_age = str(getattr(agent, "age_band", "") or "").lower()
        if agent_age != age_band:
            return False
    return True


def _point3(value: Sequence[Any]) -> np.ndarray:
    if len(value) >= 3 and isinstance(value[0], str):
        return np.array(
            [float(value[1]) + 0.5, float(value[2]) + 0.5, 0.0], dtype=float
        )
    if len(value) >= 3:
        return np.array(
            [float(value[0]), float(value[1]), float(value[2])], dtype=float
        )
    return np.array([float(value[0]), float(value[1]), 0.0], dtype=float)
