from __future__ import annotations

import json

import numpy as np
import pytest

from chiyoda.agents.commuter import Commuter
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import ImportedHazardField
from chiyoda.environment.layout import Layout
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.scenarios.manager import ScenarioManager


def _hazard_field_payload() -> dict[str, object]:
    return {
        "kind": "SMOKE",
        "origin": [0.0, 0.0],
        "cell_size": 1.0,
        "intensity": [
            [0.0, 0.0, 0.0],
            [0.0, 0.8, 0.0],
            [0.0, 0.0, 0.0],
        ],
        "visibility": [
            [1.0, 1.0, 1.0],
            [1.0, 0.35, 1.0],
            [1.0, 1.0, 1.0],
        ],
        "source": {
            "name": "unit-test precomputed smoke grid",
            "license": "test fixture",
        },
    }


def test_imported_hazard_field_affects_exposure_visibility_and_route_penalty(tmp_path):
    field_path = tmp_path / "smoke_field.json"
    field_path.write_text(json.dumps(_hazard_field_payload()))

    hazard = ImportedHazardField.from_file(field_path)
    layout = Layout.from_text(
        "XXXXX\n"
        "X@.EX\n"
        "XXXXX\n"
    )
    agent = Commuter(id=0, pos=np.array([1.5, 1.5], dtype=float))
    sim = Simulation(
        layout=layout,
        agents=[agent],
        exits=[Exit(pos=(3, 1))],
        hazards=[hazard],
        config=SimulationConfig(dt=1.0, max_steps=1, random_seed=7),
    )

    sim._ensure_bootstrapped()

    assert sim.hazard_intensity_at((1.5, 1.5)) == pytest.approx(0.8)
    assert sim.visibility_at((1.5, 1.5)) == pytest.approx(0.35)
    assert agent.current_hazard_load == pytest.approx(0.8)
    assert agent.hazard_exposure == pytest.approx(0.8)
    assert sim.hazard_penalty_at_cell((1, 1)) == pytest.approx(1.0)
    assert sim.hazard_penalty_at_cell((0, 0)) == pytest.approx(0.0)


def test_scenario_manager_loads_relative_imported_hazard_field(tmp_path):
    field_path = tmp_path / "gas_field.json"
    field_path.write_text(json.dumps(_hazard_field_payload() | {"kind": "GAS"}))
    scenario_path = tmp_path / "scenario.yaml"
    scenario_path.write_text(
        """
name: imported_hazard_fixture
layout:
  grid:
    - "XXXXX"
    - "X@.EX"
    - "XXXXX"
population:
  total: 1
  cohorts:
    - name: test
      count: 1
      spawn_cells: [[1, 1]]
hazards:
  - type: GAS
    field:
      file: gas_field.json
simulation:
  max_steps: 1
  dt: 1.0
"""
    )

    sim = ScenarioManager().load_scenario(str(scenario_path))

    assert len(sim.hazards) == 1
    assert isinstance(sim.hazards[0], ImportedHazardField)
    assert sim.hazard_intensity_at((1.5, 1.5)) == pytest.approx(0.8)


def test_imported_hazard_field_can_drive_ground_truth_route_cost(tmp_path):
    field_path = tmp_path / "smoke_field.json"
    payload = _hazard_field_payload()
    payload["intensity"] = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 3.0, 3.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ]
    payload["visibility"] = [
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 0.5, 0.5, 1.0],
        [1.0, 1.0, 1.0, 1.0],
    ]
    field_path.write_text(json.dumps(payload))

    layout = Layout.from_text(
        "XXXXXX\n"
        "X@..EX\n"
        "X....X\n"
        "XXXXXX\n"
    )
    sim = Simulation(
        layout=layout,
        agents=[],
        exits=[Exit(pos=(4, 1))],
        hazards=[ImportedHazardField.from_file(field_path)],
        config=SimulationConfig(hazard_avoidance_weight=10.0),
    )
    navigator = SmartNavigator(layout, hazard_fn=sim.hazard_penalty_at_cell)

    path = navigator.find_optimal_path((1, 1), [(4, 1)])

    assert path is not None
    assert (1, 2) in path
    assert (2, 1) not in path
