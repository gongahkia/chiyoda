from __future__ import annotations

import numpy as np


def adjusted_step(current_pos: np.ndarray, desired_step: np.ndarray, neighbors: np.ndarray, walls: list, dt: float) -> np.ndarray:
    """
    Very lightweight social-force inspired adjustment to avoid crowding.
    - Repel from neighbors within 1m
    - Damp movement into wall cells (walls detected externally)
    """
    repulsion = np.zeros(2)
    for n in neighbors:
        delta = current_pos - n
        dist = np.linalg.norm(delta) + 1e-6
        if dist < 1.0:
            repulsion += (delta / dist) * (1.0 - dist)

    adjusted = desired_step + 0.5 * repulsion
    max_step = max(float(np.linalg.norm(desired_step)), 1.0 * dt)
    nrm = np.linalg.norm(adjusted)
    if nrm > max_step:
        adjusted = adjusted / nrm * max_step
    return adjusted
