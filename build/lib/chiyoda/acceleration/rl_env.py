from __future__ import annotations

from copy import deepcopy
from pathlib import Path
from typing import Any

import numpy as np

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.information.interventions import create_intervention_policy
from chiyoda.information.warfare import create_hostile_channels
from chiyoda.scenarios.manager import ScenarioManager

try:
    from pettingzoo import ParallelEnv as _ParallelEnv
except Exception:

    class _ParallelEnv:  # type: ignore[no-redef]
        pass


class CompositeInterventionPolicy:
    def __init__(self, policies: list[Any]) -> None:
        self.policies = policies

    def execute(self, simulation) -> list[Any]:
        events: list[Any] = []
        for policy in self.policies:
            events.extend(policy.execute(simulation))
        return events


class ChiyodaRLEnv:
    """Small Gymnasium-style wrapper around ``Simulation.step``."""

    metadata = {"render_modes": []}

    def __init__(
        self,
        scenario_file: str | Path,
        *,
        intervention_slots: list[dict[str, Any]] | None = None,
        max_episode_steps: int | None = None,
        seed: int | None = None,
    ) -> None:
        self.scenario_file = str(scenario_file)
        self.manager = ScenarioManager()
        self.base_scenario = self.manager.load_config(self.scenario_file)
        base_interventions = self.base_scenario.get("interventions")
        self.intervention_slots = (
            deepcopy(intervention_slots)
            if intervention_slots is not None
            else [deepcopy(base_interventions or {"policy": "none"})]
        )
        self.max_episode_steps = max_episode_steps
        self.seed = seed
        self.analytics = SimulationAnalytics()
        self.simulation = None
        self._last_observation = np.zeros(
            (len(self.intervention_slots), 4), dtype=float
        )
        self._last_metrics: dict[str, Any] = {}
        self._terminated = False
        self._truncated = False
        self._install_optional_spaces()

    def reset(
        self,
        *,
        seed: int | None = None,
        options: dict[str, Any] | None = None,
    ) -> tuple[np.ndarray, dict[str, Any]]:
        del options
        scenario = deepcopy(self.base_scenario)
        resolved_seed = self.seed if seed is None else seed
        if resolved_seed is not None:
            scenario.setdefault("simulation", {})
            scenario["simulation"]["random_seed"] = int(resolved_seed)
        self.simulation = self.manager.build_simulation(scenario)
        self._apply_action({"policy": "none"})
        self.simulation._ensure_bootstrapped()
        self._terminated = False
        self._truncated = False
        observation = self._observation()
        return observation, self._info()

    def step(
        self, action: dict[str, Any] | list[dict[str, Any]] | None
    ) -> tuple[np.ndarray, float, bool, bool, dict[str, Any]]:
        if self.simulation is None:
            self.reset()
        if self._terminated or self._truncated:
            raise RuntimeError("episode is done; call reset() before step()")

        before_evacuated = len(self.simulation.completed_agents)
        before_exposure = self._mean_exposure()
        self._apply_action(action)
        self.simulation.step()
        observation = self._observation()
        reward = self._reward(before_evacuated, before_exposure)
        self._terminated = self._all_evacuees_done()
        self._truncated = self._step_limit_reached()
        return observation, reward, self._terminated, self._truncated, self._info()

    def close(self) -> None:
        self.simulation = None

    def _apply_action(
        self, action: dict[str, Any] | list[dict[str, Any]] | None
    ) -> None:
        slots = self._action_slots(action)
        self.intervention_slots = slots
        policies = []
        for slot in slots:
            policy = create_intervention_policy(slot)
            if policy is not None:
                policies.append(policy)
        if len(policies) == 0:
            self.simulation.attach_intervention_policy(None)
        elif len(policies) == 1:
            self.simulation.attach_intervention_policy(policies[0])
        else:
            self.simulation.attach_intervention_policy(
                CompositeInterventionPolicy(policies)
            )

    def _action_slots(
        self, action: dict[str, Any] | list[dict[str, Any]] | None
    ) -> list[dict[str, Any]]:
        if action is None:
            raw_slots: list[dict[str, Any]] = deepcopy(self.intervention_slots)
        elif isinstance(action, list):
            raw_slots = deepcopy(action)
        elif "interventions" in action:
            interventions = action["interventions"]
            if isinstance(interventions, list):
                raw_slots = deepcopy(interventions)
            else:
                raw_slots = [deepcopy(interventions)]
        else:
            raw_slots = [deepcopy(action)]

        if len(raw_slots) == 0:
            raw_slots = [{"policy": "none"}]

        merged = []
        for index, slot in enumerate(raw_slots):
            base = deepcopy(
                self.intervention_slots[min(index, len(self.intervention_slots) - 1)]
            )
            base.update(slot)
            merged.append(base)
        return merged

    def _observation(self) -> np.ndarray:
        sim = self.simulation
        latest = sim.step_history[-1] if sim.step_history else None
        metrics = self.analytics.calculate_performance_metrics(sim)
        row = np.array(
            [
                float(getattr(latest, "global_entropy", 0.0)),
                self._mean_exposure(),
                float(getattr(latest, "mean_density", 0.0)),
                float(metrics.get("harmful_convergence_index_induced", 0.0)),
            ],
            dtype=float,
        )
        self._last_metrics = metrics
        self._last_observation = np.vstack(
            [row.copy() for _ in range(max(1, len(self.intervention_slots)))]
        )
        return self._last_observation.copy()

    def _reward(self, before_evacuated: int, before_exposure: float) -> float:
        evacuated_delta = len(self.simulation.completed_agents) - before_evacuated
        exposure_delta = max(0.0, self._mean_exposure() - before_exposure)
        remaining = sum(
            1
            for agent in self.simulation.agents
            if not agent.has_evacuated
            and not getattr(agent, "is_responder", False)
            and not getattr(agent, "is_hostile", False)
        )
        hci = float(self._last_metrics.get("harmful_convergence_index_induced", 0.0))
        return float(evacuated_delta - exposure_delta - 0.01 * remaining - hci)

    def _info(self) -> dict[str, Any]:
        sim = self.simulation
        return {
            "step": int(sim.current_step),
            "time_s": float(sim.time_s),
            "metrics": dict(self._last_metrics),
            "interventions": deepcopy(self.intervention_slots),
        }

    def _all_evacuees_done(self) -> bool:
        return all(
            agent.has_evacuated
            or getattr(agent, "is_responder", False)
            or getattr(agent, "is_hostile", False)
            for agent in self.simulation.agents
        )

    def _step_limit_reached(self) -> bool:
        configured = int(self.simulation.config.max_steps)
        limit = (
            configured
            if self.max_episode_steps is None
            else min(configured, self.max_episode_steps)
        )
        return int(self.simulation.current_step) >= int(limit)

    def _mean_exposure(self) -> float:
        agents = [
            agent
            for agent in self.simulation.agents
            if not getattr(agent, "is_responder", False)
            and not getattr(agent, "is_hostile", False)
        ]
        if not agents:
            return 0.0
        return float(
            np.mean([float(getattr(agent, "hazard_exposure", 0.0)) for agent in agents])
        )

    def _install_optional_spaces(self) -> None:
        try:
            from gymnasium import spaces
        except Exception:
            self.observation_space = None
            self.action_space = None
            return
        slots = max(1, len(self.intervention_slots))
        self.observation_space = spaces.Box(
            low=0.0, high=np.inf, shape=(slots, 4), dtype=np.float32
        )
        self.action_space = spaces.Dict({})


