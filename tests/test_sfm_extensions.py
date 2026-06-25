from __future__ import annotations

import numpy as np
import pytest

from chiyoda.navigation.social_force import (
    load_social_force_calibration,
    social_force_step,
)
from chiyoda.scenarios.manager import ScenarioManager


def _lane_positions() -> tuple[np.ndarray, np.ndarray]:
    xs = np.array([1.0, 1.8, 2.6, 3.4, 8.0, 8.8, 9.6, 10.4])
    directions = np.array([1, 1, 1, 1, -1, -1, -1, -1], dtype=float)
    return np.column_stack([xs, np.full_like(xs, 1.5)]), directions


def _simulate_counterflow_lane(calibration, *, steps: int = 15) -> float:
    positions, directions = _lane_positions()
    velocities = np.column_stack([directions * 0.9, np.zeros_like(directions)])
    for _ in range(steps):
        displacements = []
        for idx in range(len(positions)):
            mask = np.arange(len(positions)) != idx
            desired = np.array([directions[idx] * 1.1, 0.0])
            displacements.append(
                social_force_step(
                    current_pos=positions[idx],
                    desired_velocity=desired,
                    current_velocity=velocities[idx],
                    neighbors=positions[mask],
                    neighbor_velocities=velocities[mask],
                    walls=[],
                    dt=0.1,
                    counter_flow=True,
                    parameters=calibration,
                )
            )
        displacements = np.vstack(displacements)
        positions = positions + displacements
        positions[:, 1] = np.clip(positions[:, 1], 0.35, 2.65)
        velocities = displacements / 0.1
    return float(positions[:4, 1].mean() - positions[4:, 1].mean())


def test_counterflow_avoidance_forms_lanes_in_corridor():
    disabled = load_social_force_calibration(
        {
            "profile": "generic_legacy",
            "parameters": {
                "agent_interaction_radius_m": 4.5,
                "agent_repulsion_strength": 0.2,
                "counter_flow_friction": 0.0,
                "counter_flow_avoidance_strength": 0.0,
                "wall_repulsion_strength": 0.0,
            },
        }
    )
    enabled = disabled.with_overrides(
        {
            "counter_flow_avoidance_strength": 1.2,
            "counter_flow_avoidance_range_m": 3.0,
            "visual_range_m": 4.5,
            "visual_field_degrees": 200.0,
            "rear_repulsion_weight": 0.1,
        }
    )

    assert abs(_simulate_counterflow_lane(disabled)) < 0.05
    assert _simulate_counterflow_lane(enabled) > 0.5


def test_limited_visual_range_downweights_rear_neighbors():
    calibration = load_social_force_calibration(
        {
            "profile": "generic_legacy",
            "parameters": {
                "agent_repulsion_strength": 2.0,
                "agent_repulsion_range_m": 0.6,
                "counter_flow_avoidance_strength": 0.0,
                "visual_range_m": 2.0,
                "visual_field_degrees": 180.0,
                "rear_repulsion_weight": 0.1,
            },
        }
    )
    current = np.array([0.0, 0.0])
    desired = np.array([1.0, 0.0])
    baseline = social_force_step(
        current, desired, desired, np.zeros((0, 2)), dt=0.1, parameters=calibration
    )
    ahead = social_force_step(
        current,
        desired,
        desired,
        np.array([[1.0, 0.0]]),
        dt=0.1,
        parameters=calibration,
    )
    behind = social_force_step(
        current,
        desired,
        desired,
        np.array([[-1.0, 0.0]]),
        dt=0.1,
        parameters=calibration,
    )
    outside = social_force_step(
        current,
        desired,
        desired,
        np.array([[3.0, 0.0]]),
        dt=0.1,
        parameters=calibration,
    )

    assert np.linalg.norm(behind - baseline) < np.linalg.norm(ahead - baseline) * 0.2
    np.testing.assert_allclose(outside, baseline)


def test_visual_range_parameters_are_overridable_per_scenario():
    scenario = {
        "name": "sfm_visual_range_override",
        "social_force_calibration": {
            "profile": "generic_legacy",
            "parameters": {
                "visual_range_m": 3.25,
                "counter_flow_avoidance_strength": 1.1,
            },
        },
        "layout": {
            "floors": [{"id": "0", "z": 0.0, "text": "XXXXXX\nX@..EX\nXXXXXX\n"}]
        },
        "population": {"total": 1},
        "simulation": {"max_steps": 1, "random_seed": 7},
    }

    sim = ScenarioManager().build_simulation(scenario)

    assert sim.social_force_parameters.visual_range_m == pytest.approx(3.25)
    assert sim.social_force_parameters.counter_flow_avoidance_strength == pytest.approx(
        1.1
    )
