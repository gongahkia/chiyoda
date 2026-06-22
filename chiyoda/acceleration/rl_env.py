from __future__ import annotations

from copy import deepcopy
from pathlib import Path
from typing import Any

import numpy as np

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.information.interventions import create_intervention_policy
from chiyoda.scenarios.manager import ScenarioManager


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


RLEvacuationEnv = ChiyodaRLEnv
