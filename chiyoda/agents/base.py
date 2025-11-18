from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Tuple, Optional
import numpy as np


@dataclass
class AgentBase:
    id: int
    pos: np.ndarray  # [x, y] float coordinates
    base_speed: float = 1.34  # m/s typical walking speed
    has_evacuated: bool = False

    # Navigation state
    current_path: List[Tuple[int, int]] = field(default_factory=list)
    path_index: int = 0
    target_exit: Optional[Tuple[int, int]] = None

    # Behavior attributes
    state: str = "CALM"
    speed_multiplier: float = 1.0

    def speed(self) -> float:
        return self.base_speed * self.speed_multiplier

    def update_navigation(self, navigator, simulation) -> None:
        """Refresh navigation path periodically or when path depleted."""
        if self.has_evacuated:
            return

        needs_path = (not self.current_path) or (self.path_index >= len(self.current_path))
        if needs_path or self.target_exit is None:
            # Compute best exit if not set
            exits = [tuple(e.pos) if hasattr(e, "pos") else e for e in simulation.exits]
            # Convert to tuples
            exit_coords = [tuple(map(int, ex)) for ex in exits]
            start = (int(round(self.pos[0])), int(round(self.pos[1])))
            path = navigator.find_optimal_path(start, exit_coords)
            if path:
                self.current_path = path
                self.path_index = 0
                self.target_exit = path[-1]

    def step(self, dt: float, simulation) -> None:
        if self.has_evacuated:
            return

        # If we have a path, move towards next waypoint
        waypoint = None
        if self.current_path and self.path_index < len(self.current_path):
            waypoint = self.current_path[self.path_index]

        if waypoint is not None:
            target = np.array([waypoint[0] + 0.5, waypoint[1] + 0.5])
            direction = target - self.pos
            dist = np.linalg.norm(direction)
            if dist > 1e-6:
                direction = direction / dist
            step_vec = direction * self.speed() * dt
            new_pos = self.pos + step_vec

            # If we reached the cell center reasonably, advance path
            if np.linalg.norm(target - new_pos) < 0.2:
                self.path_index += 1

            # Check for walls; if new pos invalid, stay in place this tick
            if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                self.pos = new_pos
        else:
            # Fallback: small random walk towards nearest exit
            exits = simulation.layout.exit_positions()
            if exits:
                goal = np.array(exits[0], dtype=float)
                direction = goal - self.pos
                dist = np.linalg.norm(direction)
                if dist > 1e-6:
                    direction = direction / dist
                noise = np.random.randn(2) * 0.05
                new_pos = self.pos + (direction + noise) * self.speed() * dt
                if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                    self.pos = new_pos
