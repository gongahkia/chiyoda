"""
FirstResponder — counter-flow agent with PPE, high credibility,
and mission-oriented pathfinding toward hazard source.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Optional, Tuple
import numpy as np
from chiyoda.agents.base import CognitiveAgent, INTENTION_RESPOND, INTENTION_FREEZE
from chiyoda.information.field import BeliefVector
from chiyoda.navigation.social_force import adjusted_step

@dataclass
class FirstResponder(CognitiveAgent):
    mission_target: Optional[Tuple[float, float]] = None
    ppe_factor: float = 0.1 # 90% PPE protection
    broadcast_radius: float = 5.0
    is_responder: bool = True

    def __post_init__(self):
        self.credibility = 1.0
        self.familiarity = 1.0
        self.gossip_radius = self.broadcast_radius
        self.base_rationality = 1.0
        self.rationality = 1.0
        self.intention = INTENTION_RESPOND
        self.state = "CALM"
        self.base_vision_radius = 8.0

    def update_physiology(self, hazard_load: float, dt: float) -> None:
        super().update_physiology(hazard_load * self.ppe_factor, dt)

    def update_intention(self, simulation) -> None:
        if self.physiology.incapacitated:
            self.intention = INTENTION_FREEZE
            return
        self.intention = INTENTION_RESPOND

    def update_navigation(self, navigator, simulation) -> None:
        if self.has_evacuated or not self.is_released(simulation):
            return
        if self.mission_target is None:
            if simulation.hazards:
                h = simulation.hazards[0]
                self.mission_target = (float(h.pos[0]), float(h.pos[1]))
            else:
                return
        needs_path = (not self.current_path) or (self.path_index >= len(self.current_path))
        stale = simulation.current_step - self.last_navigation_step >= simulation.navigation_replan_interval_steps
        if not (needs_path or stale):
            return
        start = (int(round(self.pos[0])), int(round(self.pos[1])))
        target = (int(round(self.mission_target[0])), int(round(self.mission_target[1])))
        path = navigator.find_optimal_path(start, [target])
        if path:
            self.current_path = path
            self.path_index = 0
            self.target_exit = path[-1]
            self.last_navigation_step = simulation.current_step

    def step(self, dt: float, simulation) -> None:
        if self.has_evacuated or not self.is_released(simulation):
            return
        if self.physiology.incapacitated:
            self.current_speed = 0.0
            return
        waypoint = None
        while self.current_path and self.path_index < len(self.current_path):
            candidate = self.current_path[self.path_index]
            target = np.array([candidate[0] + 0.5, candidate[1] + 0.5], dtype=float)
            if np.linalg.norm(target - self.pos) < 0.2:
                self.path_index += 1
                continue
            waypoint = candidate
            break
        if waypoint is not None:
            target = np.array([waypoint[0] + 0.5, waypoint[1] + 0.5], dtype=float)
            direction = target - self.pos
            dist = np.linalg.norm(direction)
            if dist > 1e-6:
                direction /= dist
            desired_step = direction * self.speed() * dt * 1.2
            neighbors = (
                simulation.spatial_index.neighbor_positions(self.pos, radius=1.0)
                if simulation.spatial_index is not None
                else np.zeros((0, 2), dtype=float)
            )
            adj = adjusted_step(self.pos, desired_step, neighbors, [], dt, counter_flow=True)
            new_pos = self.pos + adj
            if np.linalg.norm(target - new_pos) < 0.2:
                self.path_index += 1
            if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                self.pos = new_pos
        if self.mission_target:
            if np.linalg.norm(self.pos - np.array(self.mission_target, dtype=float)) < 2.0:
                self.mission_target = None
