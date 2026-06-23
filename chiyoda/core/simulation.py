"""
ITED Simulation runtime.

Integrates information propagation, cognitive agents, multi-hazard physics,
and social force dynamics in a single simulation loop with study-grade telemetry.

Domain-agnostic: models entities navigating spatial environments under
propagating stimuli with heterogeneous information access.
"""

from __future__ import annotations

import random
from dataclasses import dataclass
from typing import Any

import numpy as np

from chiyoda.acceleration import create_acceleration_backend
from chiyoda.analysis.measurement import MeasurementLine
from chiyoda.analysis.telemetry import (
    AgentStepTelemetry,
    BottleneckStepTelemetry,
    StepTelemetry,
    detect_bottleneck_zones,
    zone_distance,
    zone_lookup,
)
from chiyoda.information.entropy import agent_entropy, belief_accuracy, global_entropy
from chiyoda.information.field import InformationField
from chiyoda.information.padm import (
    PADM_DECIDE,
    PADM_PERSONALIZE,
    PADM_RECEIVE,
    PADM_STAGES,
    PADMStageConfig,
    padm_counter_values,
    padm_stage_enabled,
    record_padm_stage,
)
from chiyoda.information.propagation import GossipConfig, GossipModel
from chiyoda.information.warfare import evaluate_pending_provenance
from chiyoda.navigation.connectors import ConnectorQueue, ConnectorQueueEvent
from chiyoda.navigation.line_of_sight import line_of_sight


