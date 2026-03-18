from __future__ import annotations

from typing import List
import numpy as np
from scipy.spatial import cKDTree


class SpatialIndex:
    """KD-tree backed spatial index for fast neighbor queries."""

    def __init__(self) -> None:
        self.tree: cKDTree | None = None
        self._positions: np.ndarray | None = None

    def update(self, agents: List[object]) -> None:
        coords = [a.pos for a in agents if not getattr(a, "has_evacuated", False)]
        self._positions = np.array(coords) if coords else np.zeros((0, 2), dtype=float)
        if len(self._positions) > 0:
            self.tree = cKDTree(self._positions)
        else:
            self.tree = None

    def find_neighbors(self, pos: np.ndarray, radius: float) -> List[int]:
        if self.tree is None or self._positions is None or len(self._positions) == 0:
            return []
        idxs = self.tree.query_ball_point(pos, r=radius)
        return list(idxs)

    def local_density(self, pos: np.ndarray, radius: float = 1.5) -> float:
        if self.tree is None or self._positions is None or len(self._positions) == 0:
            return 0.0
        count = len(self.find_neighbors(pos, radius=radius)) - 1
        area = float(np.pi * radius * radius)
        return max(0, count) / area

    def neighbor_positions(self, pos: np.ndarray, radius: float = 1.0) -> np.ndarray:
        if self.tree is None or self._positions is None or len(self._positions) == 0:
            return np.zeros((0, 2), dtype=float)
        idxs = self.find_neighbors(pos, radius=radius)
        if not idxs:
            return np.zeros((0, 2), dtype=float)
        points = self._positions[idxs]
        mask = np.linalg.norm(points - pos, axis=1) > 1e-6
        return points[mask]

    def density_penalty_fn(self):
        def _pen(pos_tuple):
            if self.tree is None:
                return 0.0
            pos = np.array([pos_tuple[0] + 0.5, pos_tuple[1] + 0.5])
            return min(2.0, self.local_density(pos, radius=1.0) * 2.0)

        return _pen
