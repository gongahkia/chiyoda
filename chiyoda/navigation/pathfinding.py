"""
Belief-weighted pathfinding for ITED framework.

Agents plan paths based on their *believed* environment state, not ground truth.
Agents who don't know about an exit can't path to it.
Agents with stale hazard info may path through contaminated zones.
Exploration mode: random walk with wall-following heuristic.
"""
from __future__ import annotations
from typing import List, Optional, Tuple
import networkx as nx


class SmartNavigator:
    """
    Grid graph navigator with belief-weighted edge costs.

    Edge weights incorporate:
    - base traversal cost (1.0 per cell)
    - density penalty (from spatial index)
    - hazard penalty (from simulation ground truth OR agent beliefs)
    - visibility penalty (prefer well-lit / visible paths)
    """

    def __init__(self, layout, density_fn=None, hazard_fn=None) -> None:
        self.layout = layout
        self.graph = self._build_graph(layout)
        self.density_fn = density_fn
        self.hazard_fn = hazard_fn
        self._path_cache: dict[tuple, Optional[List[Tuple[int, int]]]] = {}
        self._weight_cache: dict[tuple, float] = {}

    def clear_cache(self) -> None:
        """Clear per-step path cache after density or hazard state changes."""
        self._path_cache.clear()
        self._weight_cache.clear()

    def _build_graph(self, layout) -> nx.Graph:
        G = nx.Graph()
        h, w = layout.height, layout.width
        for y in range(h):
            for x in range(w):
                if layout.is_walkable((x, y)):
                    G.add_node((x, y))
        for y in range(h):
            for x in range(w):
                if not layout.is_walkable((x, y)):
                    continue
                for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)]:
                    nx_, ny_ = x + dx, y + dy
                    if 0 <= nx_ < w and 0 <= ny_ < h and layout.is_walkable((nx_, ny_)):
                        G.add_edge((x, y), (nx_, ny_), weight=1.0)
        return G

    def _weight(self, u: Tuple[int, int], v: Tuple[int, int], attr: dict) -> float:
        base = attr.get("weight", 1.0)
        penalty = 0.0
        if self.density_fn is not None:
            penalty += 0.5 * self.density_fn(v)
        if self.hazard_fn is not None:
            penalty += self.hazard_fn(v)
        return base + penalty

    def _belief_weight(
        self,
        u: Tuple[int, int],
        v: Tuple[int, int],
        attr: dict,
        hazard_beliefs: list,
    ) -> float:
        """Edge weight using agent's hazard beliefs instead of ground truth."""
        hazard_sig = self._hazard_signature(hazard_beliefs)
        cache_key = (v, hazard_sig)
        cached = self._weight_cache.get(cache_key)
        if cached is not None:
            return cached

        base = attr.get("weight", 1.0)
        penalty = 0.0
        if self.density_fn is not None:
            penalty += 0.5 * self.density_fn(v)
        # use believed hazard info
        px = v[0] + 0.5
        py = v[1] + 0.5
        for hb in hazard_beliefs:
            radius = float(hb.radius_est)
            if radius <= 0:
                continue
            dx = px - hb.position[0]
            dy = py - hb.position[1]
            dist_sq = dx * dx + dy * dy
            radius_sq = radius * radius
            if dist_sq <= radius_sq:
                dist = dist_sq ** 0.5
                penalty += 1.25 * hb.severity_est * max(0.0, 1.0 - dist / radius)
        value = base + penalty
        self._weight_cache[cache_key] = value
        return value

    def find_optimal_path(
        self,
        start: Tuple[int, int],
        goals: List[Tuple[int, int]],
        hazard_beliefs: Optional[list] = None,
    ) -> Optional[List[Tuple[int, int]]]:
        """
        Find optimal path from start to nearest goal.

        If hazard_beliefs is provided, uses belief-weighted costs.
        Otherwise falls back to ground-truth hazard function.
        """
        cache_key = (
            tuple(start),
            tuple(sorted(tuple(goal) for goal in goals)),
            self._hazard_signature(hazard_beliefs),
        )
        if cache_key in self._path_cache:
            cached = self._path_cache[cache_key]
            return list(cached) if cached is not None else None

        best = None
        best_len = float("inf")
        for goal in goals:
            if goal not in self.graph or start not in self.graph:
                continue
            try:
                if hazard_beliefs is not None:
                    weight_fn = lambda u, v, attr: self._belief_weight(u, v, attr, hazard_beliefs)
                else:
                    weight_fn = lambda u, v, attr: self._weight(u, v, attr)

                path = nx.astar_path(
                    self.graph,
                    start,
                    goal,
                    heuristic=lambda a, b: abs(a[0] - b[0]) + abs(a[1] - b[1]),
                    weight=weight_fn,
                )
                length = 0.0
                for u, v in zip(path[:-1], path[1:]):
                    edge_attr = self.graph[u][v]
                    if hazard_beliefs is not None:
                        length += self._belief_weight(u, v, edge_attr, hazard_beliefs)
                    else:
                        length += self._weight(u, v, edge_attr)
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
                round(float(hb.position[0]), 2),
                round(float(hb.position[1]), 2),
                round(float(hb.severity_est), 3),
                round(float(hb.radius_est), 3),
            )
            for hb in hazard_beliefs
        )
