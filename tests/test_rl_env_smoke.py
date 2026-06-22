from __future__ import annotations

import numpy as np

from chiyoda.acceleration.rl_env import ChiyodaRLEnv
from chiyoda.scenarios.manager import ScenarioManager

SCENARIO = "scenarios/benchmark/transit_cbrn.yaml"


def test_random_policy_rollout_terminates_with_finite_reward():
    env = ChiyodaRLEnv(SCENARIO, max_episode_steps=100)
    observation, info = env.reset(seed=123)
    assert observation.shape == (1, 4)
    assert info["step"] == 0

    rng = np.random.default_rng(7)
    actions = [
        {"policy": "none"},
        {
            "policy": "global_broadcast",
            "start_step": 0,
            "interval_steps": 1,
            "budget_per_interval": 1,
            "message_radius": 6.0,
        },
        {
            "policy": "density_aware",
            "start_step": 0,
            "interval_steps": 1,
            "budget_per_interval": 1,
            "message_radius": 5.0,
        },
    ]

    terminated = truncated = False
    for _ in range(100):
        action = actions[int(rng.integers(0, len(actions)))]
        observation, reward, terminated, truncated, info = env.step(action)
        assert np.isfinite(observation).all()
        assert np.isfinite(reward)
        if terminated or truncated:
            break

    assert terminated or truncated
    assert info["step"] <= 100


def test_noop_rollout_matches_direct_simulation():
    env = ChiyodaRLEnv(SCENARIO)
    env.reset(seed=42)
    while True:
        _, _, terminated, truncated, _ = env.step({"policy": "none"})
        if terminated or truncated:
            break

    direct = ScenarioManager().load_scenario(SCENARIO, random_seed=42)
    direct._ensure_bootstrapped()
    while not _done(direct):
        direct.attach_intervention_policy(None)
        direct.step()

    assert env.simulation.current_step == direct.current_step
    assert len(env.simulation.completed_agents) == len(direct.completed_agents)
    env_positions = np.array([agent.pos for agent in env.simulation.agents])
    direct_positions = np.array([agent.pos for agent in direct.agents])
    np.testing.assert_allclose(env_positions, direct_positions)


def _done(simulation) -> bool:
    return simulation.current_step >= simulation.config.max_steps or all(
        agent.has_evacuated
        or getattr(agent, "is_responder", False)
        or getattr(agent, "is_hostile", False)
        for agent in simulation.agents
    )
