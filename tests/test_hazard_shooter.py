from __future__ import annotations

from chiyoda.agents.base import INTENTION_FIGHT, INTENTION_HIDE, INTENTION_RUN
from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.environment.layout import Layout
from chiyoda.navigation.line_of_sight import line_of_sight
from chiyoda.scenarios.manager import ScenarioManager


def _scenario(*, info_mode: str, adjacent: bool = False) -> dict:
    hostile_x = 2 if adjacent else 6
    return {
        "name": "shooter_branch",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXXXX\nX@....EX\nX......X\nXXXXXXXX",
                }
            ]
        },
        "population": {
            "total": 1,
            "cohorts": [{"name": "public", "count": 1, "familiarity": 0.0, "base_rationality": 1.0}],
        },
        "hostile_agents": [
            {
                "name": "hostile",
                "spawn_cells": [{"floor": "0", "x": hostile_x, "y": 1}],
                "range_m": 8.0,
                "accuracy": 0.4,
            }
        ],
        "information": {"mode": info_mode, "observation_radius": 0.0},
        "behavior": {"freeze_probability": 0.0},
        "simulation": {"max_steps": 3, "random_seed": 3},
    }


def test_line_of_sight_respects_walls_and_floor_height():
    layout = Layout.from_floors(
        [
            {"id": "0", "z": 0.0, "text": "XXXXX\nX.X.X\nXXXXX"},
            {"id": "1", "z": 3.0, "text": "XXXXX\nX...X\nXXXXX"},
        ]
    )

    assert not line_of_sight(layout, (1.5, 1.5, 0.0), (3.5, 1.5, 0.0))
    assert not line_of_sight(layout, (1.5, 1.5, 0.0), (1.5, 1.5, 3.0))


def test_run_hide_fight_intention_branches():
    manager = ScenarioManager()

    run_sim = manager.build_simulation(_scenario(info_mode="perfect"))
    run_sim._ensure_bootstrapped()
    run_agent = next(agent for agent in run_sim.agents if not getattr(agent, "is_hostile", False))
    run_agent.update_intention(run_sim)
    assert run_agent.intention == INTENTION_RUN

    hide_sim = manager.build_simulation(_scenario(info_mode="none"))
    hide_sim._ensure_bootstrapped()
    hide_agent = next(agent for agent in hide_sim.agents if not getattr(agent, "is_hostile", False))
    hide_agent.update_intention(hide_sim)
    assert hide_agent.intention == INTENTION_HIDE

    fight_sim = manager.build_simulation(_scenario(info_mode="none", adjacent=True))
    fight_sim._ensure_bootstrapped()
    fight_agent = next(agent for agent in fight_sim.agents if not getattr(agent, "is_hostile", False))
    fight_agent.update_intention(fight_sim)
    assert fight_agent.intention == INTENTION_FIGHT


def test_transit_shooter_scenario_records_active_shooter_metrics():
    sim = ScenarioManager().load_scenario("scenarios/transit_shooter.yaml")
    sim.run()
    metrics = SimulationAnalytics().calculate_performance_metrics(sim)

    assert len([agent for agent in sim.agents if getattr(agent, "is_hostile", False)]) == 1
    assert metrics["active_shooter_event_count"] >= 1
    assert metrics["exposure_to_los"] > 0.0
