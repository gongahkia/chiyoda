from __future__ import annotations

import numpy as np
import pytest

from chiyoda.environment.hazards import Hazard
from chiyoda.environment.layout import Layout
from chiyoda.scenarios.manager import ScenarioManager


def test_wildfire_hazard_spreads_embers_downwind_with_layout_origin():
    layout = Layout.from_floors(
        [
            {
                "id": "0",
                "z": 0.0,
                "text": "XXXX\nX..X\nX..X\nXXXX\n",
            }
        ],
        origin=(10.0, 20.0),
        cell_size=1.0,
    )
    sim = type("DummySimulation", (), {"layout": layout})()
    hazard = Hazard(
        pos=(11.5, 21.5, 0.0),
        kind="WILDFIRE",
        radius=0.5,
        severity=0.9,
        spread_rate=0.4,
        wind_vector=(3.0, 0.0),
        ember_spotting_rate=0.8,
        ember_ignition_radius=3.0,
        ember_decay_rate=0.05,
    )

    hazard.step(1.0, sim)

    assert hazard.radius > 0.5
    assert hazard.ember_field
    assert hazard.snapshot()["ember_cell_count"] == len(hazard.ember_field)
    assert hazard.intensity_at(np.array([15.0, 21.5, 0.0])) > 0.0


def test_wui_egress_switches_pedestrians_to_vehicle_mode():
    scenario = {
        "name": "wui_mode_switch_fixture",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXX\nX@.EX\nXXXXX\n",
                }
            ]
        },
        "population": {
            "total": 1,
            "cohorts": [
                {
                    "name": "resident",
                    "count": 1,
                    "spawn_cells": [{"floor": "0", "x": 1, "y": 1}],
                }
            ],
        },
        "wui_egress": {
            "road_segments": [
                {
                    "id": "collector",
                    "mode_switch": "vehicle",
                    "speed_multiplier": 2.5,
                    "cells": [{"floor": "0", "x": 1, "y": 1}],
                }
            ]
        },
        "simulation": {"max_steps": 1, "dt": 0.1, "random_seed": 3},
    }
    sim = ScenarioManager().build_simulation(scenario)
    agent = sim.agents[0]
    base_speed = agent.base_speed

    sim.step()

    assert agent.evacuation_mode == "vehicle"
    assert agent.mode_switch_step == 0
    assert agent.base_speed == pytest.approx(base_speed * 2.5)
    assert sim.mode_switch_events[0]["segment_id"] == "collector"


def test_wildfire_wui_benchmark_runs_broadcast_and_mode_switch():
    sim = ScenarioManager().load_scenario("scenarios/benchmark/wildfire_wui.yaml")

    sim.run()

    assert any(str(h.kind).upper() == "WILDFIRE" for h in sim.hazards)
    assert sim.hazards[0].ember_field
    assert sim.mode_switch_events
    assert sim.intervention_events
    assert sim.intervention_events[0].policy == "wildfire_long_range_broadcast"
    assert sim.intervention_events[0].message_type == "wildfire_warning"
