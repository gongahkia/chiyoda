from __future__ import annotations

from time import perf_counter

from chiyoda.information.projection import (
    DispatchProjectionRequest,
    project_dispatch_message,
)
from chiyoda.scenarios.manager import ScenarioManager


def test_avoid_hazard_projection_reduces_exposure_estimate():
    agents = [
        {"id": 1, "x": 1.0, "y": 1.0, "z": 0.0, "entropy": 0.4},
        {"id": 2, "x": 9.0, "y": 9.0, "z": 0.0, "entropy": 0.2},
    ]
    hazards = [{"x": 1.2, "y": 1.1, "z": 0.0, "radius": 3.0, "severity": 0.8}]

    result = project_dispatch_message(
        agents,
        hazards,
        DispatchProjectionRequest(
            message_type="avoid_hazard",
            target=(1.0, 1.0, 0.0),
            radius=4.0,
            credibility=0.9,
            horizon_steps=30,
        ),
    )

    assert result.recipients == 1
    assert result.mean_belief_delta > 0
    assert result.exposure_delta < 0
    assert result.harmful_convergence_delta < 0


def test_projection_runs_under_500ms_on_station_baseline():
    sim = ScenarioManager().load_scenario("scenarios/station_baseline.yaml")
    agents = [
        {
            "id": agent.id,
            "x": float(agent.pos[0]),
            "y": float(agent.pos[1]),
            "z": float(agent.pos[2]),
            "entropy": 0.5,
        }
        for agent in sim.agents
    ]

    started = perf_counter()
    result = project_dispatch_message(
        agents,
        [],
        DispatchProjectionRequest(
            message_type="route_guidance",
            target=(20.0, 10.0, 0.0),
            radius=18.0,
            credibility=0.9,
            horizon_steps=30,
        ),
    )
    elapsed_ms = (perf_counter() - started) * 1000.0

    assert result.recipients > 0
    assert elapsed_ms < 500
    assert result.latency_ms < 500
