from __future__ import annotations

from typing import List, Tuple, Optional

import networkx as nx


class SmartNavigator:
    """
    Build a grid graph from the layout and compute shortest paths.
    Edge weights can be dynamically adjusted by a provided density function
    callable: density_fn((x, y)) -> float crowd penalty.
    """

    def __init__(self, layout, density_fn=None) -> None:
        self.layout = layout
        self.graph = self._build_graph(layout)
        self.density_fn = density_fn  # callable(pos_tuple) -> float crowd penalty

    def _build_graph(self, layout) -> nx.Graph:
        G = nx.Graph()
        h, w = layout.height, layout.width
        for y in range(h):
            for x in range(w):
                if layout.is_walkable((x, y)):
                    G.add_node((x, y))
    # 4-neighbor connectivity (Manhattan)
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
        if self.density_fn is None:
            return base
        # Penalize edges leading into high-density nodes
        penalty = self.density_fn(v)
        return base + 0.5 * penalty

    def find_optimal_path(self, start: Tuple[int, int], goals: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
        best = None
        best_len = float("inf")
        for goal in goals:
            if goal not in self.graph or start not in self.graph:
                continue
            try:
                path = nx.astar_path(
                    self.graph,
                    start,
                    goal,
                    heuristic=lambda a, b: abs(a[0] - b[0]) + abs(a[1] - b[1]),
                    weight=lambda u, v, attr: self._weight(u, v, attr),
                )
                # Compute dynamic path length manually to support callable weights
                length = 0.0
                for u, v in zip(path[:-1], path[1:]):
                    edge_attr = self.graph[u][v]
                    length += self._weight(u, v, edge_attr)
                if length < best_len:
                    best = path
                    best_len = length
            except nx.NetworkXNoPath:
                continue
        return best
