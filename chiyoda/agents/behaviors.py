"""
Multi-factor behavioral state machine for ITED agents.

State transitions driven by information entropy, hazard exposure,
social influence, and time pressure. Replaces naive panic contagion.
"""

from __future__ import annotations

import random
from dataclasses import dataclass
from typing import Optional

import numpy as np


STATES = {
    "CALM": {"speed_mult": 1.0, "rationality": 1.0},
    "ANXIOUS": {"speed_mult": 1.15, "rationality": 0.8},
    "PANICKED": {"speed_mult": 1.4, "rationality": 0.4},
    "FROZEN": {"speed_mult": 0.0, "rationality": 0.0},
    "HELPING": {"speed_mult": 0.7, "rationality": 1.0},
    "EXPLORING": {"speed_mult": 0.9, "rationality": 0.9},
    "FOLLOWING": {"speed_mult": 1.1, "rationality": 0.6},
}


@dataclass
class BehaviorConfig:
    density_panic_weight: float = 0.2  # density contribution to panic prob
    neighbor_panic_weight: float = 0.1  # panicked neighbors contribution
    hazard_panic_weight: float = 0.15  # hazard load contribution
    entropy_anxiety_weight: float = 0.25  # information uncertainty → anxiety
    freeze_probability: float = 0.02  # base freeze probability when panicked
    calm_recovery_rate: float = 0.005  # probability of de-escalation per step
    helping_threshold: float = 0.7  # impairment level at which nearby agents help


class BehaviorModel:
    """Multi-factor behavior updater with entropy-driven state transitions."""

    def __init__(self, config: Optional[BehaviorConfig] = None) -> None:
        self.config = config or BehaviorConfig()

    def update_agent(self, agent, simulation) -> None:
        if not agent.is_released(simulation) or agent.has_evacuated:
            return

        cfg = self.config

        # gather context
        local_density = 0.0
        nearby_panic = 0
        nearby_impaired = 0
        if simulation.spatial_index is not None:
            neighbors = simulation.spatial_index.neighbor_agents(agent.pos, radius=2.0)
            local_density = max(0, len(neighbors)) / 8.0
            for other in neighbors:
                if getattr(other, "state", "CALM") == "PANICKED":
                    nearby_panic += 1
                if hasattr(other, "physiology") and other.physiology.impairment_level > cfg.helping_threshold:
                    nearby_impaired += 1

        # information entropy (high entropy = high uncertainty = anxiety)
        agent_entropy = 0.0
        if hasattr(agent, "beliefs"):
            from chiyoda.information.entropy import agent_entropy as calc_entropy
            total_exits = len(simulation.exits)
            total_hazards = len(simulation.hazards)
            agent_entropy = calc_entropy(agent.beliefs, total_exits, total_hazards)

        hazard_load = getattr(agent, "current_hazard_load", 0.0)

        # compute transition probabilities
        panic_prob = (
            cfg.density_panic_weight * local_density
            + cfg.neighbor_panic_weight * (nearby_panic / 5.0)
            + cfg.hazard_panic_weight * min(1.0, hazard_load)
            + cfg.entropy_anxiety_weight * agent_entropy
        )

        anxiety_prob = panic_prob * 0.6 # anxiety is easier to trigger

        # state transitions
        current = agent.state

        if current == "CALM":
            if random.random() < anxiety_prob:
                agent.state = "ANXIOUS"
            elif agent_entropy > 0.7 and random.random() < 0.1:
                agent.state = "EXPLORING" if agent.rationality > 0.5 else "FOLLOWING"

        elif current == "ANXIOUS":
            if random.random() < panic_prob:
                agent.state = "PANICKED"
            elif random.random() < cfg.calm_recovery_rate and hazard_load < 0.1:
                agent.state = "CALM"
            elif agent_entropy > 0.8:
                agent.state = "FOLLOWING" # high uncertainty → herd

        elif current == "PANICKED":
            if random.random() < cfg.freeze_probability:
                agent.state = "FROZEN"
            elif random.random() < cfg.calm_recovery_rate * 0.5:
                agent.state = "ANXIOUS"

        elif current == "FROZEN":
            # slow recovery
            if random.random() < cfg.calm_recovery_rate * 0.2 and hazard_load < 0.05:
                agent.state = "ANXIOUS"

        elif current in ("EXPLORING", "FOLLOWING"):
            if random.random() < panic_prob:
                agent.state = "PANICKED"
            elif agent_entropy < 0.3: # gained enough info
                agent.state = "CALM"

        elif current == "HELPING":
            pass # HELPING state is managed externally

        # helping behavior: calm agents near impaired agents may switch
        if current == "CALM" and nearby_impaired > 0 and random.random() < 0.15:
            agent.state = "HELPING"

        # update speed multiplier
        state_params = STATES.get(agent.state, STATES["CALM"])
        agent.speed_multiplier = state_params["speed_mult"]

        # credibility decreases when panicked
        if hasattr(agent, "credibility"):
            if agent.state == "PANICKED":
                agent.credibility = max(0.1, agent.credibility * 0.95)
            elif agent.state in ("CALM", "HELPING"):
                agent.credibility = min(1.0, agent.credibility * 1.01)
