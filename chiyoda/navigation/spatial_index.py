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

    def density_penalty_fn(self):
        def _pen(pos_tuple):
            if self.tree is None:
                return 0.0
            pos = np.array([pos_tuple[0] + 0.5, pos_tuple[1] + 0.5])
            k = len(self.find_neighbors(pos, radius=1.0))
            return max(0, k - 1) / 5.0

        return _pen
