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
from typing import Dict, List, Optional, Tuple

import numpy as np


@dataclass
class ExitBelief:
    """Belief about a single exit."""
    position: Tuple[int, int]
    exists_prob: float = 0.0  # probability the agent believes this exit exists
    congestion_est: float = 0.0  # estimated congestion [0,1]
    freshness: float = 0.0  # time since last update (0=fresh, decays toward 1)
    source_credibility: float = 0.0  # credibility of the info source [0,1]
    hop_count: int = 0  # number of gossip hops from original source


@dataclass
class HazardBelief:
    """Belief about hazard conditions at a region."""
    position: Tuple[float, float]
    severity_est: float = 0.0  # estimated severity [0,1]
    radius_est: float = 0.0  # estimated radius
    freshness: float = 0.0
    source_credibility: float = 0.0
    hop_count: int = 0


@dataclass
class BeliefVector:
    """Complete belief state for one agent."""
    exit_beliefs: Dict[Tuple[int, int], ExitBelief] = field(default_factory=dict)
    hazard_beliefs: List[HazardBelief] = field(default_factory=list)
    general_danger_level: float = 0.0  # perceived overall danger [0,1]
    information_age_s: float = 0.0  # time since last info update
    last_update_step: int = 0

    def known_exits(self) -> List[Tuple[int, int]]:
        """Return exits the agent believes exist (p > 0.5)."""
        return [
            pos for pos, belief in self.exit_beliefs.items()
            if belief.exists_prob > 0.5
        ]

    def best_exit(self) -> Optional[Tuple[int, int]]:
        """Return exit with highest probability and lowest congestion."""
        candidates = [
            (pos, b) for pos, b in self.exit_beliefs.items()
            if b.exists_prob > 0.5
        ]
        if not candidates:
            return None
        # score = exists_prob - congestion_est, prefer fresh info
        candidates.sort(
            key=lambda x: (x[1].exists_prob - x[1].congestion_est) * (1.0 - x[1].freshness * 0.3),
            reverse=True,
        )
        return candidates[0][0]

    def perceived_hazard_at(self, pos: Tuple[float, float]) -> float:
        """Estimated hazard intensity at a position based on beliefs."""
        intensity = 0.0
        for hb in self.hazard_beliefs:
            dist = np.sqrt((pos[0] - hb.position[0]) ** 2 + (pos[1] - hb.position[1]) ** 2)
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

        self.ground_truth_exits: List[Tuple[int, int]] = [] # actual exit positions
        self.beacons: List[Tuple[float, float]] = [] # signage/PA positions
        self.beacon_exit_info: Dict[Tuple[float, float], List[Tuple[int, int]]] = {} # what each beacon knows

    def set_ground_truth(
        self,
        exits: List[Tuple[int, int]],
        beacons: Optional[List[Tuple[float, float]]] = None,
    ) -> None:
        """Initialize field with actual environment state."""
        self.ground_truth_exits = list(exits)
        if beacons:
            self.beacons = list(beacons)
            for b in self.beacons: # beacons know all exits by default
                self.beacon_exit_info[b] = list(exits)

    def create_agent_beliefs(
        self,
        agent_pos: Tuple[float, float],
        familiarity: float = 0.0,
        *,
        known_exits: Optional[List[Tuple[int, int]]] = None,
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
        agent_pos: Tuple[float, float],
        vision_radius: float,
        exits: List[Tuple[int, int]],
        hazards: list,
        current_step: int,
    ) -> None:
        """Agent directly observes environment within vision cone."""
        for exit_pos in exits:
            dist = np.sqrt(
                (agent_pos[0] - exit_pos[0]) ** 2 + (agent_pos[1] - exit_pos[1]) ** 2
            )
            if dist <= vision_radius:
                agent_beliefs.exit_beliefs[exit_pos] = ExitBelief(
                    position=exit_pos,
                    exists_prob=1.0,
                    congestion_est=0.0, # will be updated by density observation
                    freshness=0.0,
                    source_credibility=1.0,
                    hop_count=0,
                )

        for hazard in hazards:
            h_pos = (float(hazard.pos[0]), float(hazard.pos[1]))
            dist = np.sqrt(
                (agent_pos[0] - h_pos[0]) ** 2 + (agent_pos[1] - h_pos[1]) ** 2
            )
            if dist <= vision_radius:
                # update or add hazard belief with direct observation
                updated = False
                for hb in agent_beliefs.hazard_beliefs:
                    hb_dist = np.sqrt(
                        (hb.position[0] - h_pos[0]) ** 2 + (hb.position[1] - h_pos[1]) ** 2
                    )
                    if hb_dist < 2.0: # same hazard
                        hb.severity_est = float(hazard.severity)
                        hb.radius_est = float(hazard.radius)
                        hb.freshness = 0.0
                        hb.source_credibility = 1.0
                        hb.hop_count = 0
                        updated = True
                        break
                if not updated:
                    agent_beliefs.hazard_beliefs.append(
                        HazardBelief(
                            position=h_pos,
                            severity_est=float(hazard.severity),
                            radius_est=float(hazard.radius),
                            freshness=0.0,
                            source_credibility=1.0,
                            hop_count=0,
                        )
                    )

        agent_beliefs.last_update_step = current_step

    def beacon_broadcast(
        self,
        agent_beliefs: BeliefVector,
        agent_pos: Tuple[float, float],
    ) -> None:
        """Agent receives info from nearby beacons (signage/PA)."""
        for beacon_pos in self.beacons:
            dist = np.sqrt(
                (agent_pos[0] - beacon_pos[0]) ** 2 + (agent_pos[1] - beacon_pos[1]) ** 2
            )
            if dist <= self.beacon_radius:
                known = self.beacon_exit_info.get(beacon_pos, [])
                for exit_pos in known:
                    existing = agent_beliefs.exit_beliefs.get(exit_pos)
                    if existing is None or existing.source_credibility < 0.9:
                        agent_beliefs.exit_beliefs[exit_pos] = ExitBelief(
                            position=exit_pos,
                            exists_prob=0.95, # high but not perfect — agent may not fully trust PA
                            freshness=0.0,
                            source_credibility=0.9,
                            hop_count=0,
                        )

    def decay_beliefs(self, agent_beliefs: BeliefVector, dt: float) -> None:
        """Age all beliefs — information becomes stale over time."""
        for eb in agent_beliefs.exit_beliefs.values():
            eb.freshness = min(1.0, eb.freshness + self.decay_rate * dt)
            eb.exists_prob *= (1.0 - self.decay_rate * dt * 0.1) # very slow confidence decay
            eb.exists_prob = max(0.0, eb.exists_prob)
        for hb in agent_beliefs.hazard_beliefs:
            hb.freshness = min(1.0, hb.freshness + self.decay_rate * dt)
        agent_beliefs.information_age_s += dt