class ChiyodaParallelRLEnv(_ParallelEnv):
    """PettingZoo ParallelEnv-compatible two-player wrapper."""

    metadata = {"name": "chiyoda_parallel_v0", "render_modes": []}
    possible_agents = ["defender", "adversary"]
    max_num_agents = 2

    def __init__(
        self,
        scenario_file: str | Path,
        *,
        max_episode_steps: int | None = None,
        seed: int | None = None,
    ) -> None:
        self.scenario_file = str(scenario_file)
        self.single_env = ChiyodaRLEnv(
            scenario_file,
            max_episode_steps=max_episode_steps,
            seed=seed,
        )
        self.rl_config = self.single_env.base_scenario.get("rl", {}) or {}
        self.defender_actions = _action_templates(
            self.rl_config.get("defender_actions"),
            _default_defender_actions(),
        )
        self.adversary_actions = _action_templates(
            self.rl_config.get("adversary_actions"),
            _default_adversary_actions(),
        )
        self.adversary_slot = deepcopy(
            self.rl_config.get("adversary_slot") or _default_adversary_slot()
        )
        self.agents = self.possible_agents[:]
        self._last_shared_observation = np.zeros(6, dtype=np.float32)
        self._install_parallel_spaces()

    @property
    def unwrapped(self) -> ChiyodaParallelRLEnv:
        return self

    def reset(
        self,
        seed: int | None = None,
        options: dict[str, Any] | None = None,
    ) -> tuple[dict[str, np.ndarray], dict[str, dict[str, Any]]]:
        del options
        self.single_env.reset(seed=seed)
        self.single_env.simulation.attach_hostile_channels([])
        self.agents = self.possible_agents[:]
        observation = self._shared_observation()
        infos = self._agent_infos(
            defender_action={"policy": "none"},
            adversary_action={"policy": "none"},
        )
        return self._agent_observations(observation), infos

    def step(self, actions: dict[str, Any] | None) -> tuple[
        dict[str, np.ndarray],
        dict[str, float],
        dict[str, bool],
        dict[str, bool],
        dict[str, dict[str, Any]],
    ]:
        if not self.agents:
            return {}, {}, {}, {}, {}
        if self.single_env.simulation is None:
            self.reset()
        if self.single_env._terminated or self.single_env._truncated:
            raise RuntimeError("episode is done; call reset() before step()")

        action_payload = actions or {}
        defender_action = self._resolve_action(
            "defender", action_payload.get("defender")
        )
        adversary_action = self._resolve_action(
            "adversary", action_payload.get("adversary")
        )

        sim = self.single_env.simulation
        before_evacuated = len(sim.completed_agents)
        before_exposure = self.single_env._mean_exposure()
        before_hostile_recipients = self._hostile_recipients()

        self.single_env._apply_action(defender_action)
        self._apply_adversary_action(adversary_action)
        sim.step()

        self.single_env._observation()
        defender_reward = self.single_env._reward(before_evacuated, before_exposure)
        hostile_delta = self._hostile_recipients() - before_hostile_recipients
        adversary_reward = float(-defender_reward + 0.1 * max(0, hostile_delta))
        self.single_env._terminated = self.single_env._all_evacuees_done()
        self.single_env._truncated = self.single_env._step_limit_reached()
        terminated = bool(self.single_env._terminated)
        truncated = bool(self.single_env._truncated)

        observation = self._shared_observation()
        live_agents = self.agents[:]
        observations = self._agent_observations(observation)
        rewards = {"defender": float(defender_reward), "adversary": adversary_reward}
        terminations = {agent: terminated for agent in live_agents}
        truncations = {agent: truncated for agent in live_agents}
        infos = self._agent_infos(
            defender_action=defender_action,
            adversary_action=adversary_action,
        )
        if terminated or truncated:
            self.agents = []
        return observations, rewards, terminations, truncations, infos

    def state(self) -> np.ndarray:
        return self._last_shared_observation.copy()

    def render(self) -> None:
        return None

    def close(self) -> None:
        self.single_env.close()
        self.agents = []

    def observation_space(self, agent: str) -> Any:
        return self.observation_spaces[agent]

    def action_space(self, agent: str) -> Any:
        return self.action_spaces[agent]

    def _resolve_action(self, agent: str, action: Any) -> dict[str, Any]:
        templates = (
            self.defender_actions if agent == "defender" else self.adversary_actions
        )
        if action is None:
            return deepcopy(templates[0])
        if isinstance(action, (np.integer, int)):
            return deepcopy(templates[int(action) % len(templates)])
        return deepcopy(action)

    def _apply_adversary_action(self, action: dict[str, Any] | list[Any]) -> None:
        sim = self.single_env.simulation
        channels = self._hostile_channel_payloads(action)
        for channel in channels:
            channel["start_step"] = int(sim.current_step)
            channel["interval_steps"] = max(1, int(channel.get("interval_steps", 1)))
            channel["budget"] = max(0, int(channel.get("budget", 1)))
        sim.attach_hostile_channels(create_hostile_channels(channels))

    def _hostile_channel_payloads(
        self, action: dict[str, Any] | list[Any]
    ) -> list[dict[str, Any]]:
        if isinstance(action, list):
            return [dict(item) for item in action]
        if not action:
            return []
        if action.get("policy") == "none" or action.get("objective") == "none":
            return []
        if "hostile_channels" in action:
            return [dict(item) for item in action["hostile_channels"] or []]
        channel = deepcopy(self.adversary_slot)
        channel.update(action)
        return [channel]

    def _shared_observation(self) -> np.ndarray:
        base = self.single_env._observation()
        row = base[0] if base.ndim == 2 else base
        metrics = self.single_env._last_metrics
        observation = np.array(
            [
                float(row[0]),
                float(row[1]),
                float(row[2]),
                float(row[3]),
                float(metrics.get("hostile_channel_event_count", 0.0)),
                float(metrics.get("hostile_channel_recipients", 0.0)),
            ],
            dtype=np.float32,
        )
        self._last_shared_observation = observation
        return observation.copy()

    def _agent_observations(self, observation: np.ndarray) -> dict[str, np.ndarray]:
        return {agent: observation.copy() for agent in self.agents}

    def _agent_infos(
        self,
        *,
        defender_action: dict[str, Any],
        adversary_action: dict[str, Any],
    ) -> dict[str, dict[str, Any]]:
        base = self.single_env._info()
        return {
            "defender": {
                **base,
                "role": "defender",
                "action": deepcopy(defender_action),
            },
            "adversary": {
                **base,
                "role": "adversary",
                "action": deepcopy(adversary_action),
            },
        }

    def _hostile_recipients(self) -> int:
        return int(
            sum(
                event.recipients
                for event in self.single_env.simulation.hostile_channel_events
            )
        )

    def _install_parallel_spaces(self) -> None:
        try:
            from gymnasium import spaces
        except Exception:
            self.observation_spaces = {
                agent: _ArrayObservationSpace((6,), np.float32)
                for agent in self.possible_agents
            }
            self.action_spaces = {
                "defender": _DiscreteActionSpace(len(self.defender_actions)),
                "adversary": _DiscreteActionSpace(len(self.adversary_actions)),
            }
            return
        self.observation_spaces = {
            agent: spaces.Box(low=0.0, high=np.inf, shape=(6,), dtype=np.float32)
            for agent in self.possible_agents
        }
        self.action_spaces = {
            "defender": spaces.Discrete(len(self.defender_actions)),
            "adversary": spaces.Discrete(len(self.adversary_actions)),
        }


