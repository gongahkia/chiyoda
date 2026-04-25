"""
ITED Simulation runtime.

Integrates information propagation, cognitive agents, multi-hazard physics,
and social force dynamics in a single simulation loop with study-grade telemetry.

Domain-agnostic: models entities navigating spatial environments under
propagating stimuli with heterogeneous information access.
"""
from __future__ import annotations
from dataclasses import dataclass
import random
from typing import Any, Dict, List, Optional, Tuple
import numpy as np
from chiyoda.acceleration import create_acceleration_backend
from chiyoda.analysis.telemetry import (
    AgentStepTelemetry, BottleneckStepTelemetry, StepTelemetry,
    detect_bottleneck_zones, zone_distance, zone_lookup,
)
from chiyoda.information.field import InformationField
from chiyoda.information.propagation import GossipModel, GossipConfig
from chiyoda.information.entropy import agent_entropy, global_entropy, belief_accuracy
from chiyoda.analysis.measurement import MeasurementLine


@dataclass
class SimulationConfig:
    max_steps: int = 500
    dt: float = 0.1
    random_seed: Optional[int] = 42
    hazard_avoidance_weight: float = 1.25
    acceleration_backend: str = "auto"
    # ITED config
    information_mode: str = "asymmetric"  # "perfect", "none", "asymmetric"
    info_decay_rate: float = 0.01
    observation_radius: float = 5.0
    gossip_radius: float = 2.0
    beacon_radius: float = 8.0


