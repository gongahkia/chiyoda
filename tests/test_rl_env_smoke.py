from __future__ import annotations

import numpy as np
import yaml

from chiyoda.acceleration.rl_env import (
    ChiyodaParallelRLEnv,
    ChiyodaRLEnv,
    create_rl_env,
)
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


def test_two_player_rl_env_selected_from_yaml(tmp_path):
    scenario_file = _two_player_scenario_file(tmp_path)

    env = create_rl_env(scenario_file, max_episode_steps=100)

    assert isinstance(env, ChiyodaParallelRLEnv)
    observations, infos = env.reset(seed=3)
    assert set(observations) == {"defender", "adversary"}
    assert observations["defender"].shape == (6,)
    assert infos["adversary"]["role"] == "adversary"


def test_two_player_random_rollout_runs_100_steps(tmp_path):
    scenario_file = _two_player_scenario_file(tmp_path)
    env = ChiyodaParallelRLEnv(scenario_file, max_episode_steps=100)
    observations, _ = env.reset(seed=11)
    assert env.agents == ["defender", "adversary"]

    for _ in range(100):
        actions = {agent: env.action_space(agent).sample() for agent in env.agents}
        observations, rewards, terminations, truncations, infos = env.step(actions)
        assert set(rewards) == {"defender", "adversary"}
        assert np.isfinite(list(rewards.values())).all()
        assert all(np.isfinite(obs).all() for obs in observations.values())
        if all(terminations.values()) or all(truncations.values()):
            break

    assert infos["defender"]["step"] == 100
    assert truncations == {"defender": True, "adversary": True}
    assert env.agents == []


def _done(simulation) -> bool:
    return simulation.current_step >= simulation.config.max_steps or all(
        agent.has_evacuated
        or getattr(agent, "is_responder", False)
        or getattr(agent, "is_hostile", False)
        for agent in simulation.agents
    )


def _two_player_scenario_file(tmp_path):
    scenario = {
        "scenario": {
            "name": "two_player_smoke",
            "layout": {
                "floors": [
                    {
                        "id": "0",
                        "z": 0.0,
                        "text": "XXXXX\nX@..X\nX...X\nXXXXX",
                    }
                ]
            },
            "population": {
                "total": 1,
                "cohorts": [{"name": "baseline", "count": 1, "familiarity": 0.0}],
            },
            "information": {"mode": "asymmetric", "observation_radius": 3.0},
            "simulation": {"max_steps": 100, "random_seed": 1},
            "rl": {
                "mode": "two_player",
                "defender_actions": [
                    {"policy": "none"},
                    {
                        "policy": "global_broadcast",
                        "start_step": 0,
                        "interval_steps": 1,
                        "budget_per_interval": 1,
                    },
                ],
                "adversary_actions": [
                    {"policy": "none"},
                    {"objective": "false-protective-action", "budget": 1},
                    {"objective": "threat-amplification", "budget": 1},
                ],
            },
        }
    }
    path = tmp_path / "two_player.yaml"
    path.write_text(yaml.safe_dump(scenario, sort_keys=False))
    return path
