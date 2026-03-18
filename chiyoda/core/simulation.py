from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple

import numpy as np

from chiyoda.analysis.telemetry import (
    AgentStepTelemetry,
    BottleneckStepTelemetry,
    StepTelemetry,
    detect_bottleneck_zones,
    zone_distance,
    zone_lookup,
)


@dataclass
class SimulationConfig:
    max_steps: int = 500
    dt: float = 0.1
    random_seed: Optional[int] = 42


class Simulation:
    """
    Chiyoda v2 Simulation runtime

    Responsibilities:
    - Hold references to environment, agents, and systems
    - Advance the simulation clock and step agents
    - Collect study-friendly telemetry during the run
    """

    def __init__(
        self,
        layout,
        agents: List["AgentBase"],
        exits: List["Exit"],
        hazards: Optional[List["Hazard"]] = None,
        config: Optional[SimulationConfig] = None,
    ) -> None:
        self.layout = layout
        self.agents = agents
        self.exits = exits
        self.hazards = hazards or []
        self.config = config or SimulationConfig()

        if self.config.random_seed is not None:
            np.random.seed(self.config.random_seed)

        self.current_step: int = 0
        self.time_s: float = 0.0
        self.completed_agents: List["AgentBase"] = []
        self.evacuated_at_step: List[int] = []
        self.density_history: List[float] = []
        self.risk_events: List[Dict[str, Any]] = []
        self.step_history: List[StepTelemetry] = []
        self.travel_times_s: List[float] = []

        self.exit_labels = {
            tuple(exit_.pos): f"Exit {idx + 1} ({exit_.pos[0]},{exit_.pos[1]})"
            for idx, exit_ in enumerate(self.exits)
        }
        self.exit_flow_cumulative = {label: 0 for label in self.exit_labels.values()}
        self.path_usage_grid = np.zeros((self.layout.height, self.layout.width), dtype=int)

        self.bottleneck_zones = detect_bottleneck_zones(layout)
        self.bottleneck_lookup = zone_lookup(self.bottleneck_zones)
        self.bottleneck_zone_map = {zone.zone_id: zone for zone in self.bottleneck_zones}
        self.bottleneck_dwell_samples = {zone.zone_id: [] for zone in self.bottleneck_zones}
        self.agent_zone_membership = {agent.id: None for agent in self.agents}
        self.agent_zone_entry_step: Dict[Tuple[int, str], int] = {}

        self.agent_traces = {
            agent.id: [(float(agent.pos[0]), float(agent.pos[1]))] for agent in self.agents
        }
        self._telemetry_bootstrapped = False
        self.navigation_replan_interval_steps = 6
        self.navigation_density_reroute_threshold = 0.55

        self.navigator = None  # type: ignore
        self.spatial_index = None  # type: ignore
        self.behavior_model = None  # type: ignore

    def attach_navigation(self, navigator) -> None:
        self.navigator = navigator

    def attach_spatial_index(self, spatial_index) -> None:
        self.spatial_index = spatial_index

    def attach_behavior_model(self, behavior_model) -> None:
        self.behavior_model = behavior_model

    def _grid_cell(self, pos: np.ndarray | Tuple[float, float]) -> Tuple[int, int]:
        x = int(np.clip(round(float(pos[0])), 0, self.layout.width - 1))
        y = int(np.clip(round(float(pos[1])), 0, self.layout.height - 1))
        return (x, y)

    def _ensure_agent_runtime_fields(self) -> None:
        for agent in self.agents:
            if not hasattr(agent, "current_speed"):
                agent.current_speed = 0.0
            if not hasattr(agent, "local_density"):
                agent.local_density = 0.0
            if not hasattr(agent, "travel_time_s"):
                agent.travel_time_s = 0.0
            if not hasattr(agent, "crowd_speed_factor"):
                agent.crowd_speed_factor = 1.0
            if not hasattr(agent, "last_navigation_step"):
                agent.last_navigation_step = -9999

    def _update_spatial_index(self) -> None:
        if self.spatial_index is not None:
            self.spatial_index.update(self.agents)

    def _refresh_agent_context(self) -> None:
        self._ensure_agent_runtime_fields()
        if self.spatial_index is None:
            for agent in self.agents:
                if agent.has_evacuated:
                    continue
                agent.local_density = 0.0
                agent.crowd_speed_factor = 1.0
            return

        for agent in self.agents:
            if agent.has_evacuated:
                continue
            agent.local_density = self.spatial_index.local_density(agent.pos, radius=1.5)
            agent.crowd_speed_factor = max(0.25, 1.0 - (agent.local_density / 2.0))

    def _empty_bottleneck_metrics(self) -> Dict[str, BottleneckStepTelemetry]:
        return {
            zone.zone_id: BottleneckStepTelemetry() for zone in self.bottleneck_zones
        }

    def _seed_bottleneck_membership(self) -> None:
        self.agent_zone_membership = {}
        self.agent_zone_entry_step = {}
        for agent in self.agents:
            if agent.has_evacuated:
                self.agent_zone_membership[agent.id] = None
                continue
            zone_id = self.bottleneck_lookup.get(self._grid_cell(agent.pos))
            self.agent_zone_membership[agent.id] = zone_id
            if zone_id is not None:
                self.agent_zone_entry_step[(agent.id, zone_id)] = self.current_step

    def _update_path_usage(self) -> None:
        for agent in self.agents:
            if agent.has_evacuated:
                continue
            x, y = self._grid_cell(agent.pos)
            self.path_usage_grid[y, x] += 1

    def _build_step_grids(self) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        occupancy = np.zeros((self.layout.height, self.layout.width), dtype=int)
        density_sum = np.zeros((self.layout.height, self.layout.width), dtype=float)
        speed_sum = np.zeros((self.layout.height, self.layout.width), dtype=float)

        for agent in self.agents:
            if agent.has_evacuated:
                continue
            x, y = self._grid_cell(agent.pos)
            occupancy[y, x] += 1
            density_sum[y, x] += float(agent.local_density)
            speed_sum[y, x] += float(agent.current_speed)

        density_grid = np.divide(
            density_sum,
            occupancy,
            out=np.zeros_like(density_sum),
            where=occupancy > 0,
        )
        speed_grid = np.divide(
            speed_sum,
            occupancy,
            out=np.zeros_like(speed_sum),
            where=occupancy > 0,
        )
        return occupancy, density_grid, speed_grid

    def _capture_step_telemetry(
        self,
        *,
        exit_flow_step: Dict[str, int],
        bottleneck_metrics: Dict[str, BottleneckStepTelemetry],
    ) -> None:
        self._update_path_usage()
        occupancy_grid, density_grid, speed_grid = self._build_step_grids()

        living_agents = [agent for agent in self.agents if not agent.has_evacuated]
        mean_speed = float(np.mean([agent.current_speed for agent in living_agents])) if living_agents else 0.0
        mean_density = (
            float(np.mean([agent.local_density for agent in living_agents])) if living_agents else 0.0
        )
        self.density_history.append(mean_density)

        agents = [
            AgentStepTelemetry(
                agent_id=agent.id,
                position=(float(agent.pos[0]), float(agent.pos[1])),
                cell=self._grid_cell(agent.pos),
                state=agent.state,
                speed=float(agent.current_speed),
                local_density=float(agent.local_density),
                target_exit=tuple(agent.target_exit) if agent.target_exit is not None else None,
                trail=tuple(self.agent_traces.get(agent.id, [])[-8:]),
            )
            for agent in living_agents
        ]

        self.step_history.append(
            StepTelemetry(
                step=self.current_step,
                time_s=self.time_s,
                occupancy_grid=occupancy_grid,
                density_grid=density_grid,
                speed_grid=speed_grid,
                path_usage_grid=self.path_usage_grid.copy(),
                agents=agents,
                exit_flow_cumulative=dict(self.exit_flow_cumulative),
                exit_flow_step=dict(exit_flow_step),
                bottlenecks={
                    zone_id: BottleneckStepTelemetry(
                        occupancy=metrics.occupancy,
                        inflow=metrics.inflow,
                        outflow=metrics.outflow,
                        queue_length=metrics.queue_length,
                        mean_dwell_s=metrics.mean_dwell_s,
                        mean_speed=metrics.mean_speed,
                        mean_density=metrics.mean_density,
                    )
                    for zone_id, metrics in bottleneck_metrics.items()
                },
                hazards=[hazard.snapshot() for hazard in self.hazards],
                evacuated_total=len(self.completed_agents),
                remaining=len(living_agents),
                mean_speed=mean_speed,
                mean_density=mean_density,
            )
        )

    def _update_bottleneck_metrics(self) -> Dict[str, BottleneckStepTelemetry]:
        metrics = self._empty_bottleneck_metrics()
        active_agents = [agent for agent in self.agents if not agent.has_evacuated]
        current_membership: Dict[int, Optional[str]] = {}

        for agent in self.agents:
            zone_id = None if agent.has_evacuated else self.bottleneck_lookup.get(self._grid_cell(agent.pos))
            current_membership[agent.id] = zone_id
            previous_zone = self.agent_zone_membership.get(agent.id)

            if previous_zone == zone_id:
                continue

            if previous_zone is not None:
                metrics[previous_zone].outflow += 1
                enter_step = self.agent_zone_entry_step.pop((agent.id, previous_zone), self.current_step)
                dwell_s = max((self.current_step - enter_step) * self.config.dt, self.config.dt)
                self.bottleneck_dwell_samples[previous_zone].append(dwell_s)

            if zone_id is not None:
                metrics[zone_id].inflow += 1
                self.agent_zone_entry_step[(agent.id, zone_id)] = self.current_step

            self.agent_zone_membership[agent.id] = zone_id

        for zone in self.bottleneck_zones:
            zone_agents = [
                agent
                for agent in active_agents
                if current_membership.get(agent.id) == zone.zone_id
            ]
            metrics[zone.zone_id].occupancy = len(zone_agents)
            metrics[zone.zone_id].mean_speed = (
                float(np.mean([agent.current_speed for agent in zone_agents])) if zone_agents else 0.0
            )
            metrics[zone.zone_id].mean_density = (
                float(np.mean([agent.local_density for agent in zone_agents])) if zone_agents else 0.0
            )
            samples = self.bottleneck_dwell_samples[zone.zone_id]
            metrics[zone.zone_id].mean_dwell_s = float(np.mean(samples)) if samples else 0.0
            metrics[zone.zone_id].queue_length = sum(
                1
                for agent in active_agents
                if current_membership.get(agent.id) != zone.zone_id
                and zone_distance(self._grid_cell(agent.pos), zone) <= 2
                and agent.current_speed <= max(0.4, agent.base_speed * 0.5)
            )

        return metrics

    def _bootstrap_telemetry(self) -> None:
        self._update_spatial_index()
        self._refresh_agent_context()
        self._seed_bottleneck_membership()
        self._capture_step_telemetry(
            exit_flow_step={label: 0 for label in self.exit_flow_cumulative},
            bottleneck_metrics=self._empty_bottleneck_metrics(),
        )
        self._telemetry_bootstrapped = True

    def _ensure_bootstrapped(self) -> None:
        if not self._telemetry_bootstrapped:
            self._bootstrap_telemetry()

    def step(self) -> None:
        self._ensure_bootstrapped()
        dt = self.config.dt

        for hz in self.hazards:
            hz.step(dt, self)

        self._update_spatial_index()
        self._refresh_agent_context()

        exit_flow_step = {label: 0 for label in self.exit_flow_cumulative}

        for agent in list(self.agents):
            if agent.has_evacuated:
                continue

            if self.behavior_model is not None:
                self.behavior_model.update_agent(agent, self)

            if self.navigator is not None:
                agent.update_navigation(self.navigator, self)

            previous_pos = np.array(agent.pos, copy=True)
            agent.step(dt, self)
            agent.current_speed = float(np.linalg.norm(agent.pos - previous_pos) / dt)
            agent.travel_time_s = self.time_s + dt
            self.agent_traces.setdefault(agent.id, []).append(
                (float(agent.pos[0]), float(agent.pos[1]))
            )

            if self.layout.is_exit(agent.pos):
                agent.has_evacuated = True
                self.completed_agents.append(agent)
                self.evacuated_at_step.append(self.current_step + 1)
                self.travel_times_s.append(agent.travel_time_s)
                exit_pos = tuple(map(int, agent.target_exit or self._grid_cell(agent.pos)))
                label = self.exit_labels.get(exit_pos)
                if label is None:
                    for known_exit, known_label in self.exit_labels.items():
                        if self._grid_cell(agent.pos) == known_exit:
                            label = known_label
                            break
                if label is not None:
                    self.exit_flow_cumulative[label] += 1
                    exit_flow_step[label] += 1

        self.current_step += 1
        self.time_s += dt

        self._update_spatial_index()
        self._refresh_agent_context()
        bottleneck_metrics = self._update_bottleneck_metrics()
        self._capture_step_telemetry(
            exit_flow_step=exit_flow_step,
            bottleneck_metrics=bottleneck_metrics,
        )

    def run(self, visualize: bool = False, visualizer=None) -> None:
        self._ensure_bootstrapped()
        for _ in range(self.config.max_steps):
            if all(a.has_evacuated for a in self.agents):
                break
            self.step()
            if visualize and visualizer is not None:
                visualizer.on_step(self)

    def live_state(self) -> Dict[str, Any]:
        self._ensure_bootstrapped()
        latest = self.step_history[-1]
        positions = (
            np.array([agent.position for agent in latest.agents], dtype=float)
            if latest.agents
            else np.zeros((0, 2), dtype=float)
        )

        return {
            "step": latest.step,
            "time_s": latest.time_s,
            "positions": positions,
            "agent_ids": [agent.agent_id for agent in latest.agents],
            "states": [agent.state for agent in latest.agents],
            "speeds": np.array([agent.speed for agent in latest.agents], dtype=float),
            "local_densities": np.array(
                [agent.local_density for agent in latest.agents], dtype=float
            ),
            "target_exits": [agent.target_exit for agent in latest.agents],
            "agents": [
                {
                    "id": agent.agent_id,
                    "position": agent.position,
                    "cell": agent.cell,
                    "state": agent.state,
                    "speed": agent.speed,
                    "local_density": agent.local_density,
                    "target_exit": agent.target_exit,
                    "trail": list(agent.trail),
                }
                for agent in latest.agents
            ],
            "exits": [e.pos for e in self.exits],
            "exit_labels": dict(self.exit_labels),
            "exit_flow_cumulative": dict(latest.exit_flow_cumulative),
            "exit_flow_step": dict(latest.exit_flow_step),
            "hazards": list(latest.hazards),
            "density": latest.mean_density,
            "mean_speed": latest.mean_speed,
            "occupancy_grid": latest.occupancy_grid.copy(),
            "density_grid": latest.density_grid.copy(),
            "speed_grid": latest.speed_grid.copy(),
            "path_usage_grid": latest.path_usage_grid.copy(),
            "bottlenecks": [
                {
                    "id": zone.zone_id,
                    "cells": list(zone.cells),
                    "orientation": zone.orientation,
                    "centroid": zone.centroid,
                    "metrics": latest.bottlenecks[zone.zone_id],
                }
                for zone in self.bottleneck_zones
            ],
            "evacuated": latest.evacuated_total,
            "remaining": latest.remaining,
        }