class Simulation:
    """
    ITED v3 Simulation runtime.

    Responsibilities:
    - Environment, agents, information field, and navigation systems
    - Advance clock, step agents, propagate information, evolve hazards
    - Collect study-friendly telemetry including entropy metrics
    """

    def __init__(
        self,
        layout,
        agents: List,
        exits: List,
        hazards: Optional[List] = None,
        config: Optional[SimulationConfig] = None,
    ) -> None:
        self.layout = layout
        self.agents = agents
        self.agent_lookup = {agent.id: agent for agent in agents}
        self.exits = exits
        self.hazards = hazards or []
        self.config = config or SimulationConfig()
        self.acceleration = create_acceleration_backend(self.config.acceleration_backend)

        if self.config.random_seed is not None:
            random.seed(self.config.random_seed)
            np.random.seed(self.config.random_seed)

        self.current_step: int = 0
        self.time_s: float = 0.0
        self.completed_agents: List = []
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

        self.navigator = None
        self.spatial_index = None
        self.behavior_model = None
        self.measurement_lines: List[MeasurementLine] = []
        self._prev_positions: Dict[int, np.ndarray] = {}
        self.intervention_policy = None
        self.intervention_events: List[Any] = []

        # ITED: information layer
        self.info_field = InformationField(
            width=layout.width,
            height=layout.height,
            decay_rate=self.config.info_decay_rate,
            observation_radius=self.config.observation_radius,
            beacon_radius=self.config.beacon_radius,
            gossip_radius=self.config.gossip_radius,
        )
        self.gossip_model = GossipModel(GossipConfig(gossip_radius=self.config.gossip_radius))
        self.entropy_history: List[float] = []
        self.accuracy_history: List[float] = []
        self.gossip_events: List[Dict[str, Any]] = []

    def attach_navigation(self, navigator) -> None:
        self.navigator = navigator

    def attach_spatial_index(self, spatial_index) -> None:
        self.spatial_index = spatial_index

    def attach_behavior_model(self, behavior_model) -> None:
        self.behavior_model = behavior_model

    def attach_measurement_lines(self, lines: List[MeasurementLine]) -> None:
        self.measurement_lines = list(lines)

    def attach_intervention_policy(self, policy) -> None:
        self.intervention_policy = policy

    def setup_information(self) -> None:
        """Initialize information field and seed agent beliefs."""
        exit_positions = [tuple(e.pos) for e in self.exits]
        # find beacons from layout
        beacon_positions = []
        from chiyoda.environment.layout import BEACON
        for y in range(self.layout.height):
            for x in range(self.layout.width):
                if self.layout.grid[y, x] == BEACON:
                    beacon_positions.append((float(x) + 0.5, float(y) + 0.5))
        self.info_field.set_ground_truth(exit_positions, beacon_positions or None)

        for agent in self.agents:
            if not hasattr(agent, 'beliefs'):
                continue
            if self.config.information_mode == "perfect":
                agent.beliefs = self.info_field.create_agent_beliefs(
                    agent_pos=(float(agent.pos[0]), float(agent.pos[1])),
                    familiarity=1.0,
                    known_exits=exit_positions,
                )
            elif self.config.information_mode == "none":
                agent.beliefs = self.info_field.create_agent_beliefs(
                    agent_pos=(float(agent.pos[0]), float(agent.pos[1])),
                    familiarity=0.0,
                )
            else: # asymmetric
                agent.beliefs = self.info_field.create_agent_beliefs(
                    agent_pos=(float(agent.pos[0]), float(agent.pos[1])),
                    familiarity=getattr(agent, 'familiarity', 0.5),
                )

    def _grid_cell(self, pos) -> Tuple[int, int]:
        x = int(np.clip(round(float(pos[0])), 0, self.layout.width - 1))
        y = int(np.clip(round(float(pos[1])), 0, self.layout.height - 1))
        return (x, y)

    def _released_agents(self) -> List:
        return [a for a in self.agents if a.is_released(self)]

    def _active_agents(self) -> List:
        return [a for a in self.agents if a.is_released(self) and not a.has_evacuated]

    def _pending_agents(self) -> List:
        return [a for a in self.agents if not a.has_evacuated and not a.is_released(self)]

    def _ensure_agent_runtime_fields(self) -> None:
        for agent in self.agents:
            for attr, default in [
                ("current_speed", 0.0), ("local_density", 0.0),
                ("travel_time_s", 0.0), ("crowd_speed_factor", 1.0),
                ("last_navigation_step", -9999), ("hazard_exposure", 0.0),
                ("current_hazard_load", 0.0), ("hazard_speed_factor", 1.0),
                ("hazard_risk", 0.0), ("evacuated_via", None),
            ]:
                if not hasattr(agent, attr):
                    setattr(agent, attr, default)

    def hazard_intensity_at(self, point) -> float:
        px = float(point[0])
        py = float(point[1])
        intensity = 0.0
        for hazard in self.hazards:
            if hasattr(hazard, 'intensity_at'):
                hx = float(hazard.pos[0])
                hy = float(hazard.pos[1])
                dx = px - hx
                dy = py - hy
                radius = float(hazard.radius)
                severity = float(hazard.severity)
                if radius <= 1e-6:
                    if dx * dx + dy * dy <= 0.75 * 0.75:
                        intensity += severity
                else:
                    dist_sq = dx * dx + dy * dy
                    if dist_sq <= radius * radius:
                        intensity += severity * max(0.0, 1.0 - ((dist_sq ** 0.5) / radius))
            else:
                radius = max(float(hazard.radius), 0.0)
                dx = px - float(hazard.pos[0])
                dy = py - float(hazard.pos[1])
                distance = float((dx * dx + dy * dy) ** 0.5)
                if radius <= 1e-6:
                    if distance <= 0.75:
                        intensity += float(hazard.severity)
                elif distance <= radius:
                    intensity += float(hazard.severity) * max(0.0, 1.0 - (distance / radius))
        return intensity

    def visibility_at(self, point) -> float:
        """Get visibility factor at a point considering all hazards."""
        pos = np.array(point, dtype=float)
        vis = 1.0
        for hazard in self.hazards:
            if hasattr(hazard, 'visibility_at'):
                vis *= hazard.visibility_at(pos)
        return vis

    def hazard_penalty_at_cell(self, cell: Tuple[int, int]) -> float:
        point = np.array([cell[0] + 0.5, cell[1] + 0.5], dtype=float)
        return self.config.hazard_avoidance_weight * self.hazard_intensity_at(point)

    def _update_spatial_index(self) -> None:
        if self.spatial_index is not None:
            self.spatial_index.update(self._active_agents())

    def _refresh_agent_context(self) -> None:
        self._ensure_agent_runtime_fields()
        active_agents = self._active_agents()
        active_ids = {a.id for a in active_agents}
        active_lookup = {a.id: i for i, a in enumerate(active_agents)}
        positions = (
            np.array([a.pos for a in active_agents], dtype=float)
            if active_agents else np.zeros((0, 2), dtype=float)
        )
        hazard_positions = (
            np.array([h.pos for h in self.hazards], dtype=float)
            if self.hazards else np.zeros((0, 2), dtype=float)
        )
        radii = (
            np.array([float(h.radius) for h in self.hazards], dtype=float)
            if self.hazards else np.zeros((0,), dtype=float)
        )
        severities = (
            np.array([float(h.severity) for h in self.hazards], dtype=float)
            if self.hazards else np.zeros((0,), dtype=float)
        )
        hazard_loads = self.acceleration.hazard_intensities(positions, hazard_positions, radii, severities)

        for agent in self.agents:
            if agent.has_evacuated or agent.id not in active_ids:
                agent.local_density = 0.0
                agent.crowd_speed_factor = 1.0
                agent.current_hazard_load = 0.0
                agent.hazard_speed_factor = 1.0
                continue
            if self.spatial_index is None:
                agent.local_density = 0.0
            else:
                agent.local_density = self.spatial_index.local_density(agent.pos, radius=1.5)
            agent.crowd_speed_factor = max(0.25, 1.0 - (agent.local_density / 2.0))
            agent.current_hazard_load = float(hazard_loads[active_lookup[agent.id]])
            # ITED: use physiology model if available
            if hasattr(agent, 'update_physiology'):
                agent.update_physiology(agent.current_hazard_load, self.config.dt)
            else:
                agent.hazard_speed_factor = max(0.45, 1.0 - (agent.current_hazard_load * 0.35))

    def _step_information(self) -> None:
        """ITED: information propagation step."""
        if self.config.information_mode == "perfect":
            return # skip — all agents have perfect info

        active = self._active_agents()
        dt = self.config.dt
        exit_positions = [tuple(e.pos) for e in self.exits]

        for agent in active:
            if not hasattr(agent, 'beliefs'):
                continue

            # direct observation
            vis = self.visibility_at(agent.pos)
            effective_vision = getattr(agent, 'vision_radius', self.config.observation_radius) * vis
            self.info_field.observe(
                agent.beliefs, (float(agent.pos[0]), float(agent.pos[1])),
                effective_vision, exit_positions, self.hazards, self.current_step,
            )

            # beacon broadcast
            self.info_field.beacon_broadcast(
                agent.beliefs, (float(agent.pos[0]), float(agent.pos[1])),
            )

            # belief decay
            self.info_field.decay_beliefs(agent.beliefs, dt)

            # update intention based on beliefs
            if hasattr(agent, 'update_intention'):
                agent.update_intention(self)

        # agent-to-agent gossip
        if self.spatial_index is not None:
            for agent in active:
                if not hasattr(agent, 'beliefs'):
                    continue
                neighbors = self.spatial_index.neighbor_agents(
                    agent.pos, radius=self.config.gossip_radius,
                )
                for other in neighbors:
                    if not hasattr(other, 'beliefs'):
                        continue
                    dist = float(np.linalg.norm(agent.pos - np.array(other.pos)))
                    transferred = self.gossip_model.exchange(
                        sender_beliefs=agent.beliefs,
                        receiver_beliefs=other.beliefs,
                        sender_credibility=getattr(agent, 'credibility', 0.5),
                        receiver_rationality=getattr(other, 'rationality', 0.8),
                        sender_state=agent.state,
                        distance=dist,
                    )
                    if transferred and (self.current_step % 10 == 0):
                        self.gossip_events.append({
                            "step": self.current_step,
                            "time_s": self.time_s,
                            "sender_id": agent.id,
                            "receiver_id": other.id,
                            "distance": dist,
                        })

    def _empty_bottleneck_metrics(self) -> Dict[str, BottleneckStepTelemetry]:
        return {zone.zone_id: BottleneckStepTelemetry() for zone in self.bottleneck_zones}

    def _seed_bottleneck_membership(self) -> None:
        self.agent_zone_membership = {}
        self.agent_zone_entry_step = {}
        for agent in self.agents:
            if agent.has_evacuated or not agent.is_released(self):
                self.agent_zone_membership[agent.id] = None
                continue
            zone_id = self.bottleneck_lookup.get(self._grid_cell(agent.pos))
            self.agent_zone_membership[agent.id] = zone_id
            if zone_id is not None:
                self.agent_zone_entry_step[(agent.id, zone_id)] = self.current_step

    def _update_path_usage(self) -> None:
        for agent in self._active_agents():
            x, y = self._grid_cell(agent.pos)
            self.path_usage_grid[y, x] += 1

    def _build_step_grids(self):
        active = self._active_agents()
        positions = np.array([a.pos for a in active], dtype=float) if active else np.zeros((0, 2), dtype=float)
        densities = np.array([float(a.local_density) for a in active], dtype=float) if active else np.zeros((0,), dtype=float)
        speeds = np.array([float(a.current_speed) for a in active], dtype=float) if active else np.zeros((0,), dtype=float)
        return self.acceleration.aggregate_step_grids(self.layout.width, self.layout.height, positions, densities, speeds)

    def _capture_step_telemetry(self, *, exit_flow_step, bottleneck_metrics) -> None:
        self._update_path_usage()
        occupancy_grid, density_grid, speed_grid = self._build_step_grids()
        living = self._active_agents()
        mean_speed = float(np.mean([a.current_speed for a in living])) if living else 0.0
        mean_density = float(np.mean([a.local_density for a in living])) if living else 0.0
        self.density_history.append(mean_density)

        # ITED: entropy metrics
        all_beliefs = [a.beliefs for a in living if hasattr(a, 'beliefs')]
        total_exits = len(self.exits)
        total_hazards = len(self.hazards)
        h_global = global_entropy(all_beliefs, total_exits, total_hazards) if all_beliefs else 0.0
        self.entropy_history.append(h_global)

        agents_tel = []
        for agent in living:
            h_agent = 0.0
            acc = 1.0
            imp = 0.0
            intention = "EVACUATE"
            if hasattr(agent, 'beliefs'):
                h_agent = agent_entropy(agent.beliefs, total_exits, total_hazards)
                true_exits = [tuple(e.pos) for e in self.exits]
                acc = belief_accuracy(agent.beliefs, true_exits, self.hazards)
            if hasattr(agent, 'physiology'):
                imp = agent.physiology.impairment_level
            if hasattr(agent, 'intention'):
                intention = agent.intention

            agents_tel.append(AgentStepTelemetry(
                agent_id=agent.id,
                position=(float(agent.pos[0]), float(agent.pos[1])),
                cell=self._grid_cell(agent.pos),
                state=agent.state,
                speed=float(agent.current_speed),
                local_density=float(agent.local_density),
                target_exit=tuple(agent.target_exit) if agent.target_exit is not None else None,
                cohort_name=str(agent.cohort_name),
                group_id=agent.group_id,
                leader_id=agent.leader_id,
                hazard_exposure=float(agent.hazard_exposure),
                hazard_load=float(agent.current_hazard_load),
                trail=tuple(self.agent_traces.get(agent.id, [])[-8:]),
                entropy=h_agent,
                belief_accuracy=acc,
                impairment=imp,
                decision_mode=intention,
            ))

        self.step_history.append(StepTelemetry(
            step=self.current_step, time_s=self.time_s,
            occupancy_grid=occupancy_grid, density_grid=density_grid,
            speed_grid=speed_grid, path_usage_grid=self.path_usage_grid.copy(),
            agents=agents_tel,
            exit_flow_cumulative=dict(self.exit_flow_cumulative),
            exit_flow_step=dict(exit_flow_step),
            bottlenecks={
                zid: BottleneckStepTelemetry(
                    occupancy=m.occupancy, inflow=m.inflow, outflow=m.outflow,
                    queue_length=m.queue_length, mean_dwell_s=m.mean_dwell_s,
                    mean_speed=m.mean_speed, mean_density=m.mean_density,
                ) for zid, m in bottleneck_metrics.items()
            },
            hazards=[h.snapshot() for h in self.hazards],
            evacuated_total=len(self.completed_agents),
            remaining=len(living),
            pending_release=len(self._pending_agents()),
            mean_speed=mean_speed, mean_density=mean_density,
            global_entropy=h_global,
        ))

    def _update_bottleneck_metrics(self) -> Dict[str, BottleneckStepTelemetry]:
        metrics = self._empty_bottleneck_metrics()
        active = self._active_agents()
        active_ids = {a.id for a in active}
        current_membership: Dict[int, Optional[str]] = {}
        for agent in self.agents:
            zone_id = None
            if agent.id in active_ids:
                zone_id = self.bottleneck_lookup.get(self._grid_cell(agent.pos))
            current_membership[agent.id] = zone_id
            prev = self.agent_zone_membership.get(agent.id)
            if prev == zone_id:
                continue
            if prev is not None:
                metrics[prev].outflow += 1
                enter = self.agent_zone_entry_step.pop((agent.id, prev), self.current_step)
                dwell = max((self.current_step - enter) * self.config.dt, self.config.dt)
                self.bottleneck_dwell_samples[prev].append(dwell)
            if zone_id is not None:
                metrics[zone_id].inflow += 1
                self.agent_zone_entry_step[(agent.id, zone_id)] = self.current_step
            self.agent_zone_membership[agent.id] = zone_id
        for zone in self.bottleneck_zones:
            za = [a for a in active if current_membership.get(a.id) == zone.zone_id]
            metrics[zone.zone_id].occupancy = len(za)
            metrics[zone.zone_id].mean_speed = float(np.mean([a.current_speed for a in za])) if za else 0.0
            metrics[zone.zone_id].mean_density = float(np.mean([a.local_density for a in za])) if za else 0.0
            samples = self.bottleneck_dwell_samples[zone.zone_id]
            metrics[zone.zone_id].mean_dwell_s = float(np.mean(samples)) if samples else 0.0
            metrics[zone.zone_id].queue_length = sum(
                1 for a in active
                if current_membership.get(a.id) != zone.zone_id
                and zone_distance(self._grid_cell(a.pos), zone) <= 2
                and a.current_speed <= max(0.4, a.base_speed * 0.5)
            )
        return metrics

    def _bootstrap_telemetry(self) -> None:
        if self.config.random_seed is not None:
            random.seed(self.config.random_seed)
            np.random.seed(self.config.random_seed)
        self.setup_information()
        self._update_spatial_index()
        self._refresh_agent_context()
        self._seed_bottleneck_membership()
        self._capture_step_telemetry(
            exit_flow_step={l: 0 for l in self.exit_flow_cumulative},
            bottleneck_metrics=self._empty_bottleneck_metrics(),
        )
        self._telemetry_bootstrapped = True

    def _ensure_bootstrapped(self) -> None:
        if not self._telemetry_bootstrapped:
            self._bootstrap_telemetry()

    def step(self) -> None:
        self._ensure_bootstrapped()
        dt = self.config.dt

        # evolve hazards
        for hz in self.hazards:
            hz.step(dt, self)

        self._update_spatial_index()
        self._refresh_agent_context()

        # ITED: information propagation
        self._step_information()
        if self.intervention_policy is not None:
            self.intervention_events.extend(self.intervention_policy.execute(self))

        exit_flow_step = {l: 0 for l in self.exit_flow_cumulative}

        if self.navigator is not None and hasattr(self.navigator, "clear_cache"):
            self.navigator.clear_cache()

        for agent in list(self._active_agents()):
            if self.behavior_model is not None:
                self.behavior_model.update_agent(agent, self)
            if self.navigator is not None:
                agent.update_navigation(self.navigator, self)

            self._prev_positions[agent.id] = np.array(agent.pos, copy=True)
            agent.step(dt, self)
            agent.current_speed = float(np.linalg.norm(agent.pos - self._prev_positions[agent.id]) / dt)
            agent.travel_time_s = self.time_s + dt

            # hazard exposure (already handled by physiology for cognitive agents)
            if not hasattr(agent, 'physiology'):
                agent.current_hazard_load = self.hazard_intensity_at(agent.pos)
                agent.hazard_exposure += agent.current_hazard_load * dt
                agent.hazard_risk = max(float(agent.hazard_risk), float(agent.current_hazard_load))

            if agent.current_hazard_load > 0.05:
                self.risk_events.append({
                    "step": self.current_step, "time_s": self.time_s,
                    "agent_id": agent.id, "hazard_load": float(agent.current_hazard_load),
                })

            self.agent_traces.setdefault(agent.id, []).append(
                (float(agent.pos[0]), float(agent.pos[1]))
            )

            if self.layout.is_exit(agent.pos):
                # responders don't evacuate
                if getattr(agent, 'is_responder', False):
                    continue
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
                    agent.evacuated_via = label
                    self.exit_flow_cumulative[label] += 1
                    exit_flow_step[label] += 1

        self.current_step += 1
        self.time_s += dt
        self._update_spatial_index()
        self._refresh_agent_context()
        bn_metrics = self._update_bottleneck_metrics()
        # measurement line recording
        for ml in self.measurement_lines:
            ml.record(self.current_step, self.time_s, dt, self._active_agents(), self._prev_positions)
        self._capture_step_telemetry(exit_flow_step=exit_flow_step, bottleneck_metrics=bn_metrics)

    def run(self, visualize: bool = False, visualizer=None) -> None:
        self._ensure_bootstrapped()
        for _ in range(self.config.max_steps):
            if all(
                a.has_evacuated or getattr(a, 'is_responder', False)
                for a in self.agents
            ):
                break
            self.step()
            if visualize and visualizer is not None:
                visualizer.on_step(self)

    def live_state(self) -> Dict[str, Any]:
        self._ensure_bootstrapped()
        latest = self.step_history[-1]
        positions = (
            np.array([a.position for a in latest.agents], dtype=float)
            if latest.agents else np.zeros((0, 2), dtype=float)
        )
        return {
            "step": latest.step, "time_s": latest.time_s,
            "positions": positions,
            "agent_ids": [a.agent_id for a in latest.agents],
            "states": [a.state for a in latest.agents],
            "speeds": np.array([a.speed for a in latest.agents], dtype=float),
            "local_densities": np.array([a.local_density for a in latest.agents], dtype=float),
            "target_exits": [a.target_exit for a in latest.agents],
            "agents": [{
                "id": a.agent_id, "position": a.position, "cell": a.cell,
                "state": a.state, "speed": a.speed, "local_density": a.local_density,
                "target_exit": a.target_exit, "cohort_name": a.cohort_name,
                "group_id": a.group_id, "leader_id": a.leader_id,
                "hazard_exposure": a.hazard_exposure, "hazard_load": a.hazard_load,
                "trail": list(a.trail),
                "entropy": a.entropy, "belief_accuracy": a.belief_accuracy,
                "impairment": a.impairment, "decision_mode": a.decision_mode,
            } for a in latest.agents],
            "exits": [e.pos for e in self.exits],
            "exit_labels": dict(self.exit_labels),
            "exit_flow_cumulative": dict(latest.exit_flow_cumulative),
            "exit_flow_step": dict(latest.exit_flow_step),
            "hazards": list(latest.hazards),
            "density": latest.mean_density, "mean_speed": latest.mean_speed,
            "global_entropy": latest.global_entropy,
            "occupancy_grid": latest.occupancy_grid.copy(),
            "density_grid": latest.density_grid.copy(),
            "speed_grid": latest.speed_grid.copy(),
            "path_usage_grid": latest.path_usage_grid.copy(),
            "bottlenecks": [{
                "id": z.zone_id, "cells": list(z.cells),
                "orientation": z.orientation, "centroid": z.centroid,
                "metrics": latest.bottlenecks[z.zone_id],
            } for z in self.bottleneck_zones],
            "evacuated": latest.evacuated_total,
            "remaining": latest.remaining,
            "pending_release": latest.pending_release,
            "intervention_events": list(self.intervention_events),
            "acceleration_backend": self.acceleration.name,
            "requested_acceleration_backend": self.acceleration.requested_backend,
        }