def create_rl_env(
    scenario_file: str | Path,
    *,
    max_episode_steps: int | None = None,
    seed: int | None = None,
) -> ChiyodaRLEnv | ChiyodaParallelRLEnv:
    scenario = ScenarioManager().load_config(str(scenario_file))
    rl_cfg = scenario.get("rl", {}) or {}
    if str(rl_cfg.get("mode", "single_agent")) == "two_player":
        return ChiyodaParallelRLEnv(
            scenario_file,
            max_episode_steps=max_episode_steps,
            seed=seed,
        )
    return ChiyodaRLEnv(
        scenario_file,
        max_episode_steps=max_episode_steps,
        seed=seed,
    )


class _DiscreteActionSpace:
    def __init__(self, n: int) -> None:
        self.n = int(n)
        self.shape = ()
        self.dtype = np.int64
        self._rng = np.random.default_rng()

    def sample(self) -> int:
        return int(self._rng.integers(0, self.n))

    def seed(self, seed: int | None = None) -> None:
        self._rng = np.random.default_rng(seed)


class _ArrayObservationSpace:
    def __init__(self, shape: tuple[int, ...], dtype: Any) -> None:
        self.shape = shape
        self.dtype = dtype

    def contains(self, value: Any) -> bool:
        array = np.asarray(value)
        return array.shape == self.shape and np.isfinite(array).all()


def _action_templates(
    value: Any, default: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    if not value:
        return deepcopy(default)
    return [deepcopy(item) for item in value]


def _default_defender_actions() -> list[dict[str, Any]]:
    return [
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


def _default_adversary_actions() -> list[dict[str, Any]]:
    return [
        {"policy": "none"},
        {"objective": "false-protective-action", "plausibility": 0.65},
        {"objective": "threat-amplification", "plausibility": 0.65},
    ]


def _default_adversary_slot() -> dict[str, Any]:
    return {
        "id": "rl_adversary",
        "channel_type": "gossip",
        "objective": "false-protective-action",
        "budget": 1,
        "start_step": 0,
        "interval_steps": 1,
        "plausibility": 0.65,
    }


RLEvacuationEnv = ChiyodaRLEnv
