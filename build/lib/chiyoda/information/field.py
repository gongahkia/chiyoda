"""
InformationField — 2D belief grid overlaid on the environment layout.

Each agent carries a BeliefVector encoding probabilistic knowledge about:
  - exit locations (known / unknown / rumored)
  - hazard presence and severity (accurate / stale / absent)
  - route congestion (observed / inferred / unknown)

The field supports reading (agents observe at their position) and
writing (agents update beliefs from direct observation or gossip).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import numpy as np

from chiyoda.information.padm import (
    PADM_RECEIVE,
    PADM_UNDERSTAND,
    PADMStageConfig,
    padm_stage_enabled,
    record_padm_stage,
)


@dataclass
class ExitBelief:
    """Belief about a single exit."""

    position: tuple
    exists_prob: float = 0.0  # probability the agent believes this exit exists
    congestion_est: float = 0.0  # estimated congestion [0,1]
    freshness: float = 0.0  # time since last update (0=fresh, decays toward 1)
    source_credibility: float = 0.0  # credibility of the info source [0,1]
    hop_count: int = 0  # number of gossip hops from original source


@dataclass
class HazardBelief:
    """Belief about hazard conditions at a region."""

    position: tuple
    severity_est: float = 0.0  # estimated severity [0,1]
    radius_est: float = 0.0  # estimated radius
    freshness: float = 0.0
    source_credibility: float = 0.0
    hop_count: int = 0


@dataclass
class BeliefVector:
    """Complete belief state for one agent."""

    exit_beliefs: dict[tuple, ExitBelief] = field(default_factory=dict)
    hazard_beliefs: list[HazardBelief] = field(default_factory=list)
    general_danger_level: float = 0.0  # perceived overall danger [0,1]
    information_age_s: float = 0.0  # time since last info update
    last_update_step: int = 0

    def known_exits(self) -> list[tuple]:
        """Return exits the agent believes exist (p > 0.5)."""
        return [
            pos for pos, belief in self.exit_beliefs.items() if belief.exists_prob > 0.5
        ]

    def best_exit(self) -> tuple | None:
        """Return exit with highest probability and lowest congestion."""
        candidates = [
            (pos, b) for pos, b in self.exit_beliefs.items() if b.exists_prob > 0.5
        ]
        if not candidates:
            return None
        # score = exists_prob - congestion_est, prefer fresh info
        candidates.sort(
            key=lambda x: (x[1].exists_prob - x[1].congestion_est)
            * (1.0 - x[1].freshness * 0.3),
            reverse=True,
        )
        return candidates[0][0]

    def perceived_hazard_at(self, pos: tuple[float, ...]) -> float:
        """Estimated hazard intensity at a position based on beliefs."""
        intensity = 0.0
        for hb in self.hazard_beliefs:
            dist = float(np.linalg.norm(_point3(pos) - _point3(hb.position)))
            if hb.radius_est > 0 and dist <= hb.radius_est:
                intensity += hb.severity_est * max(0.0, 1.0 - dist / hb.radius_est)
            elif hb.radius_est <= 0 and dist < 1.0:
                intensity += hb.severity_est
        return intensity


class InformationField:
    """
    Global information field managing belief propagation and decay.

    The field tracks ground truth and each agent's deviation from it,
    enabling entropy and accuracy calculations.
    """

    def __init__(
        self,
        width: int,
        height: int,
        *,
        decay_rate: float = 0.01,  # per-step freshness decay
        observation_radius: float = 3.0,  # how far agents can directly observe
        beacon_radius: float = 8.0,  # PA/signage broadcast radius
        gossip_radius: float = 2.0,  # agent-to-agent info transfer radius
    ) -> None:
        self.width = width
        self.height = height
        self.decay_rate = decay_rate
        self.observation_radius = observation_radius
        self.beacon_radius = beacon_radius
        self.gossip_radius = gossip_radius

        self.ground_truth_exits: list[tuple] = []  # actual exit positions
        self.beacons: list[tuple] = []  # signage/PA positions
        self.beacon_exit_info: dict[tuple, list[tuple]] = {}  # what each beacon knows
        self.exit_world_positions: dict[tuple, tuple] = {}

    def set_ground_truth(
        self,
        exits: list[tuple],
        beacons: list[tuple] | None = None,
    ) -> None:
        """Initialize field with actual environment state."""
        self.ground_truth_exits = list(exits)
        if beacons:
            self.beacons = list(beacons)
            for b in self.beacons:  # beacons know all exits by default
                self.beacon_exit_info[b] = list(exits)

    def create_agent_beliefs(
        self,
        agent_pos: tuple[float, ...],
        familiarity: float = 0.0,
        *,
        known_exits: list[tuple] | None = None,
    ) -> BeliefVector:
        """
        Create initial belief vector for an agent.

        familiarity in [0,1]: 0=tourist (knows nothing), 1=regular commuter (knows all exits).
        """
        beliefs = BeliefVector()

        if known_exits:
            for exit_pos in known_exits:
                beliefs.exit_beliefs[exit_pos] = ExitBelief(
                    position=exit_pos,
                    exists_prob=1.0,
                    source_credibility=1.0,
                )

        for exit_pos in self.ground_truth_exits:
            if exit_pos in beliefs.exit_beliefs:
                continue
            # familiar agents have higher prob of knowing exits
            if np.random.random() < familiarity:
                beliefs.exit_beliefs[exit_pos] = ExitBelief(
                    position=exit_pos,
                    exists_prob=0.7 + 0.3 * familiarity,
                    source_credibility=familiarity,
                )
            # else: agent doesn't know about this exit

        return beliefs

    def observe(
        self,
        agent_beliefs: BeliefVector,
        agent_pos: tuple[float, ...],
        vision_radius: float,
        exits: list[tuple],
        hazards: list,
        current_step: int,
    ) -> None:
        """Agent directly observes environment within vision cone."""
        self.observe_many(
            [(agent_beliefs, agent_pos, vision_radius)],
            exits,
            hazards,
            current_step,
        )

    def observe_many(
        self,
        observations: list[tuple[BeliefVector, tuple[float, ...], float]],
        exits: list[tuple],
        hazards: list,
        current_step: int,
    ) -> None:
        """Batch direct observations with broadcasted distance checks."""
        if not observations:
            return

        beliefs = [row[0] for row in observations]
        agent_points = _points3_array([row[1] for row in observations])
        vision_sq = np.square(np.array([row[2] for row in observations], dtype=float))

        exit_keys = [tuple(exit_pos) for exit_pos in exits]
        exit_points = _points3_array(
            [
                self.exit_world_positions.get(exit_pos, tuple(_cell_center3(exit_pos)))
                for exit_pos in exit_keys
            ]
        )
        if exit_keys:
            visible_agents, visible_exits = np.nonzero(
                _squared_distance_matrix(agent_points, exit_points)
                <= vision_sq[:, None]
            )
            for agent_idx, exit_idx in zip(visible_agents, visible_exits, strict=False):
                exit_pos = exit_keys[int(exit_idx)]
                beliefs[int(agent_idx)].exit_beliefs[exit_pos] = ExitBelief(
                    position=exit_pos,
                    exists_prob=1.0,
                    congestion_est=0.0,  # updated by density observation
                    freshness=0.0,
                    source_credibility=1.0,
                    hop_count=0,
                )

        hazard_positions = [
            tuple(float(value) for value in hazard.pos) for hazard in hazards
        ]
        hazard_points = _points3_array(hazard_positions)
        if hazard_positions:
            visible_agents, visible_hazards = np.nonzero(
                _squared_distance_matrix(agent_points, hazard_points)
                <= vision_sq[:, None]
            )
            for agent_idx, hazard_idx in zip(
                visible_agents, visible_hazards, strict=False
            ):
                hazard = hazards[int(hazard_idx)]
                self._apply_hazard_observation(
                    beliefs[int(agent_idx)],
                    hazard_positions[int(hazard_idx)],
                    hazard_points[int(hazard_idx)],
                    float(hazard.severity),
                    float(hazard.radius),
                )

        for belief in beliefs:
            belief.last_update_step = current_step

    def padm_receive(
        self,
        observations: list[tuple[Any, tuple[float, ...], float]],
        exits: list[tuple],
        hazards: list,
        current_step: int,
        *,
        stage_config: PADMStageConfig | None = None,
    ) -> None:
        """PADM receive hook: environmental cues, signage, and PA inputs."""
        if not observations or not padm_stage_enabled(stage_config, PADM_RECEIVE):
            return
        for agent, _agent_pos, _vision_radius in observations:
            record_padm_stage(agent, PADM_RECEIVE)
        self.observe_many(
            [
                (agent.beliefs, agent_pos, vision_radius)
                for agent, agent_pos, vision_radius in observations
            ],
            exits,
            hazards,
            current_step,
        )
        for agent, agent_pos, _vision_radius in observations:
            self.beacon_broadcast(agent.beliefs, agent_pos)

    def padm_understand(
        self,
        observations: list[tuple[Any, tuple[float, ...], float]],
        dt: float,
        *,
        stage_config: PADMStageConfig | None = None,
    ) -> None:
        """PADM understand hook: keep interpreted beliefs time-aware."""
        if not observations or not padm_stage_enabled(stage_config, PADM_UNDERSTAND):
            return
        for agent, _agent_pos, _vision_radius in observations:
            record_padm_stage(agent, PADM_UNDERSTAND)
        self.decay_beliefs_batch([agent.beliefs for agent, _, _ in observations], dt)

    def _apply_hazard_observation(
        self,
        agent_beliefs: BeliefVector,
        hazard_pos: tuple[float, ...],
        hazard_point: np.ndarray,
        severity: float,
        radius: float,
    ) -> None:
        existing = agent_beliefs.hazard_beliefs
        if existing:
            existing_points = _points3_array([hb.position for hb in existing])
            matches = np.flatnonzero(
                _squared_distances(hazard_point, existing_points) < 4.0
            )
            if matches.size > 0:
                hb = existing[int(matches[0])]
                hb.severity_est = severity
                hb.radius_est = radius
                hb.freshness = 0.0
                hb.source_credibility = 1.0
                hb.hop_count = 0
                return
        agent_beliefs.hazard_beliefs.append(
            HazardBelief(
                position=hazard_pos,
                severity_est=severity,
                radius_est=radius,
                freshness=0.0,
                source_credibility=1.0,
                hop_count=0,
            )
        )

    def beacon_broadcast(
        self,
        agent_beliefs: BeliefVector,
        agent_pos: tuple[float, ...],
    ) -> None:
        """Agent receives info from nearby beacons (signage/PA)."""
        beacon_points = _points3_array(self.beacons)
        if beacon_points.size == 0:
            return
        visible = np.flatnonzero(
            _squared_distances(_point3(agent_pos), beacon_points)
            <= self.beacon_radius * self.beacon_radius
        )
        for beacon_idx in visible:
            beacon_pos = self.beacons[int(beacon_idx)]
            known = self.beacon_exit_info.get(beacon_pos, [])
            for exit_pos in known:
                existing = agent_beliefs.exit_beliefs.get(exit_pos)
                if existing is None or existing.source_credibility < 0.9:
                    agent_beliefs.exit_beliefs[exit_pos] = ExitBelief(
                        position=exit_pos,
                        exists_prob=0.95,
                        freshness=0.0,
                        source_credibility=0.9,
                        hop_count=0,
                    )

    def decay_beliefs(self, agent_beliefs: BeliefVector, dt: float) -> None:
        """Age all beliefs — information becomes stale over time."""
        self.decay_beliefs_batch([agent_beliefs], dt)

    def decay_beliefs_batch(self, all_beliefs: list[BeliefVector], dt: float) -> None:
        """Batch age beliefs with vectorized numeric updates."""
        if not all_beliefs:
            return

        decay = self.decay_rate * dt
        exit_beliefs = [
            belief
            for agent_beliefs in all_beliefs
            for belief in agent_beliefs.exit_beliefs.values()
        ]
        if exit_beliefs:
            freshness = np.array([belief.freshness for belief in exit_beliefs])
            exists = np.array([belief.exists_prob for belief in exit_beliefs])
            next_freshness = np.minimum(1.0, freshness + decay)
            next_exists = np.maximum(0.0, exists * (1.0 - decay * 0.1))
            for belief, fresh, exists_prob in zip(
                exit_beliefs, next_freshness, next_exists, strict=False
            ):
                belief.freshness = float(fresh)
                belief.exists_prob = float(exists_prob)

        hazard_beliefs = [
            belief
            for agent_beliefs in all_beliefs
            for belief in agent_beliefs.hazard_beliefs
        ]
        if hazard_beliefs:
            freshness = np.array([belief.freshness for belief in hazard_beliefs])
            next_freshness = np.minimum(1.0, freshness + decay)
            for belief, fresh in zip(hazard_beliefs, next_freshness, strict=False):
                belief.freshness = float(fresh)

        for agent_beliefs in all_beliefs:
            agent_beliefs.information_age_s += dt


def _point3(value: Any) -> np.ndarray:
    if len(value) >= 3 and not isinstance(value[0], str):
        return np.array(
            [float(value[0]), float(value[1]), float(value[2])], dtype=float
        )
    return np.array([float(value[0]), float(value[1]), 0.0], dtype=float)


def _cell_center3(value: Any) -> np.ndarray:
    if len(value) >= 3 and isinstance(value[0], str):
        return np.array(
            [float(value[1]) + 0.5, float(value[2]) + 0.5, 0.0], dtype=float
        )
    if len(value) >= 3:
        return np.array(
            [float(value[0]) + 0.5, float(value[1]) + 0.5, float(value[2])], dtype=float
        )
    return np.array([float(value[0]) + 0.5, float(value[1]) + 0.5, 0.0], dtype=float)


def _points3_array(values: list[Any]) -> np.ndarray:
    if not values:
        return np.empty((0, 3), dtype=float)
    return np.vstack([_point3(value) for value in values])


def _squared_distances(origin: np.ndarray, points: np.ndarray) -> np.ndarray:
    if points.size == 0:
        return np.empty((0,), dtype=float)
    delta = points - origin.reshape(1, 3)
    return np.einsum("ij,ij->i", delta, delta)


def _squared_distance_matrix(left: np.ndarray, right: np.ndarray) -> np.ndarray:
    if left.size == 0 or right.size == 0:
        return np.empty((left.shape[0], right.shape[0]), dtype=float)
    delta = left[:, None, :] - right[None, :, :]
    return np.einsum("ijk,ijk->ij", delta, delta)
