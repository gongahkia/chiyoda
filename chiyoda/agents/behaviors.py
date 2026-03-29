from __future__ import annotations

import random
from dataclasses import dataclass


STATES = {
    "CALM": {"speed_mult": 1.0, "rationality": 1.0},
    "ANXIOUS": {"speed_mult": 1.15, "rationality": 0.8},
    "PANICKED": {"speed_mult": 1.4, "rationality": 0.4},
    "FROZEN": {"speed_mult": 0.0, "rationality": 0.0},
    "HELPING": {"speed_mult": 0.7, "rationality": 1.0},
}


class BehaviorModel:
    """Simple behavior updater with panic contagion influenced by density."""

    def update_agent(self, agent, simulation) -> None:
        if not agent.is_released(simulation) or agent.has_evacuated:
            return

        # Local density-based panic probability
        local_density = 0.0
        if simulation.spatial_index is not None:
            k = len(simulation.spatial_index.neighbor_agents(agent.pos, radius=1.5))
            local_density = max(0, k) / 8.0  # heuristic normalization

        # Nearby panicked agents influence
        nearby_panic = 0
        if simulation.spatial_index is not None:
            for other in simulation.spatial_index.neighbor_agents(agent.pos, radius=2.0):
                if getattr(other, "state", "CALM") == "PANICKED":
                    nearby_panic += 1

        panic_prob = 0.2 * local_density + 0.1 * (nearby_panic / 5.0)
        panic_prob += min(0.25, float(agent.current_hazard_load) * 0.15)

        # Randomly change state based on probability
        if agent.state != "PANICKED" and random.random() < panic_prob:
            agent.state = "PANICKED"

        # Update speed multiplier based on state
        agent.speed_multiplier = STATES.get(agent.state, STATES["CALM"]) ["speed_mult"]
