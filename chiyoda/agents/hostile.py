"""Hostile active-shooter agent."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from chiyoda.agents.base import CognitiveAgent
from chiyoda.navigation.line_of_sight import line_of_sight


@dataclass
class HostileAgent(CognitiveAgent):
    range_m: float = 8.0
    accuracy: float = 0.35
    target_agent_id: int | None = None
    is_hostile: bool = True

    def update_intention(self, simulation) -> None:
        self.intention = "HOSTILE_PURSUIT"

    def update_navigation(self, navigator, simulation) -> None:
        target = self._select_target(simulation)
        if target is None:
            return
        self.target_agent_id = target.id
        start = simulation._grid_cell(self)
        goal = simulation._grid_cell(target)
        path = navigator.find_optimal_path(start, [goal], route_kind="target")
        if path:
            self.current_path = path
            self.path_index = 0
            self.target_exit = path[-1]
            self.last_navigation_step = simulation.current_step

    def step(self, dt: float, simulation) -> None:
        super().step(dt, simulation)
        target = (
            simulation.agent_lookup.get(self.target_agent_id)
            if self.target_agent_id is not None
            else None
        )
        if target is None or target.has_evacuated:
            target = self._select_target(simulation)
            self.target_agent_id = target.id if target is not None else None
        if target is None:
            return
        if not self._can_see(target, simulation):
            return
        load = float(self.accuracy) * float(dt)
        target.hazard_exposure += load
        target.hazard_risk = max(
            float(getattr(target, "hazard_risk", 0.0)), float(self.accuracy)
        )
        target.state = "PANICKED"
        simulation.hostile_agent_events.append(
            {
                "step": int(simulation.current_step),
                "time_s": float(simulation.time_s),
                "hostile_agent_id": int(self.id),
                "target_agent_id": int(target.id),
                "distance": float(np.linalg.norm(self.pos - target.pos)),
                "accuracy": float(self.accuracy),
            }
        )

    def _select_target(self, simulation):
        candidates = [
            agent
            for agent in simulation._active_agents()
            if not getattr(agent, "is_hostile", False)
            and not getattr(agent, "is_responder", False)
        ]
        visible = [agent for agent in candidates if self._can_see(agent, simulation)]
        pool = visible or candidates
        if not pool:
            return None
        return min(pool, key=lambda agent: float(np.linalg.norm(self.pos - agent.pos)))

    def _can_see(self, target, simulation) -> bool:
        return line_of_sight(
            simulation.layout, self.pos, target.pos, max_range=self.range_m
        )
