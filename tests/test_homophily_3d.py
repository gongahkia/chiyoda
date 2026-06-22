from __future__ import annotations

import json

import pytest

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.environment.layout import Layout
from chiyoda.navigation.connectors import ConnectorQueue
from chiyoda.scenarios.manager import ScenarioManager


def _layout(text: str, *, z: float = 0.0) -> dict:
    return {"floors": [{"id": "0", "z": z, "text": text}]}


def test_group_attachment_roles_and_mobility_defaults_are_loaded():
    scenario = {
        "name": "family_group",
        "layout": _layout("XXXXXXX\nX@...EX\nXXXXXXX\n"),
        "population": {
            "total": 3,
            "cohorts": [
                {
                    "name": "family",
                    "count": 3,
                    "group_size": 3,
                    "mobility_class": "wheelchair",
                    "separation_anxiety_threshold": 2.4,
                }
            ],
        },
    }

    sim = ScenarioManager().build_simulation(scenario)
    agents = sim.agents

    assert {agent.group_id for agent in agents} == {1}
    assert {agent.family_id for agent in agents} == {"family_1"}
    assert [agent.role_in_group for agent in agents] == ["leader", "member", "member"]
    assert all(
        agent.separation_anxiety_threshold == pytest.approx(2.4) for agent in agents
    )
    assert all(agent.mobility_class == "wheelchair" for agent in agents)
    assert all(agent.breathing_height_m == pytest.approx(1.1) for agent in agents)
    assert all(agent.base_speed == pytest.approx(1.34 * 0.55) for agent in agents)


def test_homophily_weighted_destination_choice_prefers_matching_exit_profile():
    scenario = {
        "name": "homophily_exit",
        "layout": _layout("XXXXXXX\nXE.@.EX\nXXXXXXX\n"),
        "population": {
            "total": 1,
            "cohorts": [
                {
                    "name": "east_group",
                    "count": 1,
                    "familiarity": 1.0,
                    "homophily_profile": {"community": "east"},
                    "homophily_weight": 0.6,
                }
            ],
        },
        "destination_profiles": [
            {"cell": {"floor": "0", "x": 1, "y": 1}, "profile": {"community": "west"}},
            {"cell": {"floor": "0", "x": 5, "y": 1}, "profile": {"community": "east"}},
        ],
        "simulation": {"max_steps": 1, "random_seed": 4},
        "information": {"mode": "perfect"},
    }

    sim = ScenarioManager().build_simulation(scenario)
    sim.setup_information()
    agent = sim.agents[0]
    agent.update_intention(sim)

    assert agent.target_exit == ("0", 5, 1)


def test_height_aware_smoke_exposure_and_connector_duration():
    scenario = {
        "name": "height_smoke",
        "layout": _layout("XXXXX\nX@.EX\nXXXXX\n"),
        "population": {
            "total": 2,
            "cohorts": [
                {
                    "name": "low",
                    "count": 1,
                    "breathing_height_m": 0.8,
                    "spawn_cells": [{"floor": "0", "x": 1, "y": 1}],
                },
                {
                    "name": "high",
                    "count": 1,
                    "breathing_height_m": 2.1,
                    "spawn_cells": [{"floor": "0", "x": 1, "y": 1}],
                },
            ],
        },
        "hazards": [
            {
                "type": "SMOKE",
                "location": [1.5, 1.5, 0.0],
                "radius": 3.0,
                "severity": 1.0,
                "height_aware": True,
                "layer_base_m": 1.8,
                "layer_top_m": 3.0,
                "vertical_decay_m": 0.2,
            }
        ],
        "simulation": {"max_steps": 1, "random_seed": 3},
    }
    sim = ScenarioManager().build_simulation(scenario)
    sim._ensure_bootstrapped()
    by_cohort = {agent.cohort_name: agent.current_hazard_load for agent in sim.agents}

    assert by_cohort["high"] > by_cohort["low"] * 20

    layout = Layout.from_floors(
        [
            {"id": "0", "z": 0.0, "text": "XXX\nX.EX\nXXX"},
            {"id": "1", "z": 4.0, "text": "XXX\nX.EX\nXXX"},
        ],
        connectors=[
            {
                "id": "stairs_1",
                "type": "stairs",
                "from": {"floor": "0", "x": 1, "y": 1},
                "to": {"floor": "1", "x": 1, "y": 1},
                "speed_multiplier": 1.0,
            }
        ],
    )
    connector = layout.connectors[0]
    queue = ConnectorQueue.from_connector(connector)

    assert connector.height_delta_m == pytest.approx(4.0)
    assert queue.transfer_duration(
        connector.from_cell, connector.to_cell
    ) == pytest.approx(4.0 / 1.34)


def test_equity_metrics_report_group_and_mobility_gaps():
    scenario = {
        "name": "equity",
        "layout": _layout("XXXXXX\nX@..EX\nXXXXXX\n"),
        "population": {
            "total": 2,
            "cohorts": [
                {"name": "fast", "count": 1, "mobility_class": "standard"},
                {"name": "slow", "count": 1, "mobility_class": "walker"},
            ],
        },
    }
    sim = ScenarioManager().build_simulation(scenario)
    sim.agents[0].has_evacuated = True
    sim.completed_agents = [sim.agents[0]]
    sim.travel_times_s = [2.0, 8.0]
    sim.agents[0].hazard_exposure = 0.5
    sim.agents[1].hazard_exposure = 1.5

    metrics = SimulationAnalytics().calculate_performance_metrics(sim)

    assert metrics["left_behind_index"] == pytest.approx(1.0)
    assert json.loads(metrics["exposure_by_group"]) == {"fast": 0.5, "slow": 1.5}
    assert json.loads(metrics["exposure_by_mobility_class"]) == {
        "standard": 0.5,
        "walker": 1.5,
    }
    assert metrics["percentile_gap_time_to_safety_s"] > 0.0