@dataclass
class SimulationConfig:
    max_steps: int = 500
    dt: float = 0.1
    random_seed: int | None = 42
    hazard_avoidance_weight: float = 1.25
    acceleration_backend: str = "auto"
    density_slowdown_scale: float = 1.0
    min_crowd_speed_factor: float = 0.25
    # ITED config
    information_mode: str = "asymmetric"  # "perfect", "none", "asymmetric"
    info_decay_rate: float = 0.01
    observation_radius: float = 5.0
    gossip_radius: float = 2.0
    beacon_radius: float = 8.0
    padm_enabled_stages: tuple[str, ...] = PADM_STAGES


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
        agents: list,
        exits: list,
        hazards: list | None = None,
        config: SimulationConfig | None = None,
    ) -> None:
        self.layout = layout
        self.agents = agents
        for agent in self.agents:
            if np.array(agent.pos).shape[0] < 3:
                agent.pos = np.array(
                    [
                        float(agent.pos[0]),
                        float(agent.pos[1]),
                        self.layout.floor_z(
                            getattr(agent, "floor_id", self.layout.primary_floor_id)
                        ),
                    ],
                    dtype=float,
                )
            if not getattr(agent, "floor_id", None):
                agent.floor_id = self.layout.floor_for_z(float(agent.pos[2]))
        self.agent_lookup = {agent.id: agent for agent in agents}
        self.exits = exits
        self.hazards = hazards or []
        self.config = config or SimulationConfig()
        self.padm_stage_config = PADMStageConfig.from_enabled(
            self.config.padm_enabled_stages
        )
        self.acceleration = create_acceleration_backend(
            self.config.acceleration_backend
        )

        if self.config.random_seed is not None:
            random.seed(self.config.random_seed)
            np.random.seed(self.config.random_seed)

        self.current_step: int = 0
        self.time_s: float = 0.0
        self.completed_agents: list = []
        self.evacuated_at_step: list[int] = []
        self.density_history: list[float] = []
        self.risk_events: list[dict[str, Any]] = []
        self.step_history: list[StepTelemetry] = []
        self.travel_times_s: list[float] = []

        self.exit_labels = {
            tuple(
                exit_.pos
            ): f"Exit {idx + 1} ({exit_.pos[0]}:{exit_.pos[1]},{exit_.pos[2]})"
            for idx, exit_ in enumerate(self.exits)
        }
        self.exit_flow_cumulative = {label: 0 for label in self.exit_labels.values()}
        self.path_usage_by_floor = {
            floor_id: np.zeros_like(floor.grid, dtype=int)
            for floor_id, floor in self.layout.floors.items()
        }
        self.path_usage_grid = self.path_usage_by_floor[self.layout.primary_floor_id]

        self.bottleneck_zones = detect_bottleneck_zones(layout)
        self.bottleneck_lookup = zone_lookup(self.bottleneck_zones)
        self.bottleneck_zone_map = {
            zone.zone_id: zone for zone in self.bottleneck_zones
        }
        self.bottleneck_dwell_samples = {
            zone.zone_id: [] for zone in self.bottleneck_zones
        }
        self.agent_zone_membership = {agent.id: None for agent in self.agents}
        self.agent_zone_entry_step: dict[tuple[int, str], int] = {}

        self.agent_traces = {
            agent.id: [tuple(float(value) for value in agent.pos)]
            for agent in self.agents
        }
        self._telemetry_bootstrapped = False
        self.navigation_replan_interval_steps = 6
        self.navigation_density_reroute_threshold = 0.55

        self.navigator = None
        self.spatial_index = None
        self.behavior_model = None
        self.measurement_lines: list[MeasurementLine] = []
        self._prev_positions: dict[int, np.ndarray] = {}
        self.intervention_policy = None
        self.intervention_events: list[Any] = []
        self.agent_decision_policy = None
        self.agent_decision_events: list[Any] = []
        self.llm_call_audit: list[dict[str, Any]] = []
        self.connector_queues = {
            connector.id: ConnectorQueue.from_connector(connector)
            for connector in self.layout.connectors
        }
        self.connector_usage_cumulative = {
            connector.id: 0 for connector in self.layout.connectors
        }
        self.connector_events: list[dict[str, Any]] = []
        self.impossible_floor_jumps: list[dict[str, Any]] = []
        self.wui_egress_segments: list[dict[str, Any]] = []
        self.road_segment_cells: dict[tuple[str, int, int], dict[str, Any]] = {}
        self.mode_switch_events: list[dict[str, Any]] = []
        self.terrain_damage_cells: dict[tuple[str, int, int], float] = {}
        self.aftershock_events: list[dict[str, Any]] = []

        # ITED: information layer
        self.info_field = InformationField(
            width=layout.width,
            height=layout.height,
            decay_rate=self.config.info_decay_rate,
            observation_radius=self.config.observation_radius,
            beacon_radius=self.config.beacon_radius,
            gossip_radius=self.config.gossip_radius,
        )
        self.gossip_model = GossipModel(
            GossipConfig(gossip_radius=self.config.gossip_radius)
        )
        self.entropy_history: list[float] = []
        self.accuracy_history: list[float] = []
        self.gossip_events: list[dict[str, Any]] = []
        self.hostile_channels: list[Any] = []
        self.hostile_channel_events: list[Any] = []
        self.hostile_agent_events: list[dict[str, Any]] = []
        self.destination_profiles: dict[tuple, dict[str, Any]] = {}

    def attach_navigation(self, navigator) -> None:
        self.navigator = navigator

    def attach_spatial_index(self, spatial_index) -> None:
        self.spatial_index = spatial_index

    def attach_behavior_model(self, behavior_model) -> None:
        self.behavior_model = behavior_model

    def attach_measurement_lines(self, lines: list[MeasurementLine]) -> None:
        self.measurement_lines = list(lines)

    def attach_intervention_policy(self, policy) -> None:
        self.intervention_policy = policy

    def attach_agent_decision_policy(self, policy) -> None:
        self.agent_decision_policy = policy

    def attach_hostile_channels(self, channels: list[Any]) -> None:
        self.hostile_channels = list(channels)

    def attach_wui_egress(self, segments: list[dict[str, Any]]) -> None:
        self.wui_egress_segments = list(segments)
        self.road_segment_cells = {}
        for segment in self.wui_egress_segments:
            for cell in segment.get("cells", []):
                self.road_segment_cells[tuple(cell)] = segment

    def setup_information(self) -> None:
        """Initialize information field and seed agent beliefs."""
        exit_positions = [tuple(e.pos) for e in self.exits]
        beacon_positions = [
            tuple(self.layout.world_position(cell))
            for cell in self.layout.beacon_positions()
        ]
        self.info_field.exit_world_positions = {
            tuple(exit_.pos): tuple(self.layout.world_position(exit_.pos))
            for exit_ in self.exits
        }
        self.info_field.set_ground_truth(exit_positions, beacon_positions or None)

        for agent in self.agents:
            if not hasattr(agent, "beliefs"):
                continue
            if self.config.information_mode == "perfect":
                agent.beliefs = self.info_field.create_agent_beliefs(
                    agent_pos=tuple(float(value) for value in agent.pos),
                    familiarity=1.0,
                    known_exits=exit_positions,
                )
            elif self.config.information_mode == "none":
                agent.beliefs = self.info_field.create_agent_beliefs(
                    agent_pos=tuple(float(value) for value in agent.pos),
                    familiarity=0.0,
                )
            else:  # asymmetric
                agent.beliefs = self.info_field.create_agent_beliefs(
                    agent_pos=tuple(float(value) for value in agent.pos),
                    familiarity=getattr(agent, "familiarity", 0.5),
                )

    def _grid_cell(self, value) -> tuple:
        if hasattr(value, "pos"):
            return self.layout.cell(
                value.pos, floor_id=getattr(value, "floor_id", None)
            )
        return self.layout.cell(value)

    def _released_agents(self) -> list:
        return [a for a in self.agents if a.is_released(self)]

    def _active_agents(self) -> list:
        return [a for a in self.agents if a.is_released(self) and not a.has_evacuated]

    def _pending_agents(self) -> list:
        return [
            a for a in self.agents if not a.has_evacuated and not a.is_released(self)
        ]

    def _ensure_agent_runtime_fields(self) -> None:
        for agent in self.agents:
            for attr, default in [
                ("current_speed", 0.0),
                ("local_density", 0.0),
                ("travel_time_s", 0.0),
                ("crowd_speed_factor", 1.0),
                ("last_navigation_step", -9999),
                ("hazard_exposure", 0.0),
                ("current_hazard_load", 0.0),
                ("hazard_speed_factor", 1.0),
                ("hazard_risk", 0.0),
                ("evacuated_via", None),
                ("evacuation_mode", "pedestrian"),
                ("mode_switch_step", None),
                ("re_evacuation_count", 0),
                ("re_evacuation_step", None),
            ]:
                if not hasattr(agent, attr):
                    setattr(agent, attr, default)

    def hazard_intensity_at(self, point) -> float:
        pos = np.array(point, dtype=float)
        intensity = 0.0
        for hazard in self.hazards:
            if hasattr(hazard, "intensity_at"):
                intensity += float(hazard.intensity_at(pos))
            else:
                radius = max(float(hazard.radius), 0.0)
                dx = float(point[0]) - float(hazard.pos[0])
                dy = float(point[1]) - float(hazard.pos[1])
                distance = float((dx * dx + dy * dy) ** 0.5)
                if radius <= 1e-6:
                    if distance <= 0.75:
                        intensity += float(hazard.severity)
                elif distance <= radius:
                    intensity += float(hazard.severity) * max(
                        0.0, 1.0 - (distance / radius)
                    )
        return intensity

    def visibility_at(self, point) -> float:
        """Get visibility factor at a point considering all hazards."""
        pos = np.array(point, dtype=float)
        vis = 1.0
        for hazard in self.hazards:
            if hasattr(hazard, "visibility_at"):
                vis *= hazard.visibility_at(pos)
        return vis

    def shooter_pressure_for(self, agent) -> dict[str, Any] | None:
        hostiles = [
            hostile
            for hostile in self._active_agents()
            if getattr(hostile, "is_hostile", False)
        ]
        visible = []
        for hostile in hostiles:
            hostile_point = self._agent_sight_point(hostile)
            agent_point = self._agent_sight_point(agent)
            distance = float(np.linalg.norm(hostile_point - agent_point))
            range_m = float(getattr(hostile, "range_m", 8.0))
            if line_of_sight(
                self.layout, hostile_point, agent_point, max_range=range_m
            ):
                visible.append(
                    {"hostile": hostile, "distance": distance, "range_m": range_m}
                )
        if not visible:
            return None
        return min(visible, key=lambda item: item["distance"])

    def hazard_penalty_at_cell(self, cell: tuple[int, int]) -> float:
        if any(
            self._hazard_requires_direct_sampling(hazard) for hazard in self.hazards
        ):
            intensity = self._cell_hazard_intensity(cell, height_offset=1.5)
        else:
            intensity = self.hazard_intensity_at(self.layout.world_position(cell))
        terrain_damage = self.terrain_damage_cells.get(
            tuple(self.layout.cell(cell)), 0.0
        )
        return self.config.hazard_avoidance_weight * (intensity + terrain_damage)

    def _cell_hazard_intensity(self, cell, *, height_offset: float) -> float:
        floor_point = self.layout.world_position(cell)
        exposure_point = self.layout.world_position(cell, height_offset=height_offset)
        intensity = 0.0
        for hazard in self.hazards:
            sample = (
                exposure_point
                if self._hazard_requires_direct_sampling(hazard)
                else floor_point
            )
            intensity += float(hazard.intensity_at(sample))
        return intensity

    def _agent_exposure_point(self, agent) -> np.ndarray:
        cell = self._grid_cell(agent)
        return self.layout.world_position(
            cell, height_offset=float(getattr(agent, "breathing_height_m", 1.5))
        )

    def _agent_hazard_intensity(self, agent) -> float:
        exposure_point = self._agent_exposure_point(agent)
        floor_point = np.array(agent.pos, dtype=float)
        intensity = 0.0
        for hazard in self.hazards:
            sample = (
                exposure_point
                if hasattr(hazard, "intensity_grid")
                or getattr(hazard, "height_aware", False)
                else floor_point
            )
            intensity += float(hazard.intensity_at(sample))
        return intensity

    def _agent_sight_point(self, agent) -> np.ndarray:
        cell = self._grid_cell(agent)
        eye_height = float(
            getattr(agent, "eye_height_m", getattr(agent, "breathing_height_m", 1.5))
        )
        return self.layout.world_position(cell, height_offset=eye_height)

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
            if active_agents
            else np.zeros((0, 3), dtype=float)
        )
        hazard_positions = (
            np.array([_point3(h.pos) for h in self.hazards], dtype=float)
            if self.hazards
            else np.zeros((0, 3), dtype=float)
        )
        radii = (
            np.array([float(h.radius) for h in self.hazards], dtype=float)
            if self.hazards
            else np.zeros((0,), dtype=float)
        )
        severities = (
            np.array([float(h.severity) for h in self.hazards], dtype=float)
            if self.hazards
            else np.zeros((0,), dtype=float)
        )
        hazard_loads = self.acceleration.hazard_intensities(
            positions, hazard_positions, radii, severities
        )
        if any(
            self._hazard_requires_direct_sampling(hazard) for hazard in self.hazards
        ):
            hazard_loads = np.array(
                [self._agent_hazard_intensity(agent) for agent in active_agents],
                dtype=float,
            )

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
                agent.local_density = self.spatial_index.local_density(
                    agent.pos, radius=1.5
                )
            agent.crowd_speed_factor = max(
                self.config.min_crowd_speed_factor,
                1.0 - (self.config.density_slowdown_scale * agent.local_density / 2.0),
            )
            agent.current_hazard_load = float(hazard_loads[active_lookup[agent.id]])
            # ITED: use physiology model if available
            if hasattr(agent, "update_physiology"):
                agent.update_physiology(agent.current_hazard_load, self.config.dt)
            else:
                agent.hazard_speed_factor = max(
                    0.45, 1.0 - (agent.current_hazard_load * 0.35)
                )

    def _step_information(self) -> None:
        """ITED: information propagation step."""
        if self.config.information_mode == "perfect":
            return  # skip — all agents have perfect info

        active = self._active_agents()
        dt = self.config.dt
        exit_positions = [tuple(e.pos) for e in self.exits]
        observation_batch = []

        for agent in active:
            if not hasattr(agent, "beliefs"):
                continue

            # direct observation
            vis = self.visibility_at(agent.pos)
            effective_vision = (
                getattr(agent, "vision_radius", self.config.observation_radius) * vis
            )
            if padm_stage_enabled(self.padm_stage_config, PADM_RECEIVE):
                evaluate_pending_provenance(agent, self, effective_vision)
            observation_batch.append(
                (
                    agent,
                    tuple(float(value) for value in agent.pos),
                    effective_vision,
                )
            )

        self.info_field.padm_receive(
            observation_batch,
            exit_positions,
            self.hazards,
            self.current_step,
            stage_config=self.padm_stage_config,
        )

        self.info_field.padm_understand(
            observation_batch,
            dt,
            stage_config=self.padm_stage_config,
        )

        self._padm_personalize(observation_batch)
        self._padm_decide(observation_batch)

        # agent-to-agent gossip
        if (
            self.spatial_index is not None
            and padm_stage_enabled(self.padm_stage_config, PADM_RECEIVE)
        ):
            for agent in active:
                if not hasattr(agent, "beliefs"):
                    continue
                neighbors = self.spatial_index.neighbor_agents(
                    agent.pos,
                    radius=self.config.gossip_radius,
                )
                for other in neighbors:
                    if not hasattr(other, "beliefs"):
                        continue
                    dist = float(np.linalg.norm(agent.pos - np.array(other.pos)))
                    source_id = f"agent:{agent.id}"
                    sender_credibility = (
                        other.credibility_for_source(source_id)
                        if hasattr(other, "credibility_for_source")
                        else getattr(agent, "credibility", 0.5)
                    )
                    receiver_rationality = (
                        other.rationality_for_source(source_id)
                        if hasattr(other, "rationality_for_source")
                        else getattr(other, "rationality", 0.8)
                    )
                    transferred = self.gossip_model.exchange(
                        sender_beliefs=agent.beliefs,
                        receiver_beliefs=other.beliefs,
                        sender_credibility=sender_credibility,
                        receiver_rationality=receiver_rationality,
                        sender_state=agent.state,
                        distance=dist,
                    )
                    if transferred and (self.current_step % 10 == 0):
                        if hasattr(other, "belief_revision"):
                            claimed_exit = agent.beliefs.best_exit()
                            if claimed_exit is not None:
                                other.belief_revision.record_claim(
                                    source_id=source_id,
                                    timestamp_s=self.time_s,
                                    step=self.current_step,
                                    channel_type="gossip",
                                    objective="peer-transfer",
                                    claimed_exit=claimed_exit,
                                )
                        self.gossip_events.append(
                            {
                                "step": self.current_step,
                                "time_s": self.time_s,
                                "sender_id": agent.id,
                                "receiver_id": other.id,
                                "distance": dist,
                            }
                        )

    def _padm_personalize(
        self, observation_batch: list[tuple[Any, tuple[float, ...], float]]
    ) -> None:
        if not padm_stage_enabled(self.padm_stage_config, PADM_PERSONALIZE):
            return
        for agent, _agent_pos, _effective_vision in observation_batch:
            record_padm_stage(agent, PADM_PERSONALIZE)
            if hasattr(agent, "padm_personalize"):
                agent.padm_personalize(self)

    def _padm_decide(
        self, observation_batch: list[tuple[Any, tuple[float, ...], float]]
    ) -> None:
        if not padm_stage_enabled(self.padm_stage_config, PADM_DECIDE):
            return
        for agent, _agent_pos, _effective_vision in observation_batch:
            record_padm_stage(agent, PADM_DECIDE)
            if hasattr(agent, "padm_decide"):
                agent.padm_decide(self)
            elif hasattr(agent, "update_intention"):
                agent.update_intention(self)

    def _empty_bottleneck_metrics(self) -> dict[str, BottleneckStepTelemetry]:
        return {
            zone.zone_id: BottleneckStepTelemetry() for zone in self.bottleneck_zones
        }

    def _seed_bottleneck_membership(self) -> None:
        self.agent_zone_membership = {}
        self.agent_zone_entry_step = {}
        for agent in self.agents:
            if agent.has_evacuated or not agent.is_released(self):
                self.agent_zone_membership[agent.id] = None
                continue
            zone_id = self.bottleneck_lookup.get(self._grid_cell(agent))
            self.agent_zone_membership[agent.id] = zone_id
            if zone_id is not None:
                self.agent_zone_entry_step[(agent.id, zone_id)] = self.current_step

    def _update_path_usage(self) -> None:
        for agent in self._active_agents():
            floor_id, x, y = self._grid_cell(agent)
            self.path_usage_by_floor[floor_id][y, x] += 1
        self.path_usage_grid = self.path_usage_by_floor[self.layout.primary_floor_id]

    def _build_step_grids(self):
        active = self._active_agents()
        floor_grids = {}
        for floor_id, floor in self.layout.floors.items():
            shape = floor.grid.shape
            floor_grids[floor_id] = {
                "occupancy_grid": np.zeros(shape, dtype=int),
                "density_grid": np.zeros(shape, dtype=float),
                "speed_grid": np.zeros(shape, dtype=float),
                "speed_hits": np.zeros(shape, dtype=float),
            }
        for agent in active:
            floor_id, x, y = self._grid_cell(agent)
            grids = floor_grids[floor_id]
            grids["occupancy_grid"][y, x] += 1
            grids["density_grid"][y, x] += float(agent.local_density)
            grids["speed_grid"][y, x] += float(agent.current_speed)
            grids["speed_hits"][y, x] += 1.0
        for grids in floor_grids.values():
            hits = grids.pop("speed_hits")
            occupancy = grids["occupancy_grid"]
            grids["density_grid"] = np.divide(
                grids["density_grid"],
                np.maximum(occupancy, 1),
                out=np.zeros_like(grids["density_grid"]),
                where=occupancy > 0,
            )
            grids["speed_grid"] = np.divide(
                grids["speed_grid"],
                hits,
                out=np.zeros_like(grids["speed_grid"]),
                where=hits > 0,
            )
        primary = floor_grids[self.layout.primary_floor_id]
        return (
            primary["occupancy_grid"],
            primary["density_grid"],
            primary["speed_grid"],
            floor_grids,
        )

    def _capture_step_telemetry(self, *, exit_flow_step, bottleneck_metrics) -> None:
        self._update_path_usage()
        occupancy_grid, density_grid, speed_grid, floor_grids = self._build_step_grids()
        living = self._active_agents()
        mean_speed = (
            float(np.mean([a.current_speed for a in living])) if living else 0.0
        )
        mean_density = (
            float(np.mean([a.local_density for a in living])) if living else 0.0
        )
        self.density_history.append(mean_density)

        # ITED: entropy metrics
        all_beliefs = [a.beliefs for a in living if hasattr(a, "beliefs")]
        total_exits = len(self.exits)
        total_hazards = len(self.hazards)
        h_global = (
            global_entropy(all_beliefs, total_exits, total_hazards)
            if all_beliefs
            else 0.0
        )
        self.entropy_history.append(h_global)
        connector_telemetry = self._connector_telemetry()

        agents_tel = []
        for agent in living:
            h_agent = 0.0
            acc = 1.0
            imp = 0.0
            intention = "EVACUATE"
            if hasattr(agent, "beliefs"):
                h_agent = agent_entropy(agent.beliefs, total_exits, total_hazards)
                true_exits = [tuple(e.pos) for e in self.exits]
                acc = belief_accuracy(agent.beliefs, true_exits, self.hazards)
            if hasattr(agent, "physiology"):
                imp = agent.physiology.impairment_level
            if hasattr(agent, "intention"):
                intention = agent.intention
            padm_counts = padm_counter_values(agent)

            agents_tel.append(
                AgentStepTelemetry(
                    agent_id=agent.id,
                    position=tuple(float(value) for value in agent.pos),
                    cell=self._grid_cell(agent),
                    state=agent.state,
                    speed=float(agent.current_speed),
                    local_density=float(agent.local_density),
                    target_exit=(
                        tuple(agent.target_exit)
                        if agent.target_exit is not None
                        else None
                    ),
                    cohort_name=str(agent.cohort_name),
                    group_id=agent.group_id,
                    leader_id=agent.leader_id,
                    family_id=getattr(agent, "family_id", None),
                    role_in_group=getattr(agent, "role_in_group", "solo"),
                    mobility_class=getattr(agent, "mobility_class", "standard"),
                    evacuation_mode=getattr(agent, "evacuation_mode", "pedestrian"),
                    hazard_exposure=float(agent.hazard_exposure),
                    hazard_load=float(agent.current_hazard_load),
                    trail=tuple(self.agent_traces.get(agent.id, [])[-8:]),
                    entropy=h_agent,
                    belief_accuracy=acc,
                    impairment=imp,
                    decision_mode=intention,
                    padm_receive=padm_counts["padm_receive"],
                    padm_understand=padm_counts["padm_understand"],
                    padm_personalize=padm_counts["padm_personalize"],
                    padm_decide=padm_counts["padm_decide"],
                )
            )

        self.step_history.append(
            StepTelemetry(
                step=self.current_step,
                time_s=self.time_s,
                occupancy_grid=occupancy_grid,
                density_grid=density_grid,
                speed_grid=speed_grid,
                path_usage_grid=self.path_usage_grid.copy(),
                floor_grids={
                    floor_id: {
                        "occupancy_grid": grids["occupancy_grid"].copy(),
                        "density_grid": grids["density_grid"].copy(),
                        "speed_grid": grids["speed_grid"].copy(),
                        "path_usage_grid": self.path_usage_by_floor[floor_id].copy(),
                    }
                    for floor_id, grids in floor_grids.items()
                },
                agents=agents_tel,
                exit_flow_cumulative=dict(self.exit_flow_cumulative),
                exit_flow_step=dict(exit_flow_step),
                bottlenecks={
                    zid: BottleneckStepTelemetry(
                        occupancy=m.occupancy,
                        inflow=m.inflow,
                        outflow=m.outflow,
                        queue_length=m.queue_length,
                        mean_dwell_s=m.mean_dwell_s,
                        mean_speed=m.mean_speed,
                        mean_density=m.mean_density,
                    )
                    for zid, m in bottleneck_metrics.items()
                },
                hazards=[h.snapshot() for h in self.hazards],
                evacuated_total=len(self.completed_agents),
                remaining=len(living),
                pending_release=len(self._pending_agents()),
                mean_speed=mean_speed,
                mean_density=mean_density,
                global_entropy=h_global,
                connector_flow={
                    key: float(value["flow_step"])
                    for key, value in connector_telemetry.items()
                },
                connector_capacity={
                    key: int(value["capacity"])
                    for key, value in connector_telemetry.items()
                },
                connector_queue_length={
                    key: int(value["queue_length"])
                    for key, value in connector_telemetry.items()
                },
                connector_capacity_used={
                    key: int(value["capacity_used"])
                    for key, value in connector_telemetry.items()
                },
            )
        )

    def _update_bottleneck_metrics(self) -> dict[str, BottleneckStepTelemetry]:
        metrics = self._empty_bottleneck_metrics()
        active = self._active_agents()
        active_ids = {a.id for a in active}
        current_membership: dict[int, str | None] = {}
        for agent in self.agents:
            zone_id = None
            if agent.id in active_ids:
                zone_id = self.bottleneck_lookup.get(self._grid_cell(agent))
            current_membership[agent.id] = zone_id
            prev = self.agent_zone_membership.get(agent.id)
            if prev == zone_id:
                continue
            if prev is not None:
                metrics[prev].outflow += 1
                enter = self.agent_zone_entry_step.pop(
                    (agent.id, prev), self.current_step
                )
                dwell = max(
                    (self.current_step - enter) * self.config.dt, self.config.dt
                )
                self.bottleneck_dwell_samples[prev].append(dwell)
            if zone_id is not None:
                metrics[zone_id].inflow += 1
                self.agent_zone_entry_step[(agent.id, zone_id)] = self.current_step
            self.agent_zone_membership[agent.id] = zone_id
        for zone in self.bottleneck_zones:
            za = [a for a in active if current_membership.get(a.id) == zone.zone_id]
            metrics[zone.zone_id].occupancy = len(za)
            metrics[zone.zone_id].mean_speed = (
                float(np.mean([a.current_speed for a in za])) if za else 0.0
            )
            metrics[zone.zone_id].mean_density = (
                float(np.mean([a.local_density for a in za])) if za else 0.0
            )
            samples = self.bottleneck_dwell_samples[zone.zone_id]
            metrics[zone.zone_id].mean_dwell_s = (
                float(np.mean(samples)) if samples else 0.0
            )
            metrics[zone.zone_id].queue_length = sum(
                1
                for a in active
                if current_membership.get(a.id) != zone.zone_id
                and zone_distance(self._grid_cell(a), zone) <= 2
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
        from chiyoda._logging import log_event

        self._ensure_bootstrapped()
        dt = self.config.dt
        step_start = int(self.current_step)
        log_event(
            None,
            "simulation.step.start",
            step=step_start,
            time_s=float(self.time_s),
            active_agents=len(self._active_agents()),
            completed_agents=len(self.completed_agents),
        )

        # evolve hazards
        for hz in self.hazards:
            hz.step(dt, self)

        self._update_spatial_index()
        self._refresh_agent_context()

        # ITED: information propagation
        self._step_information()
        for channel in self.hostile_channels:
            event = channel.execute(self)
            if event is not None:
                self.hostile_channel_events.append(event)
        if self.agent_decision_policy is not None:
            self.agent_decision_events.extend(self.agent_decision_policy.execute(self))
        if self.intervention_policy is not None:
            self.intervention_events.extend(self.intervention_policy.execute(self))

        exit_flow_step = {l: 0 for l in self.exit_flow_cumulative}

        if self.navigator is not None and hasattr(self.navigator, "clear_cache"):
            self.navigator.clear_cache()

        self._process_connector_queues()
        for agent in list(self._active_agents()):
            self._maybe_switch_evacuation_mode(agent)
            if self.behavior_model is not None:
                self.behavior_model.update_agent(agent, self)
            if self.navigator is not None:
                agent.update_navigation(self.navigator, self)

            self._prev_positions[agent.id] = np.array(agent.pos, copy=True)
            if self._agent_in_connector_queue(agent):
                agent.current_speed = 0.0
                agent.travel_time_s = self.time_s + dt
                continue
            if self._maybe_enqueue_connector_transfer(agent):
                agent.current_speed = 0.0
                agent.travel_time_s = self.time_s + dt
                continue
            previous_cell = self._grid_cell(agent)
            previous_floor = getattr(agent, "floor_id", previous_cell[0])
            agent.step(dt, self)
            agent.floor_id = self.layout.floor_for_z(float(agent.pos[2]))
            agent.current_speed = float(
                np.linalg.norm(agent.pos - self._prev_positions[agent.id]) / dt
            )
            agent.travel_time_s = self.time_s + dt
            self._record_floor_jump_if_impossible(agent, previous_cell, previous_floor)

            # hazard exposure (already handled by physiology for cognitive agents)
            if not hasattr(agent, "physiology"):
                agent.current_hazard_load = self.hazard_intensity_at(agent.pos)
                agent.hazard_exposure += agent.current_hazard_load * dt
                agent.hazard_risk = max(
                    float(agent.hazard_risk), float(agent.current_hazard_load)
                )

            if agent.current_hazard_load > 0.05:
                self.risk_events.append(
                    {
                        "step": self.current_step,
                        "time_s": self.time_s,
                        "agent_id": agent.id,
                        "hazard_load": float(agent.current_hazard_load),
                    }
                )

            self.agent_traces.setdefault(agent.id, []).append(
                tuple(float(value) for value in agent.pos)
            )

            if self.layout.is_exit(
                agent.pos, floor_id=getattr(agent, "floor_id", None)
            ):
                # responders don't evacuate
                if getattr(agent, "is_responder", False) or getattr(
                    agent, "is_hostile", False
                ):
                    continue
                agent.has_evacuated = True
                self.completed_agents.append(agent)
                self.evacuated_at_step.append(self.current_step + 1)
                self.travel_times_s.append(agent.travel_time_s)
                exit_pos = tuple(agent.target_exit or self._grid_cell(agent))
                label = self.exit_labels.get(exit_pos)
                if label is None:
                    for known_exit, known_label in self.exit_labels.items():
                        if self._grid_cell(agent) == known_exit:
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
            ml.record(
                self.current_step,
                self.time_s,
                dt,
                self._active_agents(),
                self._prev_positions,
            )
        self._capture_step_telemetry(
            exit_flow_step=exit_flow_step, bottleneck_metrics=bn_metrics
        )
        log_event(
            None,
            "simulation.step.end",
            step=int(self.current_step),
            time_s=float(self.time_s),
            active_agents=len(self._active_agents()),
            completed_agents=len(self.completed_agents),
            risk_events=len(self.risk_events),
            intervention_events=len(self.intervention_events),
            hostile_channel_events=len(self.hostile_channel_events),
        )

    def _process_connector_queues(self) -> None:
        for queue in self.connector_queues.values():
            queue.reset_step()
            for event in queue.finish_ready(time_s=self.time_s):
                self._apply_connector_finish(event)
                self._record_connector_event(event)
            density = self._connector_density(queue.connector)
            for event in queue.step(
                time_s=self.time_s, dt=self.config.dt, density=density
            ):
                self._record_connector_event(event)

    def _agent_in_connector_queue(self, agent) -> bool:
        return any(
            queue.has_agent(agent.id) for queue in self.connector_queues.values()
        )

    def _maybe_enqueue_connector_transfer(self, agent) -> bool:
        edge = self._next_connector_edge(agent)
        if edge is None:
            return False
        connector, source, target = edge
        queue = self.connector_queues.get(connector.id)
        if queue is None:
            return False
        queue.enqueue(
            agent.id, source, target, priority=self._connector_priority(agent)
        )
        density = self._connector_density(connector)
        for event in queue.step(time_s=self.time_s, dt=0.0, density=density):
            self._record_connector_event(event)
        return True

    def _next_connector_edge(self, agent):
        if not agent.current_path or agent.path_index >= len(agent.current_path):
            return None
        source = self._grid_cell(agent)
        while agent.path_index < len(agent.current_path):
            target = self.layout.cell(agent.current_path[agent.path_index])
            if target != source:
                break
            agent.path_index += 1
        if agent.path_index >= len(agent.current_path):
            return None
        target = self.layout.cell(agent.current_path[agent.path_index])
        connector = self.layout.connector_for_edge(source, target)
        if connector is None:
            return None
        return connector, source, target

    def _apply_connector_finish(self, event: ConnectorQueueEvent) -> None:
        agent = self.agent_lookup.get(event.agent_id)
        if agent is None or agent.has_evacuated:
            return
        agent.pos = self.layout.world_position(event.target)
        agent.floor_id = event.target[0]
        if agent.current_path and agent.path_index < len(agent.current_path):
            if self.layout.cell(agent.current_path[agent.path_index]) == event.target:
                agent.path_index += 1
        agent.current_speed = 0.0
        agent.travel_time_s = self.time_s
        self.agent_traces.setdefault(agent.id, []).append(
            tuple(float(value) for value in agent.pos)
        )

    def _record_connector_event(self, event: ConnectorQueueEvent) -> None:
        if event.phase == "start":
            self.connector_usage_cumulative[event.connector_id] = (
                self.connector_usage_cumulative.get(event.connector_id, 0) + 1
            )
        self.connector_events.append(
            {
                "step": int(self.current_step),
                "time_s": float(self.time_s),
                "agent_id": int(event.agent_id),
                "connector_id": event.connector_id,
                "connector_type": event.connector_type,
                "phase": event.phase,
                "source": list(event.source),
                "target": list(event.target),
                "queue_length": int(event.queue_length),
                "flow_rate": float(event.flow_rate),
                "capacity_used": int(event.capacity_used),
            }
        )

    def _connector_density(self, connector) -> float:
        if self.spatial_index is None:
            return 0.0
        source = self.layout.world_position(connector.from_cell)
        target = self.layout.world_position(connector.to_cell)
        return max(
            float(self.spatial_index.local_density(source, radius=1.5)),
            float(self.spatial_index.local_density(target, radius=1.5)),
        )

    def _connector_priority(self, agent) -> float:
        return (
            float(getattr(agent, "current_hazard_load", 0.0))
            + float(getattr(agent, "hazard_exposure", 0.0)) * 0.1
        )

    def _maybe_switch_evacuation_mode(self, agent) -> None:
        if getattr(agent, "evacuation_mode", "pedestrian") != "pedestrian":
            return
        segment = self.road_segment_cells.get(self._grid_cell(agent))
        if segment is None:
            return
        mode = str(segment.get("mode_switch", "vehicle"))
        if mode not in {"vehicle", "vehicular"}:
            return
        multiplier = float(segment.get("speed_multiplier", 3.0))
        agent.evacuation_mode = "vehicle"
        agent.mode_switch_step = int(self.current_step)
        agent.base_speed *= multiplier
        self.mode_switch_events.append(
            {
                "step": int(self.current_step),
                "time_s": float(self.time_s),
                "agent_id": int(agent.id),
                "segment_id": str(segment.get("id", "")),
                "mode": "vehicle",
                "speed_multiplier": multiplier,
            }
        )

    def apply_terrain_damage(
        self, center, radius: float, severity: float, *, source: str
    ) -> dict[str, Any]:
        origin = _point3(center)
        radius = max(float(radius), 1e-6)
        severity = max(0.0, float(severity))
        affected = 0
        max_damage = 0.0
        for cell in self.layout.all_walkable_cells():
            point = self.layout.world_position(cell)
            dist = float(np.linalg.norm(point - origin))
            if dist > radius:
                continue
            increment = severity * max(0.0, 1.0 - dist / radius)
            if increment <= 0.0:
                continue
            key = tuple(cell)
            value = min(1.0, self.terrain_damage_cells.get(key, 0.0) + increment)
            self.terrain_damage_cells[key] = value
            affected += 1
            max_damage = max(max_damage, value)
        return {
            "source": source,
            "affected_cells": affected,
            "max_damage": max_damage,
        }

    def trigger_re_evacuation_wave(self, center, radius: float, *, source: str) -> int:
        origin = _point3(center)
        radius = max(float(radius), 1e-6)
        triggered = 0
        for agent in self.agents:
            if agent.has_evacuated:
                continue
            dist = float(np.linalg.norm(_point3(agent.pos) - origin))
            if dist > radius:
                continue
            agent.release_step = min(
                int(getattr(agent, "release_step", 0)), int(self.current_step)
            )
            agent.current_path = []
            agent.path_index = 0
            agent.target_exit = None
            agent.last_navigation_step = -9999
            agent.re_evacuation_count = (
                int(getattr(agent, "re_evacuation_count", 0)) + 1
            )
            agent.re_evacuation_step = int(self.current_step)
            if getattr(agent, "state", "CALM") == "CALM":
                agent.state = "ALERT"
            triggered += 1
        return triggered

    def _hazard_requires_direct_sampling(self, hazard) -> bool:
        kind = str(getattr(hazard, "kind", "")).upper()
        return (
            hasattr(hazard, "intensity_grid")
            or getattr(hazard, "height_aware", False)
            or kind in {"WILDFIRE", "EMBER", "FLOOD", "EARTHQUAKE", "AFTERSHOCK"}
        )

    def _connector_telemetry(self) -> dict[str, dict[str, float | int]]:
        return {
            connector_id: queue.telemetry()
            for connector_id, queue in self.connector_queues.items()
        }

    def _record_floor_jump_if_impossible(
        self, agent, previous_cell, previous_floor
    ) -> None:
        current_cell = self._grid_cell(agent)
        current_floor = getattr(agent, "floor_id", current_cell[0])
        if str(previous_floor) == str(current_floor):
            return
        if self._agent_in_connector_queue(agent):
            return
        if self.layout.connector_for_edge(previous_cell, current_cell) is not None:
            return
        self.impossible_floor_jumps.append(
            {
                "step": int(self.current_step),
                "time_s": float(self.time_s),
                "agent_id": int(agent.id),
                "from": list(previous_cell),
                "to": list(current_cell),
            }
        )

    def run(self, visualize: bool = False, visualizer=None) -> None:
        from chiyoda._logging import log_event

        self._ensure_bootstrapped()
        log_event(
            None,
            "simulation.run.start",
            max_steps=self.config.max_steps,
            agent_count=len(self.agents),
        )
        for _ in range(self.config.max_steps):
            if all(
                a.has_evacuated
                or getattr(a, "is_responder", False)
                or getattr(a, "is_hostile", False)
                for a in self.agents
            ):
                break
            self.step()
            if visualize and visualizer is not None:
                visualizer.on_step(self)
        log_event(
            None,
            "simulation.run.end",
            steps=int(getattr(self, "current_step", 0)),
            evacuated=int(
                sum(1 for a in self.agents if getattr(a, "has_evacuated", False))
            ),
        )

    def live_state(self) -> dict[str, Any]:
        self._ensure_bootstrapped()
        latest = self.step_history[-1]
        positions = (
            np.array([a.position for a in latest.agents], dtype=float)
            if latest.agents
            else np.zeros((0, 3), dtype=float)
        )
        return {
            "step": latest.step,
            "time_s": latest.time_s,
            "positions": positions,
            "agent_ids": [a.agent_id for a in latest.agents],
            "states": [a.state for a in latest.agents],
            "speeds": np.array([a.speed for a in latest.agents], dtype=float),
            "local_densities": np.array(
                [a.local_density for a in latest.agents], dtype=float
            ),
            "target_exits": [a.target_exit for a in latest.agents],
            "agents": [
                {
                    "id": a.agent_id,
                    "position": a.position,
                    "cell": a.cell,
                    "state": a.state,
                    "speed": a.speed,
                    "local_density": a.local_density,
                    "target_exit": a.target_exit,
                    "cohort_name": a.cohort_name,
                    "group_id": a.group_id,
                    "leader_id": a.leader_id,
                    "family_id": a.family_id,
                    "role_in_group": a.role_in_group,
                    "mobility_class": a.mobility_class,
                    "hazard_exposure": a.hazard_exposure,
                    "hazard_load": a.hazard_load,
                    "trail": list(a.trail),
                    "entropy": a.entropy,
                    "belief_accuracy": a.belief_accuracy,
                    "impairment": a.impairment,
                    "decision_mode": a.decision_mode,
                    "padm_receive": a.padm_receive,
                    "padm_understand": a.padm_understand,
                    "padm_personalize": a.padm_personalize,
                    "padm_decide": a.padm_decide,
                }
                for a in latest.agents
            ],
            "exits": [e.pos for e in self.exits],
            "exit_labels": dict(self.exit_labels),
            "exit_flow_cumulative": dict(latest.exit_flow_cumulative),
            "exit_flow_step": dict(latest.exit_flow_step),
            "hazards": list(latest.hazards),
            "density": latest.mean_density,
            "mean_speed": latest.mean_speed,
            "global_entropy": latest.global_entropy,
            "occupancy_grid": latest.occupancy_grid.copy(),
            "density_grid": latest.density_grid.copy(),
            "speed_grid": latest.speed_grid.copy(),
            "path_usage_grid": latest.path_usage_grid.copy(),
            "bottlenecks": [
                {
                    "id": z.zone_id,
                    "cells": list(z.cells),
                    "orientation": z.orientation,
                    "centroid": z.centroid,
                    "metrics": latest.bottlenecks[z.zone_id],
                }
                for z in self.bottleneck_zones
            ],
            "evacuated": latest.evacuated_total,
            "remaining": latest.remaining,
            "pending_release": latest.pending_release,
            "intervention_events": list(self.intervention_events),
            "agent_decision_events": list(self.agent_decision_events),
            "acceleration_backend": self.acceleration.name,
            "requested_acceleration_backend": self.acceleration.requested_backend,
        }


def _point3(value) -> np.ndarray:
    if len(value) >= 3:
        return np.array(
            [float(value[0]), float(value[1]), float(value[2])], dtype=float
        )
    return np.array([float(value[0]), float(value[1]), 0.0], dtype=float)
