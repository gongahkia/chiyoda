from __future__ import annotations

from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager


def _scenario(assertions: dict):
    return {
        "name": "behavioral_plausibility",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXXX\nX@...EX\nXXXXXXX",
                }
            ]
        },
        "population": {"total": 2},
        "simulation": {"max_steps": 12, "dt": 0.5, "random_seed": 7},
        "assertions": assertions,
    }


def _hazard_scenario(assertions: dict):
    scenario = _scenario(assertions)
    scenario["hazards"] = [
        {
            "type": "GAS",
            "location": [2.0, 1.0, 0.0],
            "radius": 1.5,
            "severity": 0.4,
        }
    ]
    return scenario


def _hostile_scenario(assertions: dict):
    scenario = _scenario(assertions)
    scenario["information"] = {
        "mode": "asymmetric",
        "observation_radius": 10.0,
        "gossip_radius": 0.0,
    }
    scenario["simulation"] = {"max_steps": 3, "random_seed": 7}
    scenario["population"] = {
        "total": 1,
        "cohorts": [{"name": "baseline", "count": 1, "familiarity": 0.0}],
    }
    scenario["hostile_channels"] = [
        {
            "id": "false_protective_action",
            "channel_type": "gossip",
            "objective": "false-protective-action",
            "budget": 1,
            "plausibility": 0.8,
            "claimed_exit": {"floor": "0", "x": 2, "y": 1},
        }
    ]
    return scenario


def test_behavioral_plausibility_assertions_pass_for_broad_bounds():
    manager = ScenarioManager()
    scenario = _scenario(
        {
            "behavioral_plausibility": {
                "peak_cell_occupancy": {"min": 1, "max": 999},
                "evacuation_completion_fraction": {"min": 0.0, "max": 1.0},
                "max_agent_speed_mps": {"max": 5.0},
            }
        }
    )
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert result.ok


def test_behavioral_plausibility_assertions_fail_on_impossible_bound():
    manager = ScenarioManager()
    scenario = _scenario(
        {"behavioral_plausibility": {"peak_cell_occupancy": {"max": 0}}}
    )
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert not result.ok
    assert any(
        issue.code == "assertion_failed:behavioral_plausibility.peak_cell_occupancy"
        for issue in result.issues
    )


def test_hazard_plausibility_assertions_pass_for_broad_bounds():
    manager = ScenarioManager()
    scenario = _hazard_scenario(
        {
            "hazard_plausibility": {
                "hazard_count": {"eq": 1},
                "stylized_hazard_count": {"eq": 1},
                "max_hazard_severity": {"max": 1.0},
                "max_agent_hazard_exposure": {"max": 100.0},
            }
        }
    )
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert result.ok


def test_hazard_plausibility_assertions_fail_on_missing_imported_field():
    manager = ScenarioManager()
    scenario = _hazard_scenario(
        {"hazard_plausibility": {"imported_hazard_count": {"min": 1}}}
    )
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert not result.ok
    assert any(
        issue.code == "assertion_failed:hazard_plausibility.imported_hazard_count"
        for issue in result.issues
    )


def test_vertical_transport_assertions_cover_elevator_queue_metrics():
    manager = ScenarioManager()
    scenario = manager.load_config("scenarios/validation_elevator_queue.yaml")
    scenario.setdefault("assertions", {})
    scenario["assertions"]["vertical_transport"] = {
        "elevator_count": {"eq": 1},
        "elevator_usage": {"min": 3},
        "max_elevator_queue_length": {"max": 3},
    }
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert result.ok


def test_vertical_transport_assertions_fail_on_wrong_connector_count():
    manager = ScenarioManager()
    scenario = manager.load_config("scenarios/validation_elevator_queue.yaml")
    scenario.setdefault("assertions", {})
    scenario["assertions"]["vertical_transport"] = {"elevator_count": {"eq": 0}}
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert not result.ok
    assert any(
        issue.code == "assertion_failed:vertical_transport.elevator_count"
        for issue in result.issues
    )


def test_hostile_llm_assertions_cover_hostile_channel_metrics():
    manager = ScenarioManager()
    scenario = _hostile_scenario(
        {
            "hostile_llm": {
                "hostile_channel_count": {"eq": 1},
                "hostile_channel_event_count": {"eq": 1},
                "hostile_channel_recipients": {"eq": 1},
                "llm_call_count": {"eq": 0},
            }
        }
    )
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert result.ok


def test_hostile_llm_assertions_fail_on_missing_hostile_event():
    manager = ScenarioManager()
    scenario = _hostile_scenario(
        {"hostile_llm": {"hostile_channel_event_count": {"eq": 0}}}
    )
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert not result.ok
    assert any(
        issue.code == "assertion_failed:hostile_llm.hostile_channel_event_count"
        for issue in result.issues
    )
