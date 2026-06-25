from __future__ import annotations

from chiyoda.information.llm import (
    GeneratedEvacuationMessage,
    HazardSnapshot,
    validate_generated_message,
    validator_settings,
)
from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import validate_scenario_config


def test_falsification_unreachable_spawn_fails_static_validation():
    scenario = {
        "name": "falsification_unreachable_spawn",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXXX\nX@X..EX\nXXXXXXX",
                }
            ]
        },
        "population": {"total": 1},
        "simulation": {"max_steps": 1, "random_seed": 3},
    }

    result = validate_scenario_config(scenario)

    assert result.has_errors
    assert any(issue.code == "start_unreachable" for issue in result.issues)


def test_falsification_dynamic_closure_fails_evacuation_assertion():
    scenario = {
        "name": "falsification_dynamic_closure",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXXX\nX@..E.X\nXXXXXXX",
                }
            ]
        },
        "population": {"total": 1},
        "hazards": [
            {
                "type": "FIRE",
                "location": [2.0, 1.0, 0.0],
                "radius": 2.0,
                "severity": 1.0,
            }
        ],
        "simulation": {
            "max_steps": 4,
            "dt": 0.5,
            "random_seed": 3,
            "dynamic_fire_block_intensity": 0.5,
        },
        "assertions": {"evacuated": {"min": 1}},
    }
    manager = ScenarioManager()
    simulation = manager.build_simulation(scenario)
    simulation.run()

    result = evaluate_scenario_assertions(scenario, simulation)

    assert not result.ok
    assert any(issue.code == "assertion_failed:evacuated" for issue in result.issues)
    assert simulation.dynamic_topology_events


def test_falsification_strict_llm_validation_rejects_unsafe_guidance():
    message = GeneratedEvacuationMessage(
        text="Use exit 10 1.",
        recommended_exits=[(10, 1)],
        avoid_exits=[(1, 1)],
        hazard_positions=[(3.0, 3.0)],
        confidence=0.2,
        credibility=1.0,
    )

    result = validate_generated_message(
        message,
        known_exits=[(10, 1), (1, 1)],
        known_hazards=[
            HazardSnapshot(position=(3.0, 3.0), kind="GAS", radius=2.0, severity=0.8)
        ],
        base_radius=8.0,
        max_radius=20.0,
        base_credibility=0.9,
        congested_exits=[(10, 1)],
        settings=validator_settings("strict"),
    )

    assert not result.accepted
    assert "unsafe_credibility:1.0" in result.reasons
    assert "low_confidence:0.20" in result.reasons
    assert "congested_recommendation:(10, 1)" in result.reasons
