"""
Scenario manager — wires up ITED information layer, responders, and multi-hazard system.
"""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import random
from typing import Any, Dict, List, Optional
import numpy as np
import yaml
from chiyoda.environment.layout import EMPTY, EXIT, Layout, WALL, RESPONDER_ENTRY
from chiyoda.environment.obstacles import apply_obstacles_to_grid, obstacles_from_config
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard, ImportedHazardField
from chiyoda.environment.station_provenance import load_station_provenance
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.agents.commuter import Commuter
from chiyoda.agents.hostile import HostileAgent
from chiyoda.agents.responder import FirstResponder
from chiyoda.agents.behaviors import BehaviorModel, BehaviorConfig
from chiyoda.information.decisions import create_agent_decision_policy
from chiyoda.information.interventions import create_intervention_policy
from chiyoda.information.warfare import create_hostile_channels
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.navigation.spatial_index import SpatialIndex
from chiyoda.scenarios.generated_calibration import apply_generated_population_calibration


MOBILITY_CLASS_DEFAULTS = {
    "standard": {"speed_multiplier": 1.0, "vision_multiplier": 1.0, "breathing_height_m": 1.5},
    "wheelchair": {"speed_multiplier": 0.55, "vision_multiplier": 1.0, "breathing_height_m": 1.1},
    "walker": {"speed_multiplier": 0.7, "vision_multiplier": 0.95, "breathing_height_m": 1.35},
    "visual-impairment": {"speed_multiplier": 0.85, "vision_multiplier": 0.45, "breathing_height_m": 1.5},
    "visual_impairment": {"speed_multiplier": 0.85, "vision_multiplier": 0.45, "breathing_height_m": 1.5},
}


@dataclass
class Scenario:
    name: str
    layout_file: str
    population_total: int


