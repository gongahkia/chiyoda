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
from chiyoda.environment.layout import EMPTY, EXIT, Layout, WALL, BEACON, RESPONDER_ENTRY
from chiyoda.environment.obstacles import apply_obstacles_to_grid, obstacles_from_config
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.agents.commuter import Commuter
from chiyoda.agents.responder import FirstResponder
from chiyoda.agents.behaviors import BehaviorModel, BehaviorConfig
from chiyoda.information.interventions import create_intervention_policy
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.navigation.spatial_index import SpatialIndex
from chiyoda.scenarios.generated_calibration import apply_generated_population_calibration


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
        hazards = self._build_hazards(sc.get("hazards", []) or [])
        agents = self._build_agents(layout, sc.get("population", {}) or {})

        # build responders if specified
        responders = self._build_responders(layout, sc.get("responders", []) or [], len(agents))
        agents.extend(responders)

        # ITED config
        info_cfg = sc.get("information", {}) or {}
        sim_cfg = SimulationConfig(
            max_steps=int(simulation_cfg.get("max_steps", 500)),
            dt=float(simulation_cfg.get("dt", 0.1)),
            random_seed=random_seed,
            hazard_avoidance_weight=float(simulation_cfg.get("hazard_avoidance_weight", 1.25)),
            acceleration_backend=str(simulation_cfg.get("acceleration_backend", "auto")),
            information_mode=str(info_cfg.get("mode", "asymmetric")),
            info_decay_rate=float(info_cfg.get("decay_rate", 0.01)),
            observation_radius=float(info_cfg.get("observation_radius", 5.0)),
            gossip_radius=float(info_cfg.get("gossip_radius", 2.0)),
            beacon_radius=float(info_cfg.get("beacon_radius", 8.0)),
        )
        sim = Simulation(layout=layout, agents=agents, exits=exits, hazards=hazards, config=sim_cfg)

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
        return sim

    def _build_layout(self, scenario: Dict[str, Any]) -> Layout:
        layout_cfg = scenario.get("layout", {}) or {}
        source_file = scenario.get("_source_file")
        layout: Layout
        if "geojson" in layout_cfg:
            layout = self._build_geojson_layout(layout_cfg["geojson"], source_file)
        elif "cad" in layout_cfg:
            layout = self._build_cad_layout(layout_cfg["cad"], source_file)
        elif "text" in layout_cfg:
            layout = Layout.from_text(str(layout_cfg["text"]))
        elif "grid" in layout_cfg:
            layout = Layout.from_text("\n".join(str(line) for line in layout_cfg["grid"]))
        elif "file" in layout_cfg:
            layout_path = Path(str(layout_cfg["file"]))
            if not layout_path.is_absolute():
                if layout_path.exists():
                    layout = Layout.from_file(str(layout_path.resolve()))
                elif source_file is not None:
                    candidate = Path(source_file).resolve().parent / layout_path
                    if candidate.exists():
                        layout = Layout.from_file(str(candidate))
                    else:
                        layout = Layout.from_file(str(layout_path))
                else:
                    layout = Layout.from_file(str(layout_path))
            else:
                layout = Layout.from_file(str(layout_path))
        else:
            raise ValueError(
                "Scenario layout must define one of layout.file, layout.text, layout.grid, layout.geojson, or layout.cad"
            )
        obstacle_cfgs = list(layout_cfg.get("obstacles", []) or [])
        if obstacle_cfgs:
            layout.grid = apply_obstacles_to_grid(
                layout.grid, obstacles_from_config(obstacle_cfgs),
                origin=tuple(layout.origin), cell_size=float(layout.cell_size),
            )
        return layout

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

    def _build_hazards(self, hazards_cfg: List[Dict[str, Any]]) -> List[Hazard]:
        hazards: List[Hazard] = []
        for hc in hazards_cfg:
            wind = hc.get("wind_vector", [0.0, 0.0])
            hazards.append(Hazard(
                pos=tuple(hc.get("location", [0, 0])),
                kind=hc.get("type", "GAS"),
                radius=float(hc.get("radius", 0.0)),
                severity=float(hc.get("severity", 0.5)),
                spread_rate=float(hc.get("spread_rate", 0.0)),
                wind_vector=(float(wind[0]), float(wind[1])),
                diffusion_rate=float(hc.get("diffusion_rate", 0.1)),
                visibility_reduction=float(hc.get("visibility_reduction", 0.0)),
            ))
        return hazards

    def _build_responders(self, layout: Layout, responders_cfg: List[Dict[str, Any]], agent_offset: int) -> List[FirstResponder]:
        responders: List[FirstResponder] = []
        # find responder entry points from layout
        entry_points = []
        for y in range(layout.height):
            for x in range(layout.width):
                if layout.grid[y, x] == RESPONDER_ENTRY:
                    entry_points.append((x, y))
                    layout.grid[y, x] = EMPTY # make it walkable

        for i, rc in enumerate(responders_cfg):
            count = int(rc.get("count", 1))
            release_step = int(rc.get("release_step", 0))
            ppe_factor = float(rc.get("ppe_factor", 0.1))
            base_speed = float(rc.get("base_speed", rc.get("base_speed_mps", 1.34)))
            base_speed *= float(rc.get("base_speed_multiplier", 1.0))
            spawn_cells = rc.get("spawn_cells", [])
            mission_target = rc.get("mission_target", None)
            if mission_target:
                mission_target = (float(mission_target[0]), float(mission_target[1]))

            for j in range(count):
                if spawn_cells:
                    sx, sy = spawn_cells[j % len(spawn_cells)]
                elif entry_points:
                    sx, sy = entry_points[j % len(entry_points)]
                else:
                    sx, sy = layout.random_walkable_position()
                pos = np.array([float(sx) + 0.5, float(sy) + 0.5], dtype=float)
                r = FirstResponder(
                    id=agent_offset + len(responders),
                    pos=pos,
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
            np.array([x + 0.5, y + 0.5], dtype=float) for x, y in layout.people_positions()
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
            x, y = layout.random_walkable_position()
            positions.append(np.array([x + 0.5, y + 0.5], dtype=float))

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

            members: List[Commuter] = []
            for _ in range(count):
                if spawn_cells:
                    sx, sy = spawn_cells[len(members) % len(spawn_cells)]
                    position = np.array([float(sx) + 0.5, float(sy) + 0.5], dtype=float)
                else:
                    position = np.array(positions[pos_idx], copy=True)
                    pos_idx += 1
                agent = Commuter(
                    id=len(agents), pos=position,
                    base_speed=base_speed,
                    personality=personality, calmness=calmness,
                    release_step=release_step, cohort_name=cohort_name,
                    familiarity=familiarity,
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
                    for follower in grp[1:]:
                        follower.group_id = group_counter
                        follower.leader_id = leader.id

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
                helper.base_speed = min(helper.base_speed, dependent.base_speed * 1.05)
                dependent.group_id = group_counter
                dependent.leader_id = helper.id

        return agents

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

    def apply_layout_cells(self, scenario, *, cells, fill) -> Dict[str, Any]:
        layout = self._build_layout(scenario)
        for x, y in cells:
            if 0 <= y < layout.height and 0 <= x < layout.width:
                layout.grid[y, x] = fill
        updated = dict(scenario)
        updated["layout"] = {"text": self.serialize_layout(layout)}
        return updated

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
