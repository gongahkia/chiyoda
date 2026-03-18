from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Tuple

import numpy as np

from chiyoda.navigation.social_force import adjusted_step


@dataclass
class AgentBase:
    id: int
    pos: np.ndarray  # [x, y] float coordinates
    base_speed: float = 1.34  # m/s typical walking speed
    has_evacuated: bool = False

    current_path: List[Tuple[int, int]] = field(default_factory=list)
    path_index: int = 0
    target_exit: Optional[Tuple[int, int]] = None

    state: str = "CALM"
    speed_multiplier: float = 1.0
    crowd_speed_factor: float = 1.0
    current_speed: float = 0.0
    local_density: float = 0.0
    travel_time_s: float = 0.0
    last_navigation_step: int = -9999

    def speed(self) -> float:
        return self.base_speed * self.speed_multiplier * self.crowd_speed_factor

    def update_navigation(self, navigator, simulation) -> None:
        """Refresh navigation path periodically or when congestion rises."""
        if self.has_evacuated:
            return

        needs_path = (not self.current_path) or (self.path_index >= len(self.current_path))
        stale_path = (
            simulation.current_step - self.last_navigation_step
            >= simulation.navigation_replan_interval_steps
        )
        congestion_trigger = (
            self.local_density >= simulation.navigation_density_reroute_threshold
            and simulation.current_step > self.last_navigation_step
        )

        if not (needs_path or self.target_exit is None or stale_path or congestion_trigger):
            return

        exits = [tuple(e.pos) if hasattr(e, "pos") else e for e in simulation.exits]
        exit_coords = [tuple(map(int, ex)) for ex in exits]
        start = (int(round(self.pos[0])), int(round(self.pos[1])))
        path = navigator.find_optimal_path(start, exit_coords)
        if path:
            self.current_path = path
            self.path_index = 0
            self.target_exit = path[-1]
            self.last_navigation_step = simulation.current_step

    def step(self, dt: float, simulation) -> None:
        if self.has_evacuated:
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
                direction = direction / dist

            desired_step = direction * self.speed() * dt
            neighbors = (
                simulation.spatial_index.neighbor_positions(self.pos, radius=1.0)
                if simulation.spatial_index is not None
                else np.zeros((0, 2), dtype=float)
            )
            adjusted = adjusted_step(
                current_pos=self.pos,
                desired_step=desired_step,
                neighbors=neighbors,
                walls=[],
                dt=dt,
            )
            new_pos = self.pos + adjusted

            if np.linalg.norm(target - new_pos) < 0.2:
                self.path_index += 1

            if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                self.pos = new_pos
        else:
            exits = simulation.layout.exit_positions()
            if exits:
                goal = np.array(exits[0], dtype=float)
                direction = goal - self.pos
                dist = np.linalg.norm(direction)
                if dist > 1e-6:
                    direction = direction / dist
                noise = np.random.randn(2) * 0.03
                desired_step = (direction + noise) * self.speed() * dt
                neighbors = (
                    simulation.spatial_index.neighbor_positions(self.pos, radius=1.0)
                    if simulation.spatial_index is not None
                    else np.zeros((0, 2), dtype=float)
                )
                adjusted = adjusted_step(
                    current_pos=self.pos,
                    desired_step=desired_step,
                    neighbors=neighbors,
                    walls=[],
                    dt=dt,
                )
                new_pos = self.pos + adjusted
                if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                    self.pos = new_pos
