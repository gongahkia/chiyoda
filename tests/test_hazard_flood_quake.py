from __future__ import annotations

import numpy as np

from chiyoda.agents.commuter import Commuter
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard
from chiyoda.environment.layout import Layout
from chiyoda.scenarios.manager import ScenarioManager


def test_flood_hazard_evolves_inundation_field_and_route_penalty():
    layout = Layout.from_text(
        "XXXXXX\n"
        "X@..EX\n"
        "X....X\n"
        "X....X\n"
        "XXXXXX\n"
    )
    hazard = Hazard(
        pos=(2.0, 2.5, 0.0),
        kind="FLOOD",
        radius=1.0,
        severity=0.8,
        spread_rate=0.4,
        diffusion_rate=0.05,
        flow_vector=(1.0, 0.0),
        inundation_depth_m=0.15,
        inundation_rise_rate_mps=0.1,
        flood_depth_threshold_m=0.5,
        max_depth_m=1.2,
    )
    sim = Simulation(
        layout=layout,
        agents=[],
        exits=[Exit(pos=("0", 4, 1))],
        hazards=[hazard],
        config=SimulationConfig(hazard_avoidance_weight=2.0),
    )

    hazard.step(1.0, sim)

    assert hazard.radius > 1.0
    assert hazard.inundation_field
    assert hazard.intensity_at(np.array([2.5, 2.5, 0.0])) > 0.0
    assert sim.hazard_penalty_at_cell(("0", 2, 2)) > sim.hazard_penalty_at_cell(("0", 4, 1))


def test_aftershock_damages_terrain_and_triggers_re_evacuation_wave():
    layout = Layout.from_text(
        "XXXXXXX\n"
        "X@..EX\n"
        "X..@.X\n"
        "XXXXXXX\n"
    )
    agents = [
        Commuter(id=0, pos=layout.world_position(("0", 1, 1)), floor_id="0"),
        Commuter(id=1, pos=layout.world_position(("0", 3, 2)), floor_id="0", release_step=100),
    ]
    hazard = Hazard(
        pos=(3.0, 2.0, 0.0),
        kind="EARTHQUAKE",
        radius=3.0,
        severity=0.9,
        aftershock_schedule=(0,),
        aftershock_damage_increment=0.6,
        damage_radius=3.0,
        re_evacuation_radius=4.0,
    )
    sim = Simulation(
        layout=layout,
        agents=agents,
        exits=[Exit(pos=("0", 4, 1))],
        hazards=[hazard],
        config=SimulationConfig(max_steps=1, dt=0.1, random_seed=5),
    )

    sim.step()

    assert sim.aftershock_events
    assert sim.aftershock_events[0]["affected_cells"] > 0
    assert sim.terrain_damage_cells
    assert agents[1].release_step == 0
    assert agents[1].re_evacuation_count == 1
    assert sim.hazard_penalty_at_cell(("0", 3, 2)) > 0.0


def test_flood_and_quake_benchmark_scenarios_run():
    flood = ScenarioManager().load_scenario("scenarios/benchmark/flood_urban.yaml")
    flood.run()

    assert str(flood.hazards[0].kind).upper() == "FLOOD"
    assert flood.hazards[0].inundation_field
    assert flood.intervention_events
    assert flood.intervention_events[0].message_type == "flood_warning"

    quake = ScenarioManager().load_scenario("scenarios/benchmark/quake_aftershock.yaml")
    quake.run()

    assert str(quake.hazards[0].kind).upper() == "EARTHQUAKE"
    assert quake.aftershock_events
    assert quake.terrain_damage_cells
    assert any(getattr(agent, "re_evacuation_count", 0) > 0 for agent in quake.agents)
    assert quake.intervention_events[0].message_type == "aftershock_warning"
