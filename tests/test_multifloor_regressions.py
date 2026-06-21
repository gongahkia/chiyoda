from __future__ import annotations

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import validate_scenario_file


def test_multifloor_connector_regression_assertions_pass():
    manager = ScenarioManager()
    scenario = manager.load_config("scenarios/validation_multifloor_connectors.yaml")
    validation = validate_scenario_file("scenarios/validation_multifloor_connectors.yaml")

    sim = manager.build_simulation(scenario)
    sim.run()
    result = evaluate_scenario_assertions(scenario, sim)

    assert not validation.has_errors
    assert result.ok
    assert sim.connector_usage_cumulative["stairs_main"] >= 1
    assert sim.connector_usage_cumulative["ramp_main"] >= 1
    assert sim.connector_usage_cumulative["escalator_main"] >= 1
    assert sim.connector_usage_cumulative["elevator_main"] >= 1
    assert sim.step_history[-1].connector_capacity["elevator_main"] == 1
    assert "stairs_main" in sim.step_history[-1].connector_flow
    assert not sim.impossible_floor_jumps


def test_elevator_queue_regression_assertions_pass():
    manager = ScenarioManager()
    scenario = manager.load_config("scenarios/validation_elevator_queue.yaml")
    sim = manager.build_simulation(scenario)
    sim.run()
    result = evaluate_scenario_assertions(scenario, sim)

    assert result.ok
    assert sim.connector_usage_cumulative["elevator_queue"] >= 3
    assert len([event for event in sim.connector_events if event["phase"] == "start"]) >= 3
    assert sim.step_history[-1].connector_capacity["elevator_queue"] == 1
    assert sim.step_history[-1].connector_queue_length["elevator_queue"] == 0
    assert not sim.impossible_floor_jumps


def test_assert_scenario_cli_fails_on_assertion_failure(tmp_path):
    scenario = tmp_path / "bad.yaml"
    scenario.write_text(
        """
scenario:
  name: bad_assertion
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |
          XXXXX
          X@.EX
          XXXXX
  population:
    total: 1
  simulation:
    max_steps: 1
  assertions:
    evacuated: {eq: 99}
"""
    )

    result = CliRunner().invoke(cli, ["assert-scenario", str(scenario), "--json"])

    assert result.exit_code == 1
    assert "assertion_failed:evacuated" in result.output
