from __future__ import annotations

import json
from copy import deepcopy
from pathlib import Path
from typing import Any

import numpy as np

from chiyoda.acceleration.rl_env import ChiyodaRLEnv

try:
    from gymnasium import Env as _GymEnv
except Exception:

    class _GymEnv:  # type: ignore[no-redef]
        pass


DEFAULT_ACTIONS = [
    {"policy": "none"},
    {
        "policy": "global_broadcast",
        "start_step": 0,
        "interval_steps": 5,
        "budget_per_interval": 1,
        "message_radius": 8.0,
        "credibility": 0.88,
    },
    {
        "policy": "density_aware",
        "start_step": 0,
        "interval_steps": 5,
        "budget_per_interval": 1,
        "message_radius": 8.0,
        "credibility": 0.86,
    },
]
DEFAULT_PPO_BASELINE = Path("data/baselines/ppo_smoke_discrete_policy.json")


def train_ppo_baseline(
    *,
    scenario_file: str | Path = "scenarios/benchmark/transit_cbrn.yaml",
    output_dir: str | Path = "data/baselines",
    total_timesteps: int = 256,
    seed: int = 42,
    require_sb3: bool = True,
) -> dict[str, Any]:
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    metadata_path = out / "ppo_smoke_discrete_policy.json"
    try:
        from stable_baselines3 import PPO
    except Exception as exc:
        if require_sb3:
            raise RuntimeError("stable-baselines3 is required for PPO training") from exc
        artifact = _fallback_artifact(
            scenario_file=scenario_file,
            seed=seed,
            total_timesteps=total_timesteps,
            reason=str(exc),
        )
        metadata_path.write_text(json.dumps(artifact, indent=2) + "\n")
        return artifact | {"path": str(metadata_path)}

    env = DiscreteInterventionEnv(scenario_file, seed=seed)
    model = PPO("MlpPolicy", env, verbose=0, seed=seed, n_steps=16, batch_size=16)
    model.learn(total_timesteps=int(total_timesteps))
    model_path = out / "ppo_chiyoda_smoke.zip"
    model.save(model_path)
    action_index = _model_action_index(model)
    artifact = {
        "algorithm": "PPO",
        "backend": "stable-baselines3",
        "trained_with_stable_baselines3": True,
        "policy_source": "stable-baselines3 model prediction cached as action_index",
        "scenario_file": str(scenario_file),
        "total_timesteps": int(total_timesteps),
        "seed": int(seed),
        "model_path": str(model_path),
        "actions": DEFAULT_ACTIONS,
        "action_index": action_index,
    }
    metadata_path.write_text(json.dumps(artifact, indent=2) + "\n")
    return artifact | {"path": str(metadata_path)}


def load_discrete_policy_artifact(path: str | Path = DEFAULT_PPO_BASELINE) -> dict[str, Any]:
    source = Path(path)
    if not source.exists():
        raise FileNotFoundError(source)
    return json.loads(source.read_text())


def policy_from_artifact(path: str | Path = DEFAULT_PPO_BASELINE) -> dict[str, Any]:
    artifact = load_discrete_policy_artifact(path)
    actions = artifact.get("actions") or DEFAULT_ACTIONS
    action_index = int(artifact.get("action_index", 0)) % len(actions)
    return deepcopy(actions[action_index])


def _model_action_index(model: Any) -> int:
    action, _ = model.predict(np.zeros((1, 4), dtype=np.float32), deterministic=True)
    return int(np.asarray(action).reshape(-1)[0]) % len(DEFAULT_ACTIONS)


class DiscreteInterventionEnv(_GymEnv):
    metadata = {"render_modes": []}

    def __init__(self, scenario_file: str | Path, *, seed: int | None = None) -> None:
        self.env = ChiyodaRLEnv(
            scenario_file,
            intervention_slots=[{"policy": "none"}],
            max_episode_steps=80,
            seed=seed,
        )
        self.actions = DEFAULT_ACTIONS
        self._install_spaces()

    def reset(self, *, seed: int | None = None, options: dict[str, Any] | None = None):
        observation, info = self.env.reset(seed=seed, options=options)
        return observation.astype(np.float32), info

    def step(self, action):
        observation, reward, terminated, truncated, info = self.env.step(
            self.actions[int(action) % len(self.actions)]
        )
        return observation.astype(np.float32), float(reward), terminated, truncated, info

    def close(self) -> None:
        self.env.close()

    def _install_spaces(self) -> None:
        try:
            from gymnasium import spaces
        except Exception:
            self.action_space = _DiscreteActionSpace(len(self.actions))
            self.observation_space = _ArrayObservationSpace((1, 4), np.float32)
            return
        self.action_space = spaces.Discrete(len(self.actions))
        self.observation_space = spaces.Box(
            low=0.0, high=np.inf, shape=(1, 4), dtype=np.float32
        )


def _fallback_artifact(
    *,
    scenario_file: str | Path,
    seed: int,
    total_timesteps: int,
    reason: str,
) -> dict[str, Any]:
    return {
        "algorithm": "PPO",
        "backend": "stable-baselines3",
        "trained_with_stable_baselines3": False,
        "fallback": "deterministic_discrete_policy",
        "fallback_reason": reason,
        "scenario_file": str(scenario_file),
        "total_timesteps": int(total_timesteps),
        "seed": int(seed),
        "actions": DEFAULT_ACTIONS,
        "action_index": 1,
    }


class _DiscreteActionSpace:
    def __init__(self, n: int) -> None:
        self.n = int(n)
        self.shape = ()
        self.dtype = np.int64

    def sample(self) -> int:
        return 0


class _ArrayObservationSpace:
    def __init__(self, shape: tuple[int, ...], dtype: Any) -> None:
        self.shape = shape
        self.dtype = dtype

    def contains(self, value: Any) -> bool:
        array = np.asarray(value)
        return array.shape == self.shape and np.isfinite(array).all()
