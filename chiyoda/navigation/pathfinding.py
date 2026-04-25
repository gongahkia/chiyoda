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
import numpy as np


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
        base = attr.get("weight", 1.0)
        penalty = 0.0
        if self.density_fn is not None:
            penalty += 0.5 * self.density_fn(v)
        # use believed hazard info
        point = np.array([v[0] + 0.5, v[1] + 0.5])
        for hb in hazard_beliefs:
            dist = np.sqrt((point[0] - hb.position[0])**2 + (point[1] - hb.position[1])**2)
            if hb.radius_est > 0 and dist <= hb.radius_est:
                penalty += 1.25 * hb.severity_est * max(0.0, 1.0 - dist / hb.radius_est)
        return base + penalty

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
        return best