class ScenarioManager:
    def load_config(self, scenario_file: str) -> Dict[str, Any]:
        path = Path(scenario_file).resolve()
        with path.open("r") as handle:
            cfg = yaml.safe_load(handle)
        scenario = cfg.get("scenario", cfg)
        scenario["_source_file"] = str(path)
        metadata = scenario.get("metadata", {}) or {}
        if metadata:
            provenance = load_station_provenance(metadata, source_file=str(path))
            if provenance is not None:
                scenario.setdefault("metadata", {})
                scenario["metadata"]["station_provenance"] = provenance
        return scenario

    def load_scenario(self, scenario_file: str, *, overrides=None, random_seed=None) -> Simulation:
        scenario = self.load_config(scenario_file)
        if overrides:
            scenario = self._deep_merge(scenario, overrides)
        if random_seed is not None:
            scenario.setdefault("simulation", {})
            scenario["simulation"]["random_seed"] = random_seed
        return self.build_simulation(scenario)

    def build_simulation(self, scenario: Dict[str, Any]) -> Simulation:
        sc = apply_generated_population_calibration(scenario)
        simulation_cfg = sc.get("simulation", {})
        random_seed = simulation_cfg.get("random_seed", 42)
        if random_seed is not None:
            random.seed(random_seed)
            np.random.seed(random_seed)

        layout = self._build_layout(sc)
        exits = [Exit(pos=tuple(p)) for p in layout.exit_positions()]
        hazards = self._build_hazards(
            sc.get("hazards", []) or [],
            source_file=sc.get("_source_file"),
        )
        agents = self._build_agents(layout, sc.get("population", {}) or {})

        # build responders if specified
        responders = self._build_responders(layout, sc.get("responders", []) or [], len(agents))
        agents.extend(responders)
        hostiles = self._build_hostile_agents(layout, sc.get("hostile_agents", []) or [], len(agents))
        agents.extend(hostiles)

        # ITED config
        info_cfg = sc.get("information", {}) or {}
        sim_cfg = SimulationConfig(
            max_steps=int(simulation_cfg.get("max_steps", 500)),
            dt=float(simulation_cfg.get("dt", 0.1)),
            random_seed=random_seed,
            hazard_avoidance_weight=float(simulation_cfg.get("hazard_avoidance_weight", 1.25)),
            acceleration_backend=str(simulation_cfg.get("acceleration_backend", "auto")),
            density_slowdown_scale=float(simulation_cfg.get("density_slowdown_scale", 1.0)),
            min_crowd_speed_factor=float(simulation_cfg.get("min_crowd_speed_factor", 0.25)),
            information_mode=str(info_cfg.get("mode", "asymmetric")),
            info_decay_rate=float(info_cfg.get("decay_rate", 0.01)),
            observation_radius=float(info_cfg.get("observation_radius", 5.0)),
            gossip_radius=float(info_cfg.get("gossip_radius", 2.0)),
            beacon_radius=float(info_cfg.get("beacon_radius", 8.0)),
        )
        sim = Simulation(layout=layout, agents=agents, exits=exits, hazards=hazards, config=sim_cfg)
        population_audit = (sc.get("metadata", {}) or {}).get("generated_population_calibration_audit")
        if population_audit:
            sim.llm_call_audit.append({
                "step": None,
                "time_s": 0.0,
                "surface": "population_calibration",
                "policy": "generated_population_calibration",
                "agent_id": None,
                "provider": population_audit.get("provider"),
                "model": population_audit.get("model"),
                "cache_key": population_audit.get("cache_key"),
                "cache_status": population_audit.get("cache_status"),
                "validation_status": population_audit.get("validation_status"),
                "validation_reasons": ";".join(population_audit.get("validation_reasons", [])),
                "used_fallback": population_audit.get("validation_status") != "accepted",
                "objective": (sc.get("generated_population_calibration", {}) or {}).get("objective", ""),
                "prompt_style": (sc.get("generated_population_calibration", {}) or {}).get("prompt_style", ""),
                "target_x": None,
                "target_y": None,
                "estimated_input_tokens": population_audit.get("estimated_input_tokens", 0),
                "estimated_output_tokens": population_audit.get("estimated_output_tokens", 0),
                "estimated_total_tokens": population_audit.get("estimated_total_tokens", 0),
                "estimated_usd": population_audit.get("estimated_usd", 0.0),
                "budget_reason": population_audit.get("budget_reason", ""),
                "raw_input_tokens": population_audit.get("raw_input_tokens", 0),
                "raw_output_tokens": population_audit.get("raw_output_tokens", 0),
                "raw_total_tokens": population_audit.get("raw_total_tokens", 0),
            })
        sim.attach_wui_egress(self._build_wui_egress(sc, layout))
        sim.destination_profiles = self._build_destination_profiles(sc, layout)

        spatial = SpatialIndex()
        sim.attach_spatial_index(spatial)
        navigator = SmartNavigator(
            layout,
            density_fn=spatial.density_penalty_fn(),
            hazard_fn=sim.hazard_penalty_at_cell,
        )
        sim.attach_navigation(navigator)

        # behavior config
        behavior_cfg = sc.get("behavior", {}) or {}
        bconfig = BehaviorConfig(
            density_panic_weight=float(behavior_cfg.get("density_panic_weight", 0.2)),
            neighbor_panic_weight=float(behavior_cfg.get("neighbor_panic_weight", 0.1)),
            hazard_panic_weight=float(behavior_cfg.get("hazard_panic_weight", 0.15)),
            entropy_anxiety_weight=float(behavior_cfg.get("entropy_anxiety_weight", 0.25)),
            freeze_probability=float(behavior_cfg.get("freeze_probability", 0.02)),
            calm_recovery_rate=float(behavior_cfg.get("calm_recovery_rate", 0.005)),
            helping_threshold=float(behavior_cfg.get("helping_threshold", 0.7)),
        )
        sim.attach_behavior_model(BehaviorModel(bconfig))
        policy = create_intervention_policy(sc.get("interventions"))
        if policy is not None:
            sim.attach_intervention_policy(policy)
        decision_policy = create_agent_decision_policy(sc.get("llm_decisions"))
        if decision_policy is not None:
            sim.attach_agent_decision_policy(decision_policy)
        sim.attach_hostile_channels(create_hostile_channels(sc.get("hostile_channels")))
        return sim

    def _build_layout(self, scenario: Dict[str, Any]) -> Layout:
        layout_cfg = scenario.get("layout", {}) or {}
        if "floors" not in layout_cfg:
            raise ValueError("Strict 3D scenarios must define layout.floors and may not use layout.text/file/grid/geojson/cad")
        return Layout.from_floors(
            layout_cfg["floors"],
            connectors=layout_cfg.get("connectors", []) or [],
            cell_size=float(layout_cfg.get("cell_size", 1.0)),
            origin=tuple(layout_cfg.get("origin", (0.0, 0.0))),
        )

    def _build_geojson_layout(self, geojson_cfg: Any, source_file: Optional[str]) -> Layout:
        if isinstance(geojson_cfg, str):
            geojson_cfg = {"file": geojson_cfg}
        if not isinstance(geojson_cfg, dict):
            raise ValueError("layout.geojson must be a mapping or file path")
        source = geojson_cfg.get("data")
        if source is None:
            if "file" not in geojson_cfg:
                raise ValueError("layout.geojson requires either file or data")
            source = self._resolve_relative_path(str(geojson_cfg["file"]), source_file)
        return Layout.from_geojson(
            source, cell_size=float(geojson_cfg.get("cell_size", 1.0)),
            padding=int(geojson_cfg.get("padding", 1)),
            role_property=str(geojson_cfg.get("role_property", "role")),
            default_token=geojson_cfg.get("default_token"),
            add_border_walls=bool(geojson_cfg.get("add_border_walls", False)),
        )

    def _build_cad_layout(self, cad_cfg: Any, source_file: Optional[str]) -> Layout:
        if isinstance(cad_cfg, str):
            cad_cfg = {"file": cad_cfg}
        if not isinstance(cad_cfg, dict):
            raise ValueError("layout.cad must be a mapping or file path")
        source = cad_cfg.get("data")
        if source is None:
            if "file" not in cad_cfg:
                raise ValueError("layout.cad requires either file or data")
            source = self._resolve_relative_path(str(cad_cfg["file"]), source_file)
        cad_format = str(cad_cfg.get("format", "dxf")).lower()
        if cad_format != "dxf":
            raise ValueError("Only DXF CAD ingestion is currently supported")
        return Layout.from_cad(
            source, cell_size=float(cad_cfg.get("cell_size", 1.0)),
            padding=int(cad_cfg.get("padding", 1)),
            role_layers=cad_cfg.get("role_layers"),
            default_role=str(cad_cfg.get("default_role", "obstacle")),
            default_token=cad_cfg.get("default_token"),
            add_border_walls=bool(cad_cfg.get("add_border_walls", False)),
            line_thickness=float(cad_cfg.get("line_thickness", 1.0)),
        )

    def _build_hazards(
        self,
        hazards_cfg: List[Dict[str, Any]],
        *,
        source_file: Optional[str] = None,
    ) -> List[Hazard]:
        hazards: List[Hazard] = []
        for hc in hazards_cfg:
            if "field" in hc:
                field_cfg = hc["field"]
                if isinstance(field_cfg, str):
                    field_cfg = {"file": field_cfg}
                if not isinstance(field_cfg, dict) or "file" not in field_cfg:
                    raise ValueError("Imported hazard fields require field.file")
                path = self._resolve_relative_path(
                    str(field_cfg["file"]),
                    source_file,
                )
                hazards.append(ImportedHazardField.from_file(
                    path,
                    kind=str(hc.get("type", field_cfg.get("kind", "GAS"))),
                ))
                continue
            wind = hc.get("wind_vector", [0.0, 0.0])
            hazards.append(Hazard(
                pos=tuple(float(value) for value in hc.get("location", [0, 0, 0])),
                kind=hc.get("type", "GAS"),
                radius=float(hc.get("radius", 0.0)),
                severity=float(hc.get("severity", 0.5)),
                spread_rate=float(hc.get("spread_rate", 0.0)),
                wind_vector=(float(wind[0]), float(wind[1])),
                diffusion_rate=float(hc.get("diffusion_rate", 0.1)),
                visibility_reduction=float(hc.get("visibility_reduction", 0.0)),
                range_m=float(hc.get("range", hc.get("range_m", 8.0))),
                accuracy=float(hc.get("accuracy", 0.35)),
                height_aware=bool(hc.get("height_aware", False)),
                layer_base_m=None if hc.get("layer_base_m") is None else float(hc["layer_base_m"]),
                layer_top_m=None if hc.get("layer_top_m") is None else float(hc["layer_top_m"]),
                vertical_decay_m=float(hc.get("vertical_decay_m", 1.0)),
                gas_density=float(hc.get("gas_density", 1.0)),
                ember_spotting_rate=float(hc.get("ember_spotting_rate", 0.0)),
                ember_ignition_radius=float(hc.get("ember_ignition_radius", 0.0)),
                ember_decay_rate=float(hc.get("ember_decay_rate", 0.15)),
            ))
        return hazards

    def _build_hostile_agents(self, layout: Layout, hostile_cfg: List[Dict[str, Any]], agent_offset: int) -> List[HostileAgent]:
        hostiles: List[HostileAgent] = []
        for i, hc in enumerate(hostile_cfg):
            count = int(hc.get("count", 1))
            spawn_cells = list(hc.get("spawn_cells", []) or [])
            base_speed = float(hc.get("base_speed", hc.get("base_speed_mps", 1.2)))
            for j in range(count):
                if spawn_cells:
                    cell = self._parse_cell(spawn_cells[j % len(spawn_cells)], layout)
                else:
                    cell = layout.random_walkable_position()
                hostiles.append(HostileAgent(
                    id=agent_offset + len(hostiles),
                    pos=layout.world_position(cell),
                    floor_id=cell[0],
                    base_speed=base_speed,
                    release_step=int(hc.get("release_step", 0)),
                    cohort_name=str(hc.get("name", f"hostile_{i+1}")),
                    range_m=float(hc.get("range", hc.get("range_m", 8.0))),
                    accuracy=float(hc.get("accuracy", 0.35)),
                ))
        return hostiles

    def _build_responders(self, layout: Layout, responders_cfg: List[Dict[str, Any]], agent_offset: int) -> List[FirstResponder]:
        responders: List[FirstResponder] = []
        # find responder entry points from layout
        entry_points = layout.responder_positions()
        for floor_id, x, y in entry_points:
            layout.floors[floor_id].grid[y, x] = EMPTY

        for i, rc in enumerate(responders_cfg):
            count = int(rc.get("count", 1))
            release_step = int(rc.get("release_step", 0))
            ppe_factor = float(rc.get("ppe_factor", 0.1))
            base_speed = float(rc.get("base_speed", rc.get("base_speed_mps", 1.34)))
            base_speed *= float(rc.get("base_speed_multiplier", 1.0))
            spawn_cells = rc.get("spawn_cells", [])
            mission_target = rc.get("mission_target", None)
            if mission_target:
                mission_target = tuple(float(value) for value in mission_target)

            for j in range(count):
                if spawn_cells:
                    cell = self._parse_cell(spawn_cells[j % len(spawn_cells)], layout)
                elif entry_points:
                    cell = entry_points[j % len(entry_points)]
                else:
                    cell = layout.random_walkable_position()
                pos = layout.world_position(cell)
                r = FirstResponder(
                    id=agent_offset + len(responders),
                    pos=pos,
                    floor_id=cell[0],
                    base_speed=base_speed,
                    release_step=release_step,
                    cohort_name=f"responder_{i+1}",
                    ppe_factor=ppe_factor,
                    broadcast_radius=float(rc.get("broadcast_radius", 5.0)),
                    mission_target=mission_target,
                )
                self._apply_agent_calibration(r, rc)
                responders.append(r)
        return responders

    def _build_agents(self, layout: Layout, population_cfg: Dict[str, Any]) -> List[Commuter]:
        layout_positions = [
            layout.world_position(cell) for cell in layout.people_positions()
        ]
        cohorts_cfg = list(population_cfg.get("cohorts", []) or [])
        total = int(population_cfg.get("total", 0))

        if not cohorts_cfg:
            default_total = total or len(layout_positions) or 100
            cohorts_cfg = [{
                "name": "baseline", "count": default_total,
                "personality": "NORMAL", "calmness": 0.8,
                "base_speed_multiplier": 1.0, "release_step": 0,
                "group_size": 1, "familiarity": 0.5,
            }]

        cohort_total = sum(int(c.get("count", 0)) for c in cohorts_cfg)
        if total and cohort_total < total:
            cohorts_cfg.append({
                "name": "supplemental", "count": total - cohort_total,
                "personality": "NORMAL", "calmness": 0.8,
                "base_speed_multiplier": 1.0, "release_step": 0,
                "group_size": 1, "familiarity": 0.5,
            })

        required = sum(int(c.get("count", 0)) for c in cohorts_cfg)
        positions = list(layout_positions)
        while len(positions) < required:
            positions.append(layout.world_position(layout.random_walkable_position()))

        agents: List[Commuter] = []
        pos_idx = 0
        group_counter = 0
        cohort_agents: Dict[str, List[Commuter]] = {}
        helper_specs: List[Dict[str, Any]] = []

        for cohort_cfg in cohorts_cfg:
            cohort_name = str(cohort_cfg.get("name", f"cohort_{len(cohort_agents)+1}"))
            count = int(cohort_cfg.get("count", 0))
            personality = str(cohort_cfg.get("personality", "NORMAL"))
            calmness = float(cohort_cfg.get("calmness", 0.8))
            base_speed = float(cohort_cfg.get("base_speed", cohort_cfg.get("base_speed_mps", 1.34)))
            base_speed *= float(cohort_cfg.get("base_speed_multiplier", 1.0))
            release_step = int(cohort_cfg.get("release_step", 0))
            group_size = max(1, int(cohort_cfg.get("group_size", 1)))
            spawn_cells = list(cohort_cfg.get("spawn_cells", []) or [])
            familiarity = float(cohort_cfg.get("familiarity", 0.5))
            mobility_class = str(cohort_cfg.get("mobility_class", "standard"))
            mobility = MOBILITY_CLASS_DEFAULTS.get(mobility_class, MOBILITY_CLASS_DEFAULTS["standard"])
            base_speed *= float(cohort_cfg.get("mobility_speed_multiplier", mobility["speed_multiplier"]))
            base_vision_radius = float(cohort_cfg.get("base_vision_radius", 5.0))
            base_vision_radius *= float(cohort_cfg.get("mobility_vision_multiplier", mobility["vision_multiplier"]))
            separation_threshold = float(cohort_cfg.get("separation_anxiety_threshold", 1.5))
            breathing_height = float(cohort_cfg.get("breathing_height_m", mobility["breathing_height_m"]))
            homophily_profile = dict(cohort_cfg.get("homophily_profile", {}) or {})
            homophily_weight = float(cohort_cfg.get("homophily_weight", 0.0))
            exit_affinity = float(cohort_cfg.get("exit_affinity", 0.5))
            herding = float(cohort_cfg.get("herding", 0.5))

            members: List[Commuter] = []
            for _ in range(count):
                if spawn_cells:
                    cell = self._parse_cell(spawn_cells[len(members) % len(spawn_cells)], layout)
                    position = layout.world_position(cell)
                else:
                    position = np.array(positions[pos_idx], copy=True)
                    pos_idx += 1
                    cell = layout.cell(position)
                agent = Commuter(
                    id=len(agents), pos=position,
                    floor_id=cell[0],
                    base_speed=base_speed,
                    personality=personality, calmness=calmness,
                    release_step=release_step, cohort_name=cohort_name,
                    familiarity=familiarity,
                    family_id=None if cohort_cfg.get("family_id") is None else str(cohort_cfg["family_id"]),
                    role_in_group=str(cohort_cfg.get("role_in_group", "solo")),
                    separation_anxiety_threshold=separation_threshold,
                    mobility_class=mobility_class,
                    breathing_height_m=breathing_height,
                    base_vision_radius=base_vision_radius,
                    vision_radius=base_vision_radius,
                    homophily_profile=homophily_profile,
                    homophily_weight=homophily_weight,
                    exit_affinity=exit_affinity,
                    herding=herding,
                )
                self._apply_agent_calibration(agent, cohort_cfg)
                agents.append(agent)
                members.append(agent)

            if group_size > 1:
                for start in range(0, len(members), group_size):
                    grp = members[start:start+group_size]
                    if len(grp) <= 1:
                        break
                    leader = grp[0]
                    group_counter += 1
                    leader.group_id = group_counter
                    leader.family_id = leader.family_id or f"{cohort_name}_{group_counter}"
                    if leader.role_in_group == "solo":
                        leader.role_in_group = "leader"
                    for follower in grp[1:]:
                        follower.group_id = group_counter
                        follower.leader_id = leader.id
                        follower.family_id = follower.family_id or leader.family_id
                        if follower.role_in_group == "solo":
                            follower.role_in_group = "member"

            cohort_agents[cohort_name] = members
            if cohort_cfg.get("assist_to_cohort"):
                helper_specs.append({
                    "helper_cohort": cohort_name,
                    "dependent_cohort": str(cohort_cfg["assist_to_cohort"]),
                })

        for hs in helper_specs:
            helpers = cohort_agents.get(hs["helper_cohort"], [])
            dependents = cohort_agents.get(hs["dependent_cohort"], [])
            for helper, dependent in zip(helpers, dependents):
                group_counter += 1
                helper.group_id = group_counter
                helper.assisted_agent_id = dependent.id
                helper.family_id = helper.family_id or f"assist_{group_counter}"
                helper.role_in_group = "helper"
                helper.base_speed = min(helper.base_speed, dependent.base_speed * 1.05)
                dependent.group_id = group_counter
                dependent.leader_id = helper.id
                dependent.family_id = dependent.family_id or helper.family_id
                dependent.role_in_group = "dependent"

        return agents

    def _build_destination_profiles(self, scenario: Dict[str, Any], layout: Layout) -> Dict[tuple, Dict[str, Any]]:
        raw_profiles = scenario.get("destination_profiles", scenario.get("exit_profiles", [])) or []
        profiles: Dict[tuple, Dict[str, Any]] = {}
        for raw in raw_profiles:
            if not isinstance(raw, dict):
                continue
            cell_raw = raw.get("cell", raw.get("exit"))
            if cell_raw is None:
                continue
            cell = self._parse_cell(cell_raw, layout)
            profiles[cell] = dict(raw.get("profile", raw.get("homophily_profile", {})) or {})
        return profiles

    def _build_wui_egress(self, scenario: Dict[str, Any], layout: Layout) -> List[Dict[str, Any]]:
        cfg = scenario.get("wui_egress", {}) or {}
        segments = []
        for raw in cfg.get("road_segments", []) or []:
            cells = [self._parse_cell(cell, layout) for cell in raw.get("cells", []) or []]
            if not cells:
                continue
            segments.append({
                "id": str(raw.get("id", f"road_{len(segments) + 1}")),
                "cells": cells,
                "mode_switch": str(raw.get("mode_switch", "vehicle")),
                "speed_multiplier": float(raw.get("speed_multiplier", raw.get("vehicle_speed_multiplier", 3.0))),
                "capacity": int(raw.get("capacity", 999999)),
            })
        return segments

    def _apply_agent_calibration(self, agent, config: Dict[str, Any]) -> None:
        if "base_rationality" in config:
            agent.base_rationality = float(config["base_rationality"])
            agent.rationality = float(config["base_rationality"])
        if "credibility" in config:
            agent.credibility = float(config["credibility"])
        if "gossip_radius" in config:
            agent.gossip_radius = float(config["gossip_radius"])
        if "base_vision_radius" in config:
            agent.base_vision_radius = float(config["base_vision_radius"])
            agent.vision_radius = agent.effective_vision_radius()

    def _deep_merge(self, base: Dict[str, Any], overrides: Dict[str, Any]) -> Dict[str, Any]:
        merged = dict(base)
        for key, value in overrides.items():
            if key in merged and isinstance(merged[key], dict) and isinstance(value, dict):
                merged[key] = self._deep_merge(merged[key], value)
            else:
                merged[key] = value
        return merged

    def serialize_layout(self, layout: Layout) -> str:
        return "\n".join("".join(str(c) for c in row) for row in layout.grid)

    def serialize_layout_floors(self, layout: Layout) -> List[Dict[str, Any]]:
        floors: List[Dict[str, Any]] = []
        for floor_id, floor in layout.floors.items():
            item: Dict[str, Any] = {
                "id": floor_id,
                "z": layout.floor_z(floor_id),
                "text": "\n".join("".join(str(c) for c in row) for row in floor.grid),
            }
            if floor.cell_heights is not None:
                item["cell_heights"] = floor.cell_heights.tolist()
            floors.append(item)
        return floors

    def serialize_layout_connectors(self, layout: Layout) -> List[Dict[str, Any]]:
        return [
            {
                "id": connector.id,
                "type": connector.type,
                "from": {"floor": connector.from_cell[0], "x": connector.from_cell[1], "y": connector.from_cell[2]},
                "to": {"floor": connector.to_cell[0], "x": connector.to_cell[1], "y": connector.to_cell[2]},
                "bidirectional": connector.bidirectional,
                "width": connector.width,
                "speed_multiplier": connector.speed_multiplier,
                "capacity": connector.capacity,
                "flow_rate": connector.flow_rate,
                "queue_mode": connector.queue_mode,
                "panic_jam_density": connector.panic_jam_density,
                "jam_flow_multiplier": connector.jam_flow_multiplier,
                "dwell_s": connector.dwell_s,
                "travel_s": connector.travel_s,
                "height_delta_m": connector.height_delta_m,
            }
            for connector in layout.connectors
        ]

    def apply_layout_cells(self, scenario, *, cells, fill) -> Dict[str, Any]:
        layout = self._build_layout(scenario)
        for raw in cells:
            floor_id, x, y = layout.cell(raw)
            floor = layout.floors.get(floor_id)
            if floor is not None and 0 <= y < floor.grid.shape[0] and 0 <= x < floor.grid.shape[1]:
                floor.grid[y, x] = fill
        updated = dict(scenario)
        layout_cfg = dict(scenario.get("layout", {}))
        layout_cfg["cell_size"] = layout.cell_size
        layout_cfg["floors"] = self.serialize_layout_floors(layout)
        layout_cfg["connectors"] = self.serialize_layout_connectors(layout)
        updated["layout"] = layout_cfg
        return updated

    def _parse_cell(self, raw, layout: Layout):
        if isinstance(raw, dict):
            return layout.cell((str(raw["floor"]), int(raw["x"]), int(raw["y"])))
        if len(raw) >= 3 and isinstance(raw[0], str):
            return layout.cell(raw)
        if len(raw) >= 4:
            return layout.cell((str(raw[0]), int(raw[1]), int(raw[2])))
        return layout.cell(raw)

    def _resolve_relative_path(self, raw_path: str, source_file: Optional[str]) -> str:
        path = Path(raw_path)
        if path.is_absolute():
            return str(path)
        if path.exists():
            return str(path.resolve())
        if source_file is not None:
            candidate = Path(source_file).resolve().parent / path
            if candidate.exists():
                return str(candidate)
        return str(path)

    @staticmethod
    def wall_token() -> str:
        return WALL

    @staticmethod
    def empty_token() -> str:
        return EMPTY

    @staticmethod
    def exit_token() -> str:
        return EXIT
