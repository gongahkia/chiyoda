"""
SIR-inspired gossip model for information propagation between agents.

When two agents are within gossip radius, beliefs transfer with probability
proportional to source credibility, receiver rationality, and freshness.
Rumors are modeled as distorted information — distortion increases with hop count.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, Tuple

import numpy as np

from chiyoda.information.field import BeliefVector, ExitBelief, HazardBelief


@dataclass
class GossipConfig:
    gossip_radius: float = 2.0  # max distance for info transfer
    base_transfer_prob: float = 0.3  # base probability per step per neighbor
    distortion_per_hop: float = 0.05  # information degradation per gossip hop
    credibility_threshold: float = 0.2  # minimum credibility to accept info
    max_hops: int = 10  # info dies after this many hops
    panic_distortion_mult: float = 2.0  # panicked agents distort info more


class GossipModel:
    """Manages information exchange between agents during simulation steps."""

    def __init__(self, config: Optional[GossipConfig] = None) -> None:
        self.config = config or GossipConfig()

    def exchange(
        self,
        sender_beliefs: BeliefVector,
        receiver_beliefs: BeliefVector,
        sender_credibility: float,
        receiver_rationality: float,
        sender_state: str,
        distance: float,
    ) -> bool:
        """
        Attempt gossip exchange from sender to receiver.

        Returns True if any information was transferred.
        """
        if distance > self.config.gossip_radius:
            return False

        # transfer probability: closer = higher, more credible = higher
        proximity_factor = max(0.0, 1.0 - distance / self.config.gossip_radius)
        transfer_prob = (
            self.config.base_transfer_prob
            * proximity_factor
            * sender_credibility
            * receiver_rationality
        )

        if np.random.random() > transfer_prob:
            return False

        # distortion factor: panicked senders distort more
        distortion = self.config.distortion_per_hop
        if sender_state == "PANICKED":
            distortion *= self.config.panic_distortion_mult

        transferred = False

        # transfer exit beliefs
        for exit_pos, sender_eb in sender_beliefs.exit_beliefs.items():
            if sender_eb.hop_count >= self.config.max_hops:
                continue
            if sender_eb.exists_prob < 0.3: # don't gossip about uncertain exits
                continue

            receiver_eb = receiver_beliefs.exit_beliefs.get(exit_pos)
            # transfer if receiver doesn't know, or sender has fresher/more credible info
            should_transfer = (
                receiver_eb is None
                or (sender_eb.freshness < receiver_eb.freshness and sender_eb.source_credibility > receiver_eb.source_credibility * 0.8)
                or sender_eb.source_credibility > receiver_eb.source_credibility + 0.2
            )

            if should_transfer:
                # apply distortion
                distorted_prob = max(
                    0.0,
                    min(1.0, sender_eb.exists_prob + np.random.normal(0, distortion)),
                )
                distorted_congestion = max(
                    0.0,
                    min(1.0, sender_eb.congestion_est + np.random.normal(0, distortion * 2)),
                )

                new_credibility = sender_credibility * (1.0 - distortion * sender_eb.hop_count)
                if new_credibility < self.config.credibility_threshold:
                    continue

                receiver_beliefs.exit_beliefs[exit_pos] = ExitBelief(
                    position=exit_pos,
                    exists_prob=distorted_prob,
                    congestion_est=distorted_congestion,
                    freshness=sender_eb.freshness + 0.05, # slightly staler
                    source_credibility=max(0.0, new_credibility),
                    hop_count=sender_eb.hop_count + 1,
                )
                transferred = True

        # transfer hazard beliefs
        for sender_hb in sender_beliefs.hazard_beliefs:
            if sender_hb.hop_count >= self.config.max_hops:
                continue

            # check if receiver already knows about this hazard
            matching_belief = None
            for receiver_hb in receiver_beliefs.hazard_beliefs:
                dist = np.sqrt(
                    (sender_hb.position[0] - receiver_hb.position[0]) ** 2
                    + (sender_hb.position[1] - receiver_hb.position[1]) ** 2
                )
                if dist < 3.0:
                    matching_belief = receiver_hb
                    break

            distorted_severity = max(
                0.0,
                min(
                    1.0,
                    sender_hb.severity_est + np.random.normal(0, distortion),
                ),
            )
            distorted_radius = max(
                0.0,
                sender_hb.radius_est * (1.0 + np.random.normal(0, distortion * 3)),
            )
            transferred_credibility = max(0.0, sender_credibility * (1.0 - distortion))

            if matching_belief is not None:
                if transferred_credibility > matching_belief.source_credibility:
                    matching_belief.severity_est = distorted_severity
                    matching_belief.radius_est = distorted_radius
                    matching_belief.freshness = min(matching_belief.freshness, sender_hb.freshness + 0.05)
                    matching_belief.source_credibility = transferred_credibility
                    matching_belief.hop_count = min(matching_belief.hop_count, sender_hb.hop_count + 1)
                    transferred = True
                continue

            receiver_beliefs.hazard_beliefs.append(
                HazardBelief(
                    position=sender_hb.position,
                    severity_est=distorted_severity,
                    radius_est=distorted_radius,
                    freshness=sender_hb.freshness + 0.05,
                    source_credibility=transferred_credibility,
                    hop_count=sender_hb.hop_count + 1,
                )
            )
            transferred = True

        # transfer general danger level (emotional contagion)
        if sender_beliefs.general_danger_level > receiver_beliefs.general_danger_level + 0.1:
            blended = (
                receiver_beliefs.general_danger_level * (1.0 - transfer_prob * 0.5)
                + sender_beliefs.general_danger_level * transfer_prob * 0.5
            )
            receiver_beliefs.general_danger_level = min(1.0, blended)
            transferred = True

        return transferred

    def broadcast(
        self,
        broadcaster_beliefs: BeliefVector,
        receivers: List[Tuple[BeliefVector, float, float, str, float]],
        broadcaster_credibility: float = 1.0,
    ) -> int:
        """
        Broadcast information from a single source (e.g. responder, PA) to multiple receivers.

        receivers: list of (beliefs, rationality, distance, state, agent_credibility) tuples.
        Returns count of agents that received information.
        """
        count = 0
        for recv_beliefs, rationality, distance, state, _ in receivers:
            if self.exchange(
                broadcaster_beliefs,
                recv_beliefs,
                broadcaster_credibility,
                rationality,
                "CALM", # broadcaster assumed calm (responder/PA)
                distance,
            ):
                count += 1
        return count
