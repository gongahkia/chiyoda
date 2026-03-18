from __future__ import annotations

from dataclasses import dataclass
import random
from typing import List
import yaml
import numpy as np

from chiyoda.environment.layout import Layout
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
    def load_scenario(self, scenario_file: str) -> Simulation:
        with open(scenario_file, "r") as f:
            cfg = yaml.safe_load(f)

        sc = cfg.get("scenario", cfg)
        simulation_cfg = sc.get("simulation", {})
        random_seed = simulation_cfg.get("random_seed", 42)
        if random_seed is not None:
            random.seed(random_seed)
            np.random.seed(random_seed)
        layout_path = sc["layout"]["file"]
        layout = Layout.from_file(layout_path)

        # Build exits from layout
        exits = [Exit(pos=tuple(p)) for p in layout.exit_positions()]

        # Build hazards
        hazards = []
        for h in sc.get("hazards", []) or []:
            hazards.append(
                Hazard(
                    pos=tuple(h.get("location", [0, 0])),
                    kind=h.get("type", "GAS"),
                    radius=float(h.get("radius", 0.0)),
                    severity=float(h.get("severity", 0.5)),
                    spread_rate=float(h.get("spread_rate", 0.0)),
                )
            )

        # Build agents
        total = sc.get("population", {}).get("total", 100)
        people_from_layout = [
            Commuter(id=i, pos=np.array([x + 0.5, y + 0.5], dtype=float))
            for i, (x, y) in enumerate(layout.people_positions())
        ]
        agents: List[Commuter] = people_from_layout
        # If fewer than total, add random agents
        next_id = len(agents)
        while len(agents) < total:
            x, y = layout.random_walkable_position()
            agents.append(Commuter(id=next_id, pos=np.array([x + 0.5, y + 0.5], dtype=float)))
            next_id += 1

        # Build simulation
        sim_cfg = SimulationConfig(
            max_steps=int(simulation_cfg.get("max_steps", 500)),
            dt=float(simulation_cfg.get("dt", 0.1)),
            random_seed=random_seed,
        )
        sim = Simulation(layout=layout, agents=agents, exits=exits, hazards=hazards, config=sim_cfg)

        # Wire systems
        spatial = SpatialIndex()
        sim.attach_spatial_index(spatial)
        navigator = SmartNavigator(layout, density_fn=spatial.density_penalty_fn())
        sim.attach_navigation(navigator)
        sim.attach_behavior_model(BehaviorModel())

        return sim
