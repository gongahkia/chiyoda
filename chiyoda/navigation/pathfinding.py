"""
Belief-weighted pathfinding for strict multi-floor ITED layouts.
"""

from __future__ import annotations

import heapq
import time
import warnings
from dataclasses import dataclass, field

import networkx as nx
import numpy as np

from chiyoda.environment.layout import Cell

PATHFINDING_STRATEGIES = {
    "auto",
    "networkx_astar",
    "heap_astar",
    "reverse_dijkstra",
}


@dataclass
class RouteStats:
    requested_strategy: str
    last_effective_strategy: str = ""
    route_cache_hits: int = 0
    route_cache_misses: int = 0
    path_computations: int = 0
    fallback_count: int = 0
    routing_wall_time_s: float = 0.0
    strategy_counts: dict[str, int] = field(default_factory=dict)

    def as_dict(self) -> dict[str, object]:
        effective = self.last_effective_strategy or self.requested_strategy
        if len(self.strategy_counts) > 1:
            effective = "mixed"
        return {
            "requested_pathfinding_strategy": self.requested_strategy,
            "effective_pathfinding_strategy": effective,
            "last_effective_pathfinding_strategy": self.last_effective_strategy,
            "route_cache_hits": self.route_cache_hits,
            "route_cache_misses": self.route_cache_misses,
            "path_computations": self.path_computations,
            "pathfinding_fallback_count": self.fallback_count,
            "routing_wall_time_s": round(self.routing_wall_time_s, 6),
            "pathfinding_strategy_counts": dict(sorted(self.strategy_counts.items())),
        }


