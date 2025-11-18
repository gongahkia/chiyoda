from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import List, Optional, Dict, Any, Tuple

import numpy as np


@dataclass
class SimulationConfig:
    max_steps: int = 500
    dt: float = 0.1
    random_seed: Optional[int] = 42


class Simulation:
    """
    Chiyoda v2 Simulation runtime

    Responsibilities:
    - Hold references to environment, agents, and systems
    - Advance the simulation clock and step agents
    - Collect simple metrics during the run
    """

    def __init__(
        self,
        layout,
        agents: List["AgentBase"],
        exits: List["Exit"],
        hazards: Optional[List["Hazard"]] = None,
        config: Optional[SimulationConfig] = None,
    ) -> None:
        self.layout = layout
        self.agents = agents
        self.exits = exits
        self.hazards = hazards or []
        self.config = config or SimulationConfig()

        if self.config.random_seed is not None:
            np.random.seed(self.config.random_seed)

        # runtime state
        self.current_step: int = 0
        self.time_s: float = 0.0
        self.completed_agents: List["AgentBase"] = []
        self.evacuated_at_step: List[int] = []
        self.density_history: List[float] = []
        self.risk_events: List[Dict[str, Any]] = []

        # Lazy-initialized systems (wired later)
        self.navigator = None  # type: ignore
        self.spatial_index = None  # type: ignore
        self.behavior_model = None  # type: ignore

    # Wiring helpers (to avoid hard deps at import time)
    def attach_navigation(self, navigator) -> None:
        self.navigator = navigator

    def attach_spatial_index(self, spatial_index) -> None:
        self.spatial_index = spatial_index

    def attach_behavior_model(self, behavior_model) -> None:
        self.behavior_model = behavior_model

    def step(self) -> None:
        """Advance one simulation step."""
        dt = self.config.dt

        # Update hazards
        for hz in self.hazards:
            hz.step(dt, self)

        # Update spatial index
        if self.spatial_index is not None:
            self.spatial_index.update(self.agents)

        # Step agents
        for agent in list(self.agents):
            if agent.has_evacuated:
                continue

            if self.behavior_model is not None:
                self.behavior_model.update_agent(agent, self)

            # Plan route if needed
            if self.navigator is not None:
                agent.update_navigation(self.navigator, self)

            # Move agent
            agent.step(dt, self)

            # Check evacuation
            if self.layout.is_exit(agent.pos):
                agent.has_evacuated = True
                self.completed_agents.append(agent)
                self.evacuated_at_step.append(self.current_step)

        # Compute simple density metric (avg neighbors within radius)
        if self.spatial_index is not None and self.agents:
            neighbors = [
                len(self.spatial_index.find_neighbors(agent.pos, radius=1.5)) - 1
                for agent in self.agents
                if not agent.has_evacuated
            ]
            self.density_history.append(float(np.mean(neighbors)) if neighbors else 0.0)
        else:
            self.density_history.append(0.0)

        self.current_step += 1
        self.time_s += dt

    def run(self, visualize: bool = False, visualizer=None) -> None:
        """Run simulation until completion or max_steps.

        If visualize is True and a visualizer is provided, it will be updated each step.
        """
        for _ in range(self.config.max_steps):
            # End if everyone has evacuated
            if all(a.has_evacuated for a in self.agents):
                break

            self.step()

            if visualize and visualizer is not None:
                visualizer.on_step(self)

    # Convenience for visualization layers
    def live_state(self) -> Dict[str, Any]:
        """Return current state snapshot for visualization."""
        living_agents = [a for a in self.agents if not a.has_evacuated]
        positions = np.array([a.pos for a in living_agents]) if living_agents else np.zeros((0, 2))
        return {
            "step": self.current_step,
            "time_s": self.time_s,
            "positions": positions,
            "exits": [e.pos for e in self.exits],
            "hazards": [h.snapshot() for h in self.hazards],
            "density": self.density_history[-1] if self.density_history else 0.0,
            "evacuated": len(self.completed_agents),
            "remaining": len(living_agents),
        }
