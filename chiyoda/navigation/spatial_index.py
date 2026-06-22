from __future__ import annotations

import numpy as np
from scipy.spatial import cKDTree

try:
    from numba import njit

    NUMBA_AVAILABLE = True
except Exception:  # pragma: no cover - depends on optional dependency
    NUMBA_AVAILABLE = False
    njit = None


if NUMBA_AVAILABLE:

    @njit(cache=True)
    def _nonself_mask(points: np.ndarray, pos: np.ndarray, eps: float) -> np.ndarray:
        mask = np.zeros(points.shape[0], dtype=np.bool_)
        for idx in range(points.shape[0]):
            total = 0.0
            for axis in range(points.shape[1]):
                delta = points[idx, axis] - pos[axis]
                total += delta * delta
            mask[idx] = np.sqrt(total) > eps
        return mask

else:

    def _nonself_mask(points: np.ndarray, pos: np.ndarray, eps: float) -> np.ndarray:
        return np.linalg.norm(points - pos, axis=1) > eps


class SpatialIndex:
    """KD-tree backed spatial index for fast neighbor queries."""

    def __init__(self) -> None:
        self.tree: cKDTree | None = None
        self._positions: np.ndarray | None = None
        self._agents: list[object] = []
        self._density_penalty_cache: dict[tuple, float] = {}

    def update(self, agents: list[object]) -> None:
        self._agents = [a for a in agents if not getattr(a, "has_evacuated", False)]
        self._density_penalty_cache.clear()
        coords = [a.pos for a in self._agents]
        self._positions = (
            np.array(coords, dtype=float) if coords else np.zeros((0, 3), dtype=float)
        )
        if len(self._positions) > 0:
            self.tree = cKDTree(self._positions)
        else:
            self.tree = None

    def find_neighbors(self, pos: np.ndarray, radius: float) -> list[int]:
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
            return np.zeros((0, 3), dtype=float)
        idxs = self.find_neighbors(pos, radius=radius)
        if not idxs:
            return np.zeros((0, 3), dtype=float)
        points = self._positions[idxs]
        mask = _nonself_mask(points, pos, 1e-6)
        return points[mask]

    def neighbor_agents(self, pos: np.ndarray, radius: float = 1.0) -> list[object]:
        if self.tree is None or self._positions is None or len(self._positions) == 0:
            return []
        idxs = self.find_neighbors(pos, radius=radius)
        neighbors = [self._agents[idx] for idx in idxs]
        positions = self._positions[idxs]
        mask = _nonself_mask(positions, pos, 1e-6)
        return [
            agent for agent, keep in zip(neighbors, mask, strict=False) if bool(keep)
        ]

    def density_penalty_fn(self):
        def _pen(pos_tuple):
            if self.tree is None:
                return 0.0
            if len(pos_tuple) >= 3 and isinstance(pos_tuple[0], str):
                cell = (str(pos_tuple[0]), int(pos_tuple[1]), int(pos_tuple[2]))
            elif len(pos_tuple) >= 3:
                cell = ("", int(pos_tuple[0]), int(pos_tuple[1]), float(pos_tuple[2]))
            else:
                cell = ("", int(pos_tuple[0]), int(pos_tuple[1]), 0.0)
            cached = self._density_penalty_cache.get(cell)
            if cached is not None:
                return cached
            if len(pos_tuple) >= 3 and isinstance(pos_tuple[0], str):
                pos = np.array([cell[1] + 0.5, cell[2] + 0.5, 0.0], dtype=float)
            elif len(pos_tuple) >= 3:
                pos = np.array(
                    [cell[1] + 0.5, cell[2] + 0.5, float(pos_tuple[2])], dtype=float
                )
            else:
                pos = np.array([cell[1] + 0.5, cell[2] + 0.5, 0.0], dtype=float)
            value = min(2.0, self.local_density(pos, radius=1.0) * 2.0)
            self._density_penalty_cache[cell] = value
            return value

        return _pen