class SmartNavigator:
    """Floor-aware grid graph navigator with vertical connector edges."""

    def __init__(
        self,
        layout,
        density_fn=None,
        hazard_fn=None,
        strategy: str = "auto",
        blocked_fn=None,
        edge_blocked_fn=None,
    ) -> None:
        strategy = str(strategy or "auto").lower()
        if strategy not in PATHFINDING_STRATEGIES:
            allowed = ", ".join(sorted(PATHFINDING_STRATEGIES))
            raise ValueError(f"pathfinding_strategy must be one of {allowed}")
        self.layout = layout
        self.graph = self._build_graph(layout)
        self.density_fn = density_fn
        self.hazard_fn = hazard_fn
        self.blocked_fn = blocked_fn
        self.edge_blocked_fn = edge_blocked_fn
        self.strategy = strategy
        self._path_cache: dict[tuple, list[Cell] | None] = {}
        self._weight_cache: dict[tuple, float] = {}
        self._reverse_cache: dict[tuple, tuple[dict[Cell, float], dict[Cell, Cell]]] = (
            {}
        )
        self._position_cache: dict[Cell, np.ndarray] = {}
        self.stats = RouteStats(requested_strategy=strategy)

    def clear_cache(self) -> None:
        self._path_cache.clear()
        self._weight_cache.clear()
        self._reverse_cache.clear()

    def route_stats(self) -> dict[str, object]:
        return self.stats.as_dict()

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
        if self._blocked(u) or self._blocked(v) or self._edge_blocked(u, v):
            return float("inf")
        cache_key = ("ground", u, v)
        cached = self._weight_cache.get(cache_key)
        if cached is not None:
            return cached
        base = float(attr.get("weight", 1.0))
        penalty = 0.0
        if self.density_fn is not None:
            penalty += 0.5 * self.density_fn(v)
        if self.hazard_fn is not None:
            penalty += self.hazard_fn(v)
        value = base + penalty
        self._weight_cache[cache_key] = value
        return value

    def _belief_weight(
        self, u: Cell, v: Cell, attr: dict, hazard_beliefs: list
    ) -> float:
        if self._blocked(u) or self._blocked(v) or self._edge_blocked(u, v):
            return float("inf")
        hazard_sig = self._hazard_signature(hazard_beliefs)
        cache_key = ("belief", u, v, hazard_sig)
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
        hazard_beliefs: list | None = None,
        route_kind: str = "evacuation",
    ) -> list[Cell] | None:
        start_cell = self.layout.cell(start)
        if self._blocked(start_cell):
            return None
        goal_cells = [self.layout.cell(goal) for goal in goals]
        goal_cells = [goal for goal in goal_cells if not self._blocked(goal)]
        if not goal_cells:
            return None
        hazard_sig = self._hazard_signature(hazard_beliefs)
        hazard_key = (hazard_beliefs is not None, hazard_sig)
        effective_strategy = self._effective_strategy(route_kind)
        self._record_strategy(effective_strategy)
        cache_key = (
            effective_strategy,
            str(route_kind),
            start_cell,
            tuple(sorted(goal_cells)),
            hazard_key,
        )
        if cache_key in self._path_cache:
            self.stats.route_cache_hits += 1
            cached = self._path_cache[cache_key]
            return list(cached) if cached is not None else None
        self.stats.route_cache_misses += 1

        started = time.perf_counter()
        best: list[Cell] | None = None
        try:
            best = self._find_with_strategy(
                effective_strategy, start_cell, goal_cells, hazard_beliefs, hazard_key
            )
        except Exception as exc:
            if self.strategy != "auto" or effective_strategy == "networkx_astar":
                raise
            self.stats.fallback_count += 1
            warnings.warn(
                f"{effective_strategy} failed; falling back to networkx_astar ({exc})",
                RuntimeWarning,
            )
            best = self._networkx_astar(start_cell, goal_cells, hazard_beliefs)
        self.stats.path_computations += 1
        self.stats.routing_wall_time_s += time.perf_counter() - started
        self._path_cache[cache_key] = list(best) if best is not None else None
        return best

    def _effective_strategy(self, route_kind: str) -> str:
        if self.strategy != "auto":
            return self.strategy
        if str(route_kind) == "target":
            return "heap_astar"
        return "reverse_dijkstra"

    def _record_strategy(self, strategy: str) -> None:
        self.stats.last_effective_strategy = strategy
        self.stats.strategy_counts[strategy] = (
            self.stats.strategy_counts.get(strategy, 0) + 1
        )

    def _find_with_strategy(
        self,
        strategy: str,
        start_cell: Cell,
        goal_cells: list[Cell],
        hazard_beliefs: list | None,
        hazard_key: tuple,
    ) -> list[Cell] | None:
        if strategy == "networkx_astar":
            return self._networkx_astar(start_cell, goal_cells, hazard_beliefs)
        if strategy == "heap_astar":
            return self._heap_astar(start_cell, goal_cells, hazard_beliefs)
        if strategy == "reverse_dijkstra":
            return self._reverse_dijkstra(
                start_cell, goal_cells, hazard_beliefs, hazard_key
            )
        raise ValueError(f"unknown pathfinding strategy: {strategy}")

    def _weight_fn(self, hazard_beliefs: list | None):
        if hazard_beliefs is not None:
            return lambda u, v, attr: self._belief_weight(u, v, attr, hazard_beliefs)
        return lambda u, v, attr: self._weight(u, v, attr)

    def _networkx_astar(
        self,
        start_cell: Cell,
        goal_cells: list[Cell],
        hazard_beliefs: list | None,
    ) -> list[Cell] | None:
        best: list[Cell] | None = None
        best_len = float("inf")
        weight_fn = self._weight_fn(hazard_beliefs)
        for goal in goal_cells:
            if (
                goal not in self.graph
                or start_cell not in self.graph
                or self._blocked(goal)
                or self._blocked(start_cell)
            ):
                continue
            try:
                path = nx.astar_path(
                    self.graph,
                    start_cell,
                    goal,
                    heuristic=self._heuristic,
                    weight=weight_fn,
                )
                length = self._path_cost(path, weight_fn)
                if np.isfinite(length) and length < best_len:
                    best = path
                    best_len = length
            except nx.NetworkXNoPath:
                continue
        return best

    def _heap_astar(
        self,
        start_cell: Cell,
        goal_cells: list[Cell],
        hazard_beliefs: list | None,
    ) -> list[Cell] | None:
        if start_cell not in self.graph or self._blocked(start_cell):
            return None
        weight_fn = self._weight_fn(hazard_beliefs)
        best_path: list[Cell] | None = None
        best_cost = float("inf")
        for goal in goal_cells:
            if goal not in self.graph or self._blocked(goal):
                continue
            path = self._heap_astar_one(start_cell, goal, weight_fn)
            if path is None:
                continue
            cost = self._path_cost(path, weight_fn)
            if np.isfinite(cost) and cost < best_cost:
                best_path = path
                best_cost = cost
        return best_path

    def _heap_astar_one(self, start: Cell, goal: Cell, weight_fn) -> list[Cell] | None:
        queue: list[tuple[float, int, Cell]] = []
        counter = 0
        g_score: dict[Cell, float] = {start: 0.0}
        came_from: dict[Cell, Cell] = {}
        heapq.heappush(queue, (self._heuristic(start, goal), counter, start))
        closed: set[Cell] = set()
        while queue:
            _, _, current = heapq.heappop(queue)
            if current in closed:
                continue
            if current == goal:
                return self._reconstruct(came_from, current)
            closed.add(current)
            base = g_score[current]
            for neighbor, attr in self.graph[current].items():
                if (
                    neighbor in closed
                    or self._blocked(neighbor)
                    or self._edge_blocked(current, neighbor)
                ):
                    continue
                candidate = base + weight_fn(current, neighbor, attr)
                if not np.isfinite(candidate):
                    continue
                if candidate >= g_score.get(neighbor, float("inf")):
                    continue
                came_from[neighbor] = current
                g_score[neighbor] = candidate
                counter += 1
                priority = candidate + self._heuristic(neighbor, goal)
                heapq.heappush(queue, (priority, counter, neighbor))
        return None

    def _reverse_dijkstra(
        self,
        start_cell: Cell,
        goal_cells: list[Cell],
        hazard_beliefs: list | None,
        hazard_key: tuple,
    ) -> list[Cell] | None:
        goals = tuple(
            sorted(
                goal
                for goal in goal_cells
                if goal in self.graph and not self._blocked(goal)
            )
        )
        if start_cell not in self.graph or self._blocked(start_cell) or not goals:
            return None
        cache_key = (goals, hazard_key)
        cached = self._reverse_cache.get(cache_key)
        if cached is None:
            cached = self._build_reverse_dijkstra(goals, hazard_beliefs)
            self._reverse_cache[cache_key] = cached
        distance, next_hop = cached
        if start_cell not in distance:
            return None
        goal_set = set(goals)
        path = [start_cell]
        current = start_cell
        visited = {current}
        while current not in goal_set:
            current = next_hop.get(current)
            if current is None or current in visited:
                return None
            path.append(current)
            visited.add(current)
        return path

    def _build_reverse_dijkstra(
        self, goals: tuple[Cell, ...], hazard_beliefs: list | None
    ) -> tuple[dict[Cell, float], dict[Cell, Cell]]:
        weight_fn = self._weight_fn(hazard_beliefs)
        distance: dict[Cell, float] = {}
        next_hop: dict[Cell, Cell] = {}
        queue: list[tuple[float, int, Cell]] = []
        counter = 0
        for goal in goals:
            distance[goal] = 0.0
            heapq.heappush(queue, (0.0, counter, goal))
            counter += 1
        while queue:
            base, _, current = heapq.heappop(queue)
            if base > distance.get(current, float("inf")):
                continue
            for predecessor, attr in self.graph.pred[current].items():
                if (
                    self._blocked(current)
                    or self._blocked(predecessor)
                    or self._edge_blocked(predecessor, current)
                ):
                    continue
                candidate = base + weight_fn(predecessor, current, attr)
                if not np.isfinite(candidate):
                    continue
                if candidate >= distance.get(predecessor, float("inf")):
                    continue
                distance[predecessor] = candidate
                next_hop[predecessor] = current
                heapq.heappush(queue, (candidate, counter, predecessor))
                counter += 1
        return distance, next_hop

    def _path_cost(self, path: list[Cell], weight_fn) -> float:
        total = 0.0
        for u, v in zip(path[:-1], path[1:], strict=False):
            total += weight_fn(u, v, self.graph[u][v])
        return total

    def _heuristic(self, a: Cell, b: Cell) -> float:
        return float(np.linalg.norm(self._world_position(a) - self._world_position(b)))

    def _world_position(self, cell: Cell) -> np.ndarray:
        cached = self._position_cache.get(cell)
        if cached is not None:
            return cached
        value = self.layout.world_position(cell)
        self._position_cache[cell] = value
        return value

    def _reconstruct(self, came_from: dict[Cell, Cell], current: Cell) -> list[Cell]:
        path = [current]
        while current in came_from:
            current = came_from[current]
            path.append(current)
        path.reverse()
        return path

    def _blocked(self, cell: Cell) -> bool:
        return bool(self.blocked_fn is not None and self.blocked_fn(cell))

    def _edge_blocked(self, source: Cell, target: Cell) -> bool:
        return bool(
            self.edge_blocked_fn is not None and self.edge_blocked_fn(source, target)
        )

    def _hazard_signature(self, hazard_beliefs: list | None) -> tuple:
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
