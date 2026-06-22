from __future__ import annotations

import numpy as np
import pytest

from chiyoda.acceleration.backends import (
    JuliaAccelerationBackend,
    PythonAccelerationBackend,
)
from chiyoda.navigation.social_force import social_force_step


def test_python_social_force_batch_matches_scalar_kernel():
    inputs = _corridor_inputs()
    backend = PythonAccelerationBackend()

    batch = backend.social_force_steps(**inputs)
    scalar = np.vstack(
        [
            social_force_step(
                current_pos=inputs["current_positions"][idx],
                desired_velocity=inputs["desired_velocities"][idx],
                current_velocity=inputs["current_velocities"][idx],
                neighbors=inputs["neighbor_positions"][
                    idx, : inputs["neighbor_counts"][idx], :
                ],
                neighbor_velocities=inputs["neighbor_velocities"][
                    idx, : inputs["neighbor_counts"][idx], :
                ],
                walls=inputs["walls"].tolist(),
                dt=inputs["dt"],
                counter_flow=inputs["counter_flow"],
            )
            for idx in range(inputs["current_positions"].shape[0])
        ]
    )

    np.testing.assert_allclose(batch, scalar, atol=0.0, rtol=0.0)


def test_julia_social_force_kernel_matches_python_corridor():
    pytest.importorskip("juliacall")
    inputs = _corridor_inputs()

    python = PythonAccelerationBackend().social_force_steps(**inputs)
    julia = JuliaAccelerationBackend().social_force_steps(**inputs)

    np.testing.assert_allclose(julia, python, atol=1e-6, rtol=1e-6)


def _corridor_inputs() -> dict:
    count = 100
    positions = np.column_stack(
        (
            np.linspace(0.0, 49.5, count),
            1.0 + (np.arange(count) % 3) * 0.35,
        )
    )
    desired = np.tile(np.array([1.2, 0.0]), (count, 1))
    current = desired * 0.8
    max_neighbors = 4
    neighbors = np.zeros((count, max_neighbors, 2), dtype=float)
    neighbor_velocities = np.zeros_like(neighbors)
    neighbor_counts = np.zeros(count, dtype=int)

    for idx in range(count):
        candidate_indices = [
            other
            for other in (idx - 2, idx - 1, idx + 1, idx + 2)
            if 0 <= other < count
        ]
        neighbor_counts[idx] = len(candidate_indices)
        for slot, other in enumerate(candidate_indices):
            neighbors[idx, slot] = positions[other]
            direction = -1.0 if other % 5 == 0 else 1.0
            neighbor_velocities[idx, slot] = np.array([direction, 0.0])

    wall_x = np.linspace(0.0, 50.0, 12)
    walls = np.vstack(
        (
            np.column_stack((wall_x, np.zeros_like(wall_x))),
            np.column_stack((wall_x, np.full_like(wall_x, 3.0))),
        )
    )
    return {
        "current_positions": positions,
        "desired_velocities": desired,
        "current_velocities": current,
        "neighbor_positions": neighbors,
        "neighbor_counts": neighbor_counts,
        "neighbor_velocities": neighbor_velocities,
        "walls": walls,
        "dt": 0.1,
        "counter_flow": True,
    }
