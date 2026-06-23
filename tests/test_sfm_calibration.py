from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest

from chiyoda.navigation.social_force import (
    load_social_force_calibration,
    social_force_step,
)
from chiyoda.scenarios.manager import ScenarioManager


SENSITIVITY_PATH = Path("data/sfm_calibrations/sensitivity_baseline.json")


def _scenario() -> dict:
    return {
        "name": "sfm_profile",
        "social_force_calibration": {
            "profile": "yolov5_mdpi_2024",
            "parameters": {"base_vision_radius_m": 4.0},
        },
        "layout": {"floors": [{"id": "0", "z": 0.0, "text": "XXXXXX\nX@..EX\nXXXXXX\n"}]},
        "population": {"total": 1},
        "simulation": {"max_steps": 1, "random_seed": 7},
    }


def _sensitivity_payload() -> dict:
    return json.loads(SENSITIVITY_PATH.read_text())


def _sensitivity_step(calibration, payload: dict) -> np.ndarray:
    state = payload["baseline"]
    heading = np.array(state["desired_heading"], dtype=float)
    heading = heading / np.linalg.norm(heading)
    desired_velocity = heading * calibration.desired_speed_mps
    desired_velocity[1] += float(state["desired_lateral_velocity_mps"])
    return social_force_step(
        current_pos=np.array(state["current_pos"], dtype=float),
        desired_velocity=desired_velocity,
        current_velocity=np.array(state["current_velocity"], dtype=float),
        neighbors=np.array(state["neighbors"], dtype=float),
        neighbor_velocities=np.array(state["neighbor_velocities"], dtype=float),
        walls=state["walls"],
        dt=float(state["dt"]),
        counter_flow=bool(state["counter_flow"]),
        parameters=calibration,
    )


def test_social_force_profiles_load_from_yaml_with_provenance():
    generic = load_social_force_calibration("generic_legacy")
    yolov5 = load_social_force_calibration("yolov5_mdpi_2024")
    override = load_social_force_calibration(
        {
            "profile": "yolov5_mdpi_2024",
            "parameters": {"base_vision_radius_m": 4.25},
        }
    )

    assert generic.profile == "generic_legacy"
    assert generic.desired_speed_mps == pytest.approx(1.34)
    assert yolov5.profile == "yolov5_mdpi_2024"
    assert yolov5.desired_speed_mps == pytest.approx(1.37)
    assert yolov5.relaxation_time_s == pytest.approx(0.53)
    assert yolov5.agent_repulsion_strength == pytest.approx(10.25)
    assert yolov5.agent_repulsion_range_m == pytest.approx(0.28)
    assert "10.3390/s24155011" in str(
        yolov5.provenance_for("agent_repulsion_strength")
    )
    assert override.base_vision_radius_m == pytest.approx(4.25)


def test_social_force_calibration_changes_kernel_displacement():
    payload = _sensitivity_payload()
    generic = load_social_force_calibration("generic_legacy")
    yolov5 = load_social_force_calibration("yolov5_mdpi_2024")

    generic_step = _sensitivity_step(generic, payload)
    yolov5_step = _sensitivity_step(yolov5, payload)

    assert np.linalg.norm(yolov5_step - generic_step) > 0.05


def test_scenario_yaml_selects_yolov5_defaults():
    sim = ScenarioManager().build_simulation(_scenario())
    agent = sim.agents[0]

    assert sim.social_force_calibration_profile == "yolov5_mdpi_2024"
    assert sim.social_force_parameters.desired_speed_mps == pytest.approx(1.37)
    assert sim.social_force_parameters.relaxation_time_s == pytest.approx(0.53)
    assert agent.base_speed == pytest.approx(1.37)
    assert agent.base_vision_radius == pytest.approx(4.0)
    assert "10.3390/s24155011" in str(
        sim.social_force_parameters.provenance_for("desired_speed_mps")
    )


def test_sfm_sensitivity_baseline_documents_each_parameter_delta():
    payload = _sensitivity_payload()
    generic = load_social_force_calibration("generic_legacy")
    yolov5 = load_social_force_calibration("yolov5_mdpi_2024")
    generic_params = generic.to_parameters()
    yolov5_params = yolov5.to_parameters()

    assert payload["generic_profile"] == "generic_legacy"
    assert payload["comparison_profile"] == "yolov5_mdpi_2024"
    assert set(payload["parameters"]) == set(generic_params)

    generic_step = _sensitivity_step(generic, payload)
    yolov5_step = _sensitivity_step(yolov5, payload)
    assert payload["generic_displacement"] == pytest.approx(generic_step.tolist())
    assert payload["comparison_displacement"] == pytest.approx(yolov5_step.tolist())
    assert payload["comparison_delta_m"] == pytest.approx(
        float(np.linalg.norm(yolov5_step - generic_step))
    )

    for key, entry in payload["parameters"].items():
        assert entry["generic_value"] == pytest.approx(generic_params[key])
        assert entry["profile_value"] == pytest.approx(yolov5_params[key])
        one = generic.with_overrides({key: yolov5_params[key]})
        delta = float(np.linalg.norm(_sensitivity_step(one, payload) - generic_step))
        assert entry["single_parameter_delta_m"] == pytest.approx(delta)

    for key in (
        "desired_speed_mps",
        "relaxation_time_s",
        "agent_repulsion_strength",
        "agent_repulsion_range_m",
    ):
        entry = payload["parameters"][key]
        assert entry["single_parameter_delta_m"] > 0
        assert "10.3390/s24155011" in entry["provenance"]
