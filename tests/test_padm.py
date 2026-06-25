from __future__ import annotations

import numpy as np

from chiyoda.agents.base import INTENTION_EVACUATE, INTENTION_EXPLORE
from chiyoda.agents.commuter import Commuter
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.environment.exits import Exit
from chiyoda.environment.layout import Layout
from chiyoda.information.field import BeliefVector, ExitBelief, InformationField
from chiyoda.information.padm import (
    PADM_DECIDE,
    PADM_PERSONALIZE,
    PADM_RECEIVE,
    PADM_UNDERSTAND,
    PADMStageConfig,
    padm_counter_values,
)


def _agent(pos=(0.5, 0.5, 0.0)) -> Commuter:
    return Commuter(id=1, pos=np.array(pos, dtype=float))


def _sim(agent: Commuter, *, muted: str | None = None) -> Simulation:
    layout = Layout.from_text("XXXXX\nX@.EX\nXXXXX")
    exit_pos = layout.exit_positions()[0]
    agent.pos = np.array(layout.world_position(("0", 1, 1)), dtype=float)
    config = SimulationConfig(max_steps=1, dt=0.1, random_seed=7)
    if muted is not None:
        config.padm_enabled_stages = PADMStageConfig.with_muted(muted).enabled_stages
    return Simulation(layout, [agent], [Exit(pos=exit_pos)], config=config)


def test_padm_receive_can_be_muted_independently():
    field = InformationField(4, 3)
    field.exit_world_positions = {("0", 2, 1): (2.5, 1.5, 0.0)}
    agent = _agent((1.5, 1.5, 0.0))
    observations = [(agent, tuple(agent.pos), 2.0)]

    field.padm_receive(
        observations,
        [("0", 2, 1)],
        [],
        current_step=3,
        stage_config=PADMStageConfig.with_muted(PADM_RECEIVE),
    )

    assert agent.beliefs.exit_beliefs == {}
    assert padm_counter_values(agent)["padm_receive"] == 0

    field.padm_receive(observations, [("0", 2, 1)], [], current_step=3)

    assert ("0", 2, 1) in agent.beliefs.exit_beliefs
    assert padm_counter_values(agent)["padm_receive"] == 1


def test_padm_understand_can_be_muted_independently():
    field = InformationField(4, 3, decay_rate=0.5)
    agent = _agent()
    agent.beliefs = BeliefVector(
        exit_beliefs={("0", 2, 1): ExitBelief(position=("0", 2, 1), exists_prob=1.0)}
    )
    observations = [(agent, tuple(agent.pos), 2.0)]

    field.padm_understand(
        observations,
        dt=1.0,
        stage_config=PADMStageConfig.with_muted(PADM_UNDERSTAND),
    )

    assert agent.beliefs.exit_beliefs[("0", 2, 1)].freshness == 0.0
    assert padm_counter_values(agent)["padm_understand"] == 0

    field.padm_understand(observations, dt=1.0)

    assert agent.beliefs.exit_beliefs[("0", 2, 1)].freshness > 0.0
    assert padm_counter_values(agent)["padm_understand"] == 1


def test_padm_personalize_can_be_muted_independently():
    agent = _agent()
    agent.current_hazard_load = 0.7
    sim = _sim(agent, muted=PADM_PERSONALIZE)
    observation_batch = [(agent, tuple(agent.pos), 5.0)]

    sim._padm_personalize(observation_batch)

    assert agent.personalized_risk == 0.0
    assert padm_counter_values(agent)["padm_personalize"] == 0

    sim.padm_stage_config = PADMStageConfig()
    sim._padm_personalize(observation_batch)

    assert agent.personalized_risk == 0.7
    assert padm_counter_values(agent)["padm_personalize"] == 1


def test_padm_decide_can_be_muted_independently():
    agent = _agent()
    sim = _sim(agent, muted=PADM_DECIDE)
    exit_pos = sim.layout.exit_positions()[0]
    agent.intention = INTENTION_EXPLORE
    agent.beliefs.exit_beliefs[exit_pos] = ExitBelief(
        position=exit_pos,
        exists_prob=1.0,
    )
    observation_batch = [(agent, tuple(agent.pos), 5.0)]

    sim._padm_decide(observation_batch)

    assert agent.intention == INTENTION_EXPLORE
    assert padm_counter_values(agent)["padm_decide"] == 0

    sim.padm_stage_config = PADMStageConfig()
    sim._padm_decide(observation_batch)

    assert agent.intention == INTENTION_EVACUATE
    assert padm_counter_values(agent)["padm_decide"] == 1


def test_padm_counters_reach_agent_telemetry():
    agent = _agent()
    sim = _sim(agent)

    sim.run()

    tel = sim.step_history[-1].agents[0]
    assert tel.padm_receive >= 1
    assert tel.padm_understand >= 1
    assert tel.padm_personalize >= 1
    assert tel.padm_decide >= 1
