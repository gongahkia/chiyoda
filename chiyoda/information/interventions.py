"""
Information intervention policies for ITED studies.

Policies model controllable emergency communication: signage, PA broadcasts,
responder relay, and adaptive targeting based on entropy, density, exposure,
or bottleneck pressure. They operate on existing agent belief vectors and emit
study-grade telemetry for information-safety analysis.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Sequence, Tuple

import numpy as np

from chiyoda.information.entropy import agent_entropy, belief_accuracy
from chiyoda.information.field import ExitBelief, HazardBelief


Cell = Tuple[int, int]
Point = Tuple[float, float]


@dataclass
class InformationInterventionConfig:
    policy: str = "none"
    start_step: int = 0
    end_step: Optional[int] = None
    interval_steps: int = 20
    budget_per_interval: int = 1
    message_radius: float = 8.0
    credibility: float = 0.9
    message_type: str = "route_guidance"
    objective: str = "reduce_entropy_without_bottlenecking"
    enabled: bool = True

    @classmethod
    def from_mapping(cls, payload: Optional[Dict[str, Any]]) -> "InformationInterventionConfig":
        data = dict(payload or {})
        return cls(
            policy=str(data.get("policy", "none")),
            start_step=int(data.get("start_step", 0)),
            end_step=None if data.get("end_step") is None else int(data["end_step"]),
            interval_steps=max(1, int(data.get("interval_steps", 20))),
            budget_per_interval=max(1, int(data.get("budget_per_interval", 1))),
            message_radius=float(data.get("message_radius", 8.0)),
            credibility=float(data.get("credibility", 0.9)),
            message_type=str(data.get("message_type", "route_guidance")),
            objective=str(data.get("objective", "reduce_entropy_without_bottlenecking")),
            enabled=bool(data.get("enabled", True)),
        )


@dataclass
class InterventionTarget:
    point: Point
    reason: str
    score: float = 0.0
    source_agent_id: Optional[int] = None
    zone_id: Optional[str] = None


@dataclass
class InterventionMessage:
    message_type: str
    credibility: float
    radius: float
    exits: Sequence[Cell] = field(default_factory=list)
    hazards: Sequence[Any] = field(default_factory=list)
    congested_exits: Sequence[Cell] = field(default_factory=list)


@dataclass
class InterventionEvent:
    step: int
    time_s: float
    policy: str
    message_type: str
    target_x: float
    target_y: float
    radius: float
    recipients: int
    entropy_before: float
    entropy_after: float
    accuracy_before: float
    accuracy_after: float
    mean_local_density: float
    mean_hazard_load: float
    peak_queue_length: int
    selected_reason: str
    target_score: float
    objective: str


class InterventionPolicy:
    """Base class for emergency information policies."""

    name = "base"

    def __init__(self, config: InformationInterventionConfig) -> None:
        self.config = config

    def should_fire(self, simulation) -> bool:
        cfg = self.config
        if not cfg.enabled or cfg.policy == "none":
            return False
        if simulation.current_step < cfg.start_step:
            return False
        if cfg.end_step is not None and simulation.current_step > cfg.end_step:
            return False
        return (simulation.current_step - cfg.start_step) % cfg.interval_steps == 0

    def select_targets(self, simulation) -> List[InterventionTarget]:
        return []

    def build_message(self, simulation, target: InterventionTarget) -> InterventionMessage:
        congested = self._congested_exits(simulation)
        return InterventionMessage(
            message_type=self.config.message_type,
            credibility=self.config.credibility,
            radius=self.config.message_radius,
            exits=[tuple(e.pos) for e in simulation.exits],
            hazards=list(simulation.hazards),
            congested_exits=congested,
        )

    def execute(self, simulation) -> List[InterventionEvent]:
        if not self.should_fire(simulation):
            return []
        targets = self.select_targets(simulation)[: self.config.budget_per_interval]
        events: List[InterventionEvent] = []
        for target in targets:
            message = self.build_message(simulation, target)
            events.append(_apply_message(simulation, self, target, message))
        return events

    def _congested_exits(self, simulation) -> List[Cell]:
        totals = getattr(simulation, "exit_flow_cumulative", {})
        if not totals:
            return []
        labels_by_cell = {label: cell for cell, label in simulation.exit_labels.items()}
        total_flow = sum(totals.values())
        if total_flow <= 0:
            return []
        return [
            labels_by_cell[label]
            for label, flow in totals.items()
            if flow / total_flow >= 0.5 and label in labels_by_cell
        ]


class StaticBroadcastPolicy(InterventionPolicy):
    name = "static_broadcast"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        points: List[Point] = []
        if getattr(simulation.info_field, "beacons", None):
            points = list(simulation.info_field.beacons)
        if not points:
            points = [(simulation.layout.width / 2.0, simulation.layout.height / 2.0)]
        return [
            InterventionTarget(point=point, reason="static_broadcast_source", score=1.0)
            for point in points
        ]


class GlobalBroadcastPolicy(InterventionPolicy):
    name = "global_broadcast"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        return [
            InterventionTarget(
                point=(simulation.layout.width / 2.0, simulation.layout.height / 2.0),
                reason="global_broadcast",
                score=float(len(simulation._active_agents())),
            )
        ]

    def build_message(self, simulation, target: InterventionTarget) -> InterventionMessage:
        message = super().build_message(simulation, target)
        message.radius = max(simulation.layout.width, simulation.layout.height) * 2.0
        return message


class ResponderRelayPolicy(InterventionPolicy):
    name = "responder_relay"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        responders = [
            agent for agent in simulation._active_agents()
            if getattr(agent, "is_responder", False)
        ]
        return [
            InterventionTarget(
                point=(float(agent.pos[0]), float(agent.pos[1])),
                reason="responder_relay",
                score=float(getattr(agent, "credibility", 1.0)),
                source_agent_id=agent.id,
            )
            for agent in responders
        ]


class EntropyTargetedPolicy(InterventionPolicy):
    name = "entropy_targeted"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        rows = []
        total_exits = len(simulation.exits)
        total_hazards = len(simulation.hazards)
        for agent in simulation._active_agents():
            if not hasattr(agent, "beliefs"):
                continue
            score = agent_entropy(agent.beliefs, total_exits, total_hazards)
            rows.append((score, agent))
        rows.sort(key=lambda item: item[0], reverse=True)
        return [
            InterventionTarget(
                point=(float(agent.pos[0]), float(agent.pos[1])),
                reason="highest_agent_entropy",
                score=float(score),
                source_agent_id=agent.id,
            )
            for score, agent in rows
        ]


class DensityAwarePolicy(InterventionPolicy):
    name = "density_aware"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        rows = [
            (float(getattr(agent, "local_density", 0.0)), agent)
            for agent in simulation._active_agents()
        ]
        rows.sort(key=lambda item: item[0], reverse=True)
        return [
            InterventionTarget(
                point=(float(agent.pos[0]), float(agent.pos[1])),
                reason="highest_local_density",
                score=float(score),
                source_agent_id=agent.id,
            )
            for score, agent in rows
            if score > 0.0
        ]


class ExposureAwarePolicy(InterventionPolicy):
    name = "exposure_aware"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        rows = [
            (
                float(getattr(agent, "current_hazard_load", 0.0))
                + 0.25 * float(getattr(agent, "hazard_exposure", 0.0)),
                agent,
            )
            for agent in simulation._active_agents()
        ]
        rows.sort(key=lambda item: item[0], reverse=True)
        return [
            InterventionTarget(
                point=(float(agent.pos[0]), float(agent.pos[1])),
                reason="highest_exposure_pressure",
                score=float(score),
                source_agent_id=agent.id,
            )
            for score, agent in rows
            if score > 0.0
        ]


class BottleneckAvoidancePolicy(InterventionPolicy):
    name = "bottleneck_avoidance"

    def select_targets(self, simulation) -> List[InterventionTarget]:
        if not simulation.step_history:
            return []
        latest = simulation.step_history[-1]
        rows = []
        for zone_id, metrics in latest.bottlenecks.items():
            zone = simulation.bottleneck_zone_map.get(zone_id)
            if zone is None:
                continue
            score = float(metrics.queue_length + metrics.occupancy)
            rows.append((score, zone))
        rows.sort(key=lambda item: item[0], reverse=True)
        return [
            InterventionTarget(
                point=(float(zone.centroid[0]), float(zone.centroid[1])),
                reason="bottleneck_pressure",
                score=float(score),
                zone_id=zone.zone_id,
            )
            for score, zone in rows
            if score > 0.0
        ]


POLICIES = {
    "static_broadcast": StaticBroadcastPolicy,
    "static_beacon": StaticBroadcastPolicy,
    "global_broadcast": GlobalBroadcastPolicy,
    "responder_relay": ResponderRelayPolicy,
    "entropy_targeted": EntropyTargetedPolicy,
    "density_aware": DensityAwarePolicy,
    "exposure_aware": ExposureAwarePolicy,
    "bottleneck_avoidance": BottleneckAvoidancePolicy,
}


def create_intervention_policy(payload: Optional[Dict[str, Any]]) -> Optional[InterventionPolicy]:
    config = InformationInterventionConfig.from_mapping(payload)
    if not config.enabled or config.policy == "none":
        return None
    cls = POLICIES.get(config.policy)
    if cls is None:
        raise ValueError(f"Unsupported information intervention policy: {config.policy}")
    return cls(config)


def _apply_message(
    simulation,
    policy: InterventionPolicy,
    target: InterventionTarget,
    message: InterventionMessage,
) -> InterventionEvent:
    active = [
        agent for agent in simulation._active_agents()
        if hasattr(agent, "beliefs")
        and _distance((float(agent.pos[0]), float(agent.pos[1])), target.point) <= message.radius
    ]
    total_exits = len(simulation.exits)
    total_hazards = len(simulation.hazards)
    true_exits = [tuple(e.pos) for e in simulation.exits]
    before_entropy = _mean([
        agent_entropy(agent.beliefs, total_exits, total_hazards)
        for agent in active
    ])
    before_accuracy = _mean([
        belief_accuracy(agent.beliefs, true_exits, simulation.hazards)
        for agent in active
    ], default=1.0)

    for agent in active:
        _update_agent_beliefs(agent, message, simulation.current_step)
        if hasattr(agent, "update_intention"):
            agent.update_intention(simulation)

    after_entropy = _mean([
        agent_entropy(agent.beliefs, total_exits, total_hazards)
        for agent in active
    ])
    after_accuracy = _mean([
        belief_accuracy(agent.beliefs, true_exits, simulation.hazards)
        for agent in active
    ], default=1.0)
    latest = simulation.step_history[-1] if simulation.step_history else None
    peak_queue = 0
    if latest is not None:
        peak_queue = max((m.queue_length for m in latest.bottlenecks.values()), default=0)

    return InterventionEvent(
        step=simulation.current_step,
        time_s=simulation.time_s,
        policy=policy.config.policy,
        message_type=message.message_type,
        target_x=float(target.point[0]),
        target_y=float(target.point[1]),
        radius=float(message.radius),
        recipients=len(active),
        entropy_before=float(before_entropy),
        entropy_after=float(after_entropy),
        accuracy_before=float(before_accuracy),
        accuracy_after=float(after_accuracy),
        mean_local_density=_mean([float(getattr(agent, "local_density", 0.0)) for agent in active]),
        mean_hazard_load=_mean([float(getattr(agent, "current_hazard_load", 0.0)) for agent in active]),
        peak_queue_length=int(peak_queue),
        selected_reason=target.reason,
        target_score=float(target.score),
        objective=policy.config.objective,
    )


def _update_agent_beliefs(agent, message: InterventionMessage, current_step: int) -> None:
    for exit_pos in message.exits:
        existing = agent.beliefs.exit_beliefs.get(tuple(exit_pos))
        congestion = 0.0
        if tuple(exit_pos) in message.congested_exits:
            congestion = 0.8
        if existing is None or existing.source_credibility <= message.credibility:
            agent.beliefs.exit_beliefs[tuple(exit_pos)] = ExitBelief(
                position=tuple(exit_pos),
                exists_prob=min(1.0, 0.55 + 0.45 * message.credibility),
                congestion_est=congestion,
                freshness=0.0,
                source_credibility=message.credibility,
                hop_count=0,
            )
        else:
            existing.freshness = min(existing.freshness, 0.05)
            existing.source_credibility = max(existing.source_credibility, message.credibility)
            existing.congestion_est = max(existing.congestion_est, congestion)

    for hazard in message.hazards:
        h_pos = (float(hazard.pos[0]), float(hazard.pos[1]))
        matched = False
        for hb in agent.beliefs.hazard_beliefs:
            if _distance(hb.position, h_pos) < 3.0:
                hb.severity_est = max(float(hb.severity_est), float(hazard.severity) * message.credibility)
                hb.radius_est = max(float(hb.radius_est), float(hazard.radius))
                hb.freshness = 0.0
                hb.source_credibility = max(float(hb.source_credibility), message.credibility)
                matched = True
                break
        if not matched:
            agent.beliefs.hazard_beliefs.append(
                HazardBelief(
                    position=h_pos,
                    severity_est=float(hazard.severity) * message.credibility,
                    radius_est=float(hazard.radius),
                    freshness=0.0,
                    source_credibility=message.credibility,
                    hop_count=0,
                )
            )

    agent.beliefs.general_danger_level = max(
        float(agent.beliefs.general_danger_level),
        min(1.0, 0.4 + 0.4 * len(message.hazards)),
    )
    agent.beliefs.information_age_s = 0.0
    agent.beliefs.last_update_step = current_step


def _distance(a: Point, b: Point) -> float:
    return float(np.linalg.norm(np.array(a, dtype=float) - np.array(b, dtype=float)))


def _mean(values: Sequence[float], default: float = 0.0) -> float:
    if not values:
        return float(default)
    return float(np.mean(values))
