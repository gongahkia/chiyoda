from __future__ import annotations

import csv
import math
from pathlib import Path

import numpy as np

from chiyoda.environment.hazards import ImportedHazardField
from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager

REFERENCE = Path("data/fds_reference/smoke_detector_reference.csv")
SCENARIO = Path("scenarios/validation_fds_room_corridor.yaml")


def _rmse(errors: list[float]) -> float:
    return math.sqrt(sum(error * error for error in errors) / len(errors))


def test_fds_reference_scalar_profile_import_matches_reference_values():
    field = ImportedHazardField.from_file(REFERENCE, kind="SMOKE")
    gas_errors: list[float] = []
    visibility_errors: list[float] = []

    with REFERENCE.open(newline="") as handle:
        for row in csv.DictReader(handle):
            point = np.array(
                [float(row["x"]) + 0.5, float(row["y"]) + 0.5],
                dtype=float,
            )
            gas_errors.append(
                field.gas_concentration_at(point)
                - float(row["gas_concentration_kg_kg"])
            )
            visibility_errors.append(
                field.visibility_m_at(point) - float(row["visibility_m"])
            )

    assert _rmse(gas_errors) <= 1e-12
    assert max(abs(error) for error in gas_errors) <= 1e-12
    assert _rmse(visibility_errors) <= 1e-9
    assert max(abs(error) for error in visibility_errors) <= 1e-9


def test_fds_reference_scenario_runs_with_imported_scalar_field():
    manager = ScenarioManager()
    scenario = manager.load_config(str(SCENARIO))
    simulation = manager.build_simulation(scenario)

    simulation.run()
    assertions = evaluate_scenario_assertions(scenario, simulation)

    assert isinstance(simulation.hazards[0], ImportedHazardField)
    assert assertions.ok, assertions.issues
