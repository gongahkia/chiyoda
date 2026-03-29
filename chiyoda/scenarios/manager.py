from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import random
from typing import Any, Dict, List, Optional

import numpy as np
import yaml

from chiyoda.environment.layout import EMPTY, EXIT, Layout, WALL
from chiyoda.environment.obstacles import apply_obstacles_to_grid, obstacles_from_config
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.agents.commuter import Commuter
from chiyoda.agents.behaviors import BehaviorModel
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.navigation.spatial_index import SpatialIndex


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

    def load_scenario(
        self,
        scenario_file: str,
        *,
        overrides: Optional[Dict[str, Any]] = None,
        random_seed: Optional[int] = None,
    ) -> Simulation:
        scenario = self.load_config(scenario_file)
        if overrides:
            scenario = self._deep_merge(scenario, overrides)
        if random_seed is not None:
            scenario.setdefault("simulation", {})
            scenario["simulation"]["random_seed"] = random_seed
        return self.build_simulation(scenario)

    def build_simulation(self, scenario: Dict[str, Any]) -> Simulation:
        sc = dict(scenario)
        simulation_cfg = sc.get("simulation", {})
        random_seed = simulation_cfg.get("random_seed", 42)
        if random_seed is not None:
            random.seed(random_seed)
            np.random.seed(random_seed)

        layout = self._build_layout(sc)

        exits = [Exit(pos=tuple(p)) for p in layout.exit_positions()]
        hazards = self._build_hazards(sc.get("hazards", []) or [])
        agents = self._build_agents(layout, sc.get("population", {}) or {})

        sim_cfg = SimulationConfig(
            max_steps=int(simulation_cfg.get("max_steps", 500)),
            dt=float(simulation_cfg.get("dt", 0.1)),
            random_seed=random_seed,
            hazard_avoidance_weight=float(
                simulation_cfg.get("hazard_avoidance_weight", 1.25)
            ),
            acceleration_backend=str(simulation_cfg.get("acceleration_backend", "auto")),
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
        sim.attach_behavior_model(BehaviorModel())
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
                layout.grid,
                obstacles_from_config(obstacle_cfgs),
                origin=tuple(layout.origin),
                cell_size=float(layout.cell_size),
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
            source,
            cell_size=float(geojson_cfg.get("cell_size", 1.0)),
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
            source,
            cell_size=float(cad_cfg.get("cell_size", 1.0)),
            padding=int(cad_cfg.get("padding", 1)),
            role_layers=cad_cfg.get("role_layers"),
            default_role=str(cad_cfg.get("default_role", "obstacle")),
            default_token=cad_cfg.get("default_token"),
            add_border_walls=bool(cad_cfg.get("add_border_walls", False)),
            line_thickness=float(cad_cfg.get("line_thickness", 1.0)),
        )

    def _build_hazards(self, hazards_cfg: List[Dict[str, Any]]) -> List[Hazard]:
        hazards: List[Hazard] = []
        for hazard_cfg in hazards_cfg:
            hazards.append(
                Hazard(
                    pos=tuple(hazard_cfg.get("location", [0, 0])),
                    kind=hazard_cfg.get("type", "GAS"),
                    radius=float(hazard_cfg.get("radius", 0.0)),
                    severity=float(hazard_cfg.get("severity", 0.5)),
                    spread_rate=float(hazard_cfg.get("spread_rate", 0.0)),
                )
            )
        return hazards

    def _build_agents(self, layout: Layout, population_cfg: Dict[str, Any]) -> List[Commuter]:
        layout_positions = [
            np.array([x + 0.5, y + 0.5], dtype=float) for x, y in layout.people_positions()
        ]
        cohorts_cfg = list(population_cfg.get("cohorts", []) or [])
        total = int(population_cfg.get("total", 0))

        if not cohorts_cfg:
            default_total = total or len(layout_positions) or 100
            cohorts_cfg = [
                {
                    "name": "baseline",
                    "count": default_total,
                    "personality": "NORMAL",
                    "calmness": 0.8,
                    "base_speed_multiplier": 1.0,
                    "release_step": 0,
                    "group_size": 1,
                }
            ]

        cohort_total = sum(int(cohort.get("count", 0)) for cohort in cohorts_cfg)
        if total and cohort_total < total:
            cohorts_cfg.append(
                {
                    "name": "supplemental",
                    "count": total - cohort_total,
                    "personality": "NORMAL",
                    "calmness": 0.8,
                    "base_speed_multiplier": 1.0,
                    "release_step": 0,
                    "group_size": 1,
                }
            )

        required_agents = sum(int(cohort.get("count", 0)) for cohort in cohorts_cfg)
        positions = list(layout_positions)
        while len(positions) < required_agents:
            x, y = layout.random_walkable_position()
            positions.append(np.array([x + 0.5, y + 0.5], dtype=float))

        agents: List[Commuter] = []
        position_index = 0
        group_counter = 0
        cohort_agents: Dict[str, List[Commuter]] = {}
        helper_specs: List[Dict[str, Any]] = []

        for cohort_cfg in cohorts_cfg:
            cohort_name = str(cohort_cfg.get("name", f"cohort_{len(cohort_agents) + 1}"))
            count = int(cohort_cfg.get("count", 0))
            personality = str(cohort_cfg.get("personality", "NORMAL"))
            calmness = float(cohort_cfg.get("calmness", 0.8))
            base_speed_multiplier = float(cohort_cfg.get("base_speed_multiplier", 1.0))
            release_step = int(cohort_cfg.get("release_step", 0))
            group_size = max(1, int(cohort_cfg.get("group_size", 1)))
            spawn_cells = list(cohort_cfg.get("spawn_cells", []) or [])

            cohort_members: List[Commuter] = []
            for _ in range(count):
                if spawn_cells:
                    x, y = spawn_cells[(len(cohort_members)) % len(spawn_cells)]
                    position = np.array([float(x) + 0.5, float(y) + 0.5], dtype=float)
                else:
                    position = np.array(positions[position_index], copy=True)
                    position_index += 1
                agent = Commuter(
                    id=len(agents),
                    pos=position,
                    base_speed=1.34 * base_speed_multiplier,
                    personality=personality,
                    calmness=calmness,
                    release_step=release_step,
                    cohort_name=cohort_name,
                )
                agents.append(agent)
                cohort_members.append(agent)

            if group_size > 1:
                for start in range(0, len(cohort_members), group_size):
                    members = cohort_members[start : start + group_size]
                    if len(members) <= 1:
                        break
                    leader = members[0]
                    group_counter += 1
                    leader.group_id = group_counter
                    for follower in members[1:]:
                        follower.group_id = group_counter
                        follower.leader_id = leader.id

            cohort_agents[cohort_name] = cohort_members
            if cohort_cfg.get("assist_to_cohort"):
                helper_specs.append(
                    {
                        "helper_cohort": cohort_name,
                        "dependent_cohort": str(cohort_cfg["assist_to_cohort"]),
                    }
                )

        for helper_spec in helper_specs:
            helpers = cohort_agents.get(helper_spec["helper_cohort"], [])
            dependents = cohort_agents.get(helper_spec["dependent_cohort"], [])
            for helper, dependent in zip(helpers, dependents):
                group_counter += 1
                helper.group_id = group_counter
                helper.assisted_agent_id = dependent.id
                helper.base_speed = min(helper.base_speed, dependent.base_speed * 1.05)
                dependent.group_id = group_counter
                dependent.leader_id = helper.id

        return agents

    def _deep_merge(self, base: Dict[str, Any], overrides: Dict[str, Any]) -> Dict[str, Any]:
        merged = dict(base)
        for key, value in overrides.items():
            if (
                key in merged
                and isinstance(merged[key], dict)
                and isinstance(value, dict)
            ):
                merged[key] = self._deep_merge(merged[key], value)
            else:
                merged[key] = value
        return merged

    def serialize_layout(self, layout: Layout) -> str:
        lines: List[str] = []
        for row in layout.grid:
            lines.append("".join(str(cell) for cell in row))
        return "\n".join(lines)

    def apply_layout_cells(
        self,
        scenario: Dict[str, Any],
        *,
        cells: List[List[int]] | List[tuple[int, int]],
        fill: str,
    ) -> Dict[str, Any]:
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
