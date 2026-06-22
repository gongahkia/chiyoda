"""
Belief-weighted pathfinding for strict multi-floor ITED layouts.
"""

from __future__ import annotations

from typing import Optional

import networkx as nx
import numpy as np

from chiyoda.environment.layout import Cell


class SmartNavigator:
    """Floor-aware grid graph navigator with vertical connector edges."""

    def __init__(self, layout, density_fn=None, hazard_fn=None) -> None:
        self.layout = layout
        self.graph = self._build_graph(layout)
        self.density_fn = density_fn
        self.hazard_fn = hazard_fn
        self._path_cache: dict[tuple, Optional[list[Cell]]] = {}
        self._weight_cache: dict[tuple, float] = {}

    def clear_cache(self) -> None:
        self._path_cache.clear()
        self._weight_cache.clear()

    def _build_graph(self, layout) -> nx.Graph:
        graph = nx.DiGraph()
        for cell in layout.all_walkable_cells():
            graph.add_node(cell)
        for floor_id, floor in layout.floors.items():
            height, width = floor.grid.shape
            for y in range(height):
                for x in range(width):
                    cell = (floor_id, x, y)
                    if not layout.is_walkable(cell):
                        continue
                    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                        target = (floor_id, x + dx, y + dy)
                        if layout.is_walkable(target):
                            graph.add_edge(cell, target, weight=1.0, connector=None)
        for source, target, connector in layout.connector_edges():
            if layout.is_walkable(source) and layout.is_walkable(target):
                weight = self._connector_weight(source, target, connector)
                graph.add_edge(source, target, weight=weight, connector=connector.id)
        return graph

    def _connector_weight(self, source: Cell, target: Cell, connector) -> float:
        if connector.type == "elevator":
            if connector.travel_s > 0:
                return max(1.0, connector.travel_s + connector.dwell_s)
        source_pos = self.layout.world_position(source)
        target_pos = self.layout.world_position(target)
        distance = float(np.linalg.norm(target_pos - source_pos))
        return max(1.0, distance / max(connector.speed_multiplier, 1e-6))

    def _weight(self, u: Cell, v: Cell, attr: dict) -> float:
        base = float(attr.get("weight", 1.0))
        penalty = 0.0
        if self.density_fn is not None:
            penalty += 0.5 * self.density_fn(v)
        if self.hazard_fn is not None:
            penalty += self.hazard_fn(v)
        return base + penalty

    def _belief_weight(
        self, u: Cell, v: Cell, attr: dict, hazard_beliefs: list
    ) -> float:
        hazard_sig = self._hazard_signature(hazard_beliefs)
        cache_key = (v, hazard_sig)
        cached = self._weight_cache.get(cache_key)
        if cached is not None:
            return cached

        base = float(attr.get("weight", 1.0))
        penalty = 0.0
        if self.density_fn is not None:
            penalty += 0.5 * self.density_fn(v)
        point = self.layout.world_position(v)
        for belief in hazard_beliefs:
            radius = float(belief.radius_est)
            if radius <= 0:
                continue
            hazard_pos = np.array(_belief_position_3d(belief.position), dtype=float)
            dist = float(np.linalg.norm(point - hazard_pos))
            if dist <= radius:
                penalty += 1.25 * belief.severity_est * max(0.0, 1.0 - dist / radius)
        value = base + penalty
        self._weight_cache[cache_key] = value
        return value

    def find_optimal_path(
        self,
        start,
        goals,
        hazard_beliefs: Optional[list] = None,
    ) -> Optional[list[Cell]]:
        start_cell = self.layout.cell(start)
        goal_cells = [self.layout.cell(goal) for goal in goals]
        cache_key = (
            start_cell,
            tuple(sorted(goal_cells)),
            self._hazard_signature(hazard_beliefs),
        )
        if cache_key in self._path_cache:
            cached = self._path_cache[cache_key]
            return list(cached) if cached is not None else None

        best: list[Cell] | None = None
        best_len = float("inf")
        for goal in goal_cells:
            if goal not in self.graph or start_cell not in self.graph:
                continue
            try:
                weight_fn = (
                    (lambda u, v, attr: self._belief_weight(u, v, attr, hazard_beliefs))
                    if hazard_beliefs is not None
                    else (lambda u, v, attr: self._weight(u, v, attr))
                )
                path = nx.astar_path(
                    self.graph,
                    start_cell,
                    goal,
                    heuristic=lambda a, b: float(
                        np.linalg.norm(
                            self.layout.world_position(a)
                            - self.layout.world_position(b)
                        )
                    ),
                    weight=weight_fn,
                )
                length = 0.0
                for u, v in zip(path[:-1], path[1:]):
                    attr = self.graph[u][v]
                    length += weight_fn(u, v, attr)
                if length < best_len:
                    best = path
                    best_len = length
            except nx.NetworkXNoPath:
                continue
        self._path_cache[cache_key] = list(best) if best is not None else None
        return best

    def _hazard_signature(self, hazard_beliefs: Optional[list]) -> tuple:
        if hazard_beliefs is None:
            return ()
        return tuple(
            (
                tuple(
                    round(float(value), 2)
                    for value in _belief_position_3d(belief.position)
                ),
                round(float(belief.severity_est), 3),
                round(float(belief.radius_est), 3),
            )
            for belief in hazard_beliefs
        )


def _belief_position_3d(position) -> tuple[float, float, float]:
    if len(position) >= 3:
        return float(position[0]), float(position[1]), float(position[2])
    return float(position[0]), float(position[1]), 0.0
