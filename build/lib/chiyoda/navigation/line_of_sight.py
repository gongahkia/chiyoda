"""Line-of-sight checks over strict 3D layouts."""

from __future__ import annotations

from collections.abc import Sequence

import numpy as np


def line_of_sight(
    layout,
    source: Sequence[float],
    target: Sequence[float],
    *,
    max_range: float | None = None,
) -> bool:
    src = _point3(source)
    dst = _point3(target)
    distance = float(np.linalg.norm(dst - src))
    if max_range is not None and distance > max_range:
        return False
    if distance <= 1e-9:
        return True
    if layout.floor_for_z(float(src[2])) != layout.floor_for_z(float(dst[2])):
        return False
    steps = max(2, int(np.ceil(distance * 2.0)))
    for idx in range(1, steps):
        point = src + (dst - src) * (idx / steps)
        if not layout.is_walkable(point):
            return False
    return True


def _point3(value: Sequence[float]) -> np.ndarray:
    if len(value) >= 3:
        return np.array(
            [float(value[0]), float(value[1]), float(value[2])], dtype=float
        )
    return np.array([float(value[0]), float(value[1]), 0.0], dtype=float)
