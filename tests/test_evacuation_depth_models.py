from __future__ import annotations

import pytest

from chiyoda.agents.commuter import Commuter
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard
from chiyoda.environment.layout import Layout
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.scenarios.manager import ScenarioManager


def test_navigator_treats_dynamic_blocked_cells_as_closed():
    layout = Layout.from_text("XXXXXXX\n" "X@...EX\n" "X.....X\n" "XXXXXXX\n")
    blocked = {("0", 3, 1)}
    nav = SmartNavigator(
        layout,
        blocked_fn=lambda cell: tuple(layout.cell(cell)) in blocked,
        strategy="reverse_dijkstra",
    )

    path = nav.find_optimal_path(("0", 1, 1), layout.exit_positions())

    assert path is not None
    assert ("0", 3, 1) not in path
    assert ("0", 3, 2) in path


def test_dynamic_flood_closure_forces_replan_around_blocked_path():
    scenario = {
        "name": "dynamic_closure_replan",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXXX\nX@...EX\nX.....X\nXXXXXXX",
                }
            ]
        },
        "population": {
            "total": 1,
            "cohorts": [
                {
                    "name": "test",
                    "count": 1,
                    "spawn_cells": [{"floor": "0", "x": 1, "y": 1}],
                    "familiarity": 1.0,
                }
            ],
        },
        "hazards": [
            {
                "type": "FLOOD",
                "location": [3.5, 1.5, 0.0],
                "radius": 0.0,
                "severity": 1.0,
                "flood_depth_threshold_m": 0.5,
            }
        ],
        "information": {"mode": "perfect"},
        "simulation": {
            "max_steps": 1,
            "dt": 1.0,
            "dynamic_flood_block_depth_m": 0.5,
        },
    }
    sim = ScenarioManager().build_simulation(scenario)
    agent = sim.agents[0]
    sim._ensure_bootstrapped()
    agent.update_navigation(sim.navigator, sim)
    assert ("0", 3, 1) in agent.current_path

    hazard = sim.hazards[0]
    hazard.inundation_origin = (0.0, 0.0)
    hazard.inundation_cell_size = 1.0
    hazard.inundation_field[(3, 1)] = 0.7
    sim._update_dynamic_topology()
    sim._refresh_agent_context()
    agent.update_navigation(sim.navigator, sim)

    assert ("0", 3, 1) in sim.dynamic_closed_cells
    assert ("0", 3, 1) not in agent.current_path
    assert ("0", 3, 2) in agent.current_path
    assert sim.replan_events[-1]["reason"] in {"blocked_path", "topology_changed"}


def test_hazard_specific_dose_buckets_are_tracked_separately():
    layout = Layout.from_text("XXXXX\n" "X@.EX\n" "XXXXX\n")
    agent = Commuter(id=0, pos=layout.world_position(("0", 1, 1)), floor_id="0")
    sim = Simulation(
        layout=layout,
        agents=[agent],
        exits=[Exit(pos=("0", 3, 1))],
        hazards=[
            Hazard(pos=(1.5, 1.5, 0.0), kind="GAS", radius=1.0, severity=1.0),
            Hazard(pos=(1.5, 1.5, 0.0), kind="FIRE", radius=1.0, severity=0.6),
        ],
        config=SimulationConfig(dt=1.0, max_steps=1, random_seed=3),
    )

    sim._ensure_bootstrapped()

    assert agent.toxic_load == pytest.approx(1.0)
    assert agent.heat_load == pytest.approx(0.6)
    assert agent.smoke_fed == pytest.approx(0.0)
    assert agent.hazard_exposure == pytest.approx(1.6)
    assert agent.physiology.impairment_level > 0.0


def test_bottleneck_door_flow_calibration_limits_speed_in_narrow_zone():
    layout = Layout.from_text("XXXXXX\n" "X@@.EX\n" "XXXXXX\n")
    agents = [
        Commuter(id=0, pos=layout.world_position(("0", 1, 1)), floor_id="0"),
        Commuter(id=1, pos=layout.world_position(("0", 2, 1)), floor_id="0"),
    ]
    sim = Simulation(
        layout=layout,
        agents=agents,
        exits=[Exit(pos=("0", 4, 1))],
        config=SimulationConfig(
            dt=1.0,
            door_flow_enabled=True,
            door_specific_flow_per_m_s=1.3,
            door_effective_width_loss_m=0.3,
            door_min_speed_factor=0.2,
        ),
    )

    sim._ensure_bootstrapped()
    metrics = sim._update_bottleneck_metrics()

    assert agents[0].door_flow_speed_factor < 1.0
    assert agents[1].door_flow_speed_factor < 1.0
    assert any(
        metric.capacity_per_s == pytest.approx(0.91) for metric in metrics.values()
    )
    assert any(metric.flow_speed_factor < 1.0 for metric in metrics.values())
