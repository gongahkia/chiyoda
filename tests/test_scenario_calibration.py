from __future__ import annotations

import pytest

from chiyoda.scenarios.manager import ScenarioManager


def test_behavior_and_cohort_calibration_are_loaded_from_scenario():
    scenario = {
        "name": "calibration_smoke",
        "layout": {"text": ("XXXXXX\n" "X@..EX\n" "XXXXXX\n")},
        "population": {
            "total": 1,
            "cohorts": [
                {
                    "name": "calibrated",
                    "count": 1,
                    "calmness": 0.4,
                    "base_speed": 1.05,
                    "base_rationality": 0.62,
                    "credibility": 0.73,
                    "gossip_radius": 3.4,
                    "base_vision_radius": 6.8,
                }
            ],
        },
        "behavior": {
            "density_panic_weight": 0.31,
            "neighbor_panic_weight": 0.07,
            "hazard_panic_weight": 0.23,
            "entropy_anxiety_weight": 0.41,
            "freeze_probability": 0.09,
            "calm_recovery_rate": 0.013,
            "helping_threshold": 0.55,
        },
        "simulation": {"max_steps": 1, "random_seed": 7},
    }

    sim = ScenarioManager().build_simulation(scenario)
    agent = sim.agents[0]
    cfg = sim.behavior_model.config

    assert agent.base_speed == pytest.approx(1.05)
    assert agent.base_rationality == pytest.approx(0.62)
    assert agent.rationality == pytest.approx(0.62)
    assert agent.credibility == pytest.approx(0.73)
    assert agent.gossip_radius == pytest.approx(3.4)
    assert agent.base_vision_radius == pytest.approx(6.8)
    assert cfg.density_panic_weight == pytest.approx(0.31)
    assert cfg.neighbor_panic_weight == pytest.approx(0.07)
    assert cfg.hazard_panic_weight == pytest.approx(0.23)
    assert cfg.entropy_anxiety_weight == pytest.approx(0.41)
    assert cfg.freeze_probability == pytest.approx(0.09)
    assert cfg.calm_recovery_rate == pytest.approx(0.013)
    assert cfg.helping_threshold == pytest.approx(0.55)


def test_base_speed_multiplier_still_scales_default_speed():
    scenario = {
        "name": "speed_multiplier",
        "layout": {"text": ("XXXXXX\n" "X@..EX\n" "XXXXXX\n")},
        "population": {
            "total": 1,
            "cohorts": [
                {
                    "name": "scaled",
                    "count": 1,
                    "base_speed_multiplier": 0.5,
                }
            ],
        },
    }

    sim = ScenarioManager().build_simulation(scenario)

    assert sim.agents[0].base_speed == pytest.approx(0.67)
