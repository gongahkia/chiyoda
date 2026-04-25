"""
CognitiveAgent — ITED agent with BDI architecture, physiology, and belief state.

Replaces the simple AgentBase with a cognitively-grounded model that
maintains partial knowledge, responds to physiological impairment, and
makes bounded-rational decisions under uncertainty.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Tuple

import numpy as np

from chiyoda.information.field import BeliefVector
from chiyoda.navigation.social_force import adjusted_step


# BDI intention labels
INTENTION_EVACUATE = "EVACUATE"  # following a known path to a believed exit
INTENTION_EXPLORE = "EXPLORE"  # no known exit, exploring environment
INTENTION_FOLLOW = "FOLLOW"  # following nearby agents (herd behavior)
INTENTION_ASSIST = "ASSIST"  # helping another agent
INTENTION_FREEZE = "FREEZE"  # frozen in place
INTENTION_RESPOND = "RESPOND"  # first responder moving toward hazard


@dataclass
class PhysiologyState:
    """Tracks physiological effects of hazard exposure."""
    cumulative_exposure: float = 0.0
    speed_factor: float = 1.0  # [0,1] multiplier on base speed
    vision_factor: float = 1.0  # [0,1] multiplier on observation radius
    rationality_factor: float = 1.0  # [0,1] multiplier on decision quality
    incapacitated: bool = False
    impairment_level: float = 0.0  # [0,1] overall impairment

    def update_from_exposure(self, hazard_load: float, dt: float) -> None:
        """Update physiology based on current hazard load."""
        self.cumulative_exposure += hazard_load * dt

        # piecewise impairment curve: mild → moderate → severe → incapacitation
        e = self.cumulative_exposure
        if e < 0.5:
            self.impairment_level = e * 0.2  # mild
        elif e < 2.0:
            self.impairment_level = 0.1 + (e - 0.5) * 0.3  # moderate
        elif e < 5.0:
            self.impairment_level = 0.55 + (e - 2.0) * 0.15  # severe
        else:
            self.impairment_level = 1.0
            self.incapacitated = True

        self.speed_factor = max(0.0, 1.0 - self.impairment_level * 0.8)
        self.vision_factor = max(0.1, 1.0 - self.impairment_level * 0.6)
        self.rationality_factor = max(0.0, 1.0 - self.impairment_level * 0.5)


@dataclass
class CognitiveAgent:
    """ITED cognitive agent with BDI decision-making and physiological model."""
    id: int
    pos: np.ndarray  # [x, y] float coordinates
    base_speed: float = 1.34  # m/s
    has_evacuated: bool = False
    release_step: int = 0
    cohort_name: str = "baseline"
    group_id: Optional[int] = None
    leader_id: Optional[int] = None
    assisted_agent_id: Optional[int] = None

    # navigation
    current_path: List[Tuple[int, int]] = field(default_factory=list)
    path_index: int = 0
    target_exit: Optional[Tuple[int, int]] = None

    # behavioral state
    state: str = "CALM"
    speed_multiplier: float = 1.0
    crowd_speed_factor: float = 1.0
    current_speed: float = 0.0
    local_density: float = 0.0
    travel_time_s: float = 0.0
    last_navigation_step: int = -9999
    evacuated_via: Optional[str] = None

    # ITED: information
    beliefs: BeliefVector = field(default_factory=BeliefVector)
    familiarity: float = 0.5  # [0,1] how well agent knows the environment
    credibility: float = 0.5  # [0,1] how much other agents trust this one
    gossip_radius: float = 2.0

    # ITED: cognition (BDI)
    intention: str = INTENTION_EVACUATE
    rationality: float = 1.0  # effective rationality (base * physiology)
    base_rationality: float = 0.8
    explore_direction: Optional[np.ndarray] = None  # random walk direction when exploring
    explore_steps_remaining: int = 0

    # ITED: physiology
    physiology: PhysiologyState = field(default_factory=PhysiologyState)
    hazard_exposure: float = 0.0
    current_hazard_load: float = 0.0
    hazard_speed_factor: float = 1.0
    hazard_risk: float = 0.0

    # ITED: observation
    base_vision_radius: float = 5.0
    vision_radius: float = 5.0  # effective vision (base * physiology * smoke)

    def speed(self) -> float:
        if self.physiology.incapacitated:
            return 0.0
        return (
            self.base_speed
            * self.speed_multiplier
            * self.crowd_speed_factor
            * self.physiology.speed_factor
        )

    def is_released(self, simulation) -> bool:
        return simulation.current_step >= self.release_step

    def effective_vision_radius(self) -> float:
        return self.base_vision_radius * self.physiology.vision_factor

    def update_physiology(self, hazard_load: float, dt: float) -> None:
        """Update physiological state from hazard exposure."""
        self.current_hazard_load = hazard_load
        self.hazard_exposure += hazard_load * dt
        self.hazard_risk = max(self.hazard_risk, hazard_load)
        self.physiology.update_from_exposure(hazard_load, dt)
        self.rationality = self.base_rationality * self.physiology.rationality_factor
        self.vision_radius = self.effective_vision_radius()
        self.hazard_speed_factor = self.physiology.speed_factor

    def update_intention(self, simulation) -> None:
        """BDI decision cycle: update intention based on beliefs and state."""
        if self.physiology.incapacitated:
            self.intention = INTENTION_FREEZE
            return

        if self.state == "FROZEN":
            self.intention = INTENTION_FREEZE
            return

        if self.assisted_agent_id is not None:
            partner = simulation.agent_lookup.get(self.assisted_agent_id)
            if partner and not partner.has_evacuated and partner.is_released(simulation):
                self.intention = INTENTION_ASSIST
                return

        # check if we know any exits
        known_exits = self.beliefs.known_exits()
        if known_exits:
            self.intention = INTENTION_EVACUATE
            # pick the best exit from beliefs
            best = self.beliefs.best_exit()
            if best and best != self.target_exit:
                self.target_exit = best
                self.current_path = []  # force replan
                self.path_index = 0
        elif self.rationality < 0.4:
            # low rationality + no known exits → follow herd
            self.intention = INTENTION_FOLLOW
        else:
            # rational but no known exits → explore
            self.intention = INTENTION_EXPLORE

    def update_navigation(self, navigator, simulation) -> None:
        """Refresh navigation path based on current intention and beliefs."""
        if self.has_evacuated or not self.is_released(simulation):
            return

        if self.intention == INTENTION_FREEZE:
            return

        if self.intention == INTENTION_EXPLORE:
            self._navigate_explore(simulation)
            return

        if self.intention == INTENTION_FOLLOW:
            self._navigate_follow(simulation)
            return

        # EVACUATE or ASSIST: path to target
        needs_path = (not self.current_path) or (self.path_index >= len(self.current_path))
        stale_path = (
            simulation.current_step - self.last_navigation_step
            >= simulation.navigation_replan_interval_steps
        )
        congestion_trigger = (
            self.local_density >= simulation.navigation_density_reroute_threshold
            and simulation.current_step > self.last_navigation_step
        )

        if not (needs_path or self.target_exit is None or stale_path or congestion_trigger):
            return

        # use only believed exits for pathfinding
        known_exits = self.beliefs.known_exits()
        if not known_exits:
            # fallback: try all actual exits (degenerate case compatibility)
            known_exits = [tuple(e.pos) if hasattr(e, "pos") else e for e in simulation.exits]

        exit_coords = [tuple(map(int, ex)) for ex in known_exits]
        start = (int(round(self.pos[0])), int(round(self.pos[1])))

        # pathfind using belief-weighted navigator
        path = navigator.find_optimal_path(
            start, exit_coords,
            hazard_beliefs=self.beliefs.hazard_beliefs if hasattr(navigator, '_belief_weight') else None,
        )
        if path:
            self.current_path = path
            self.path_index = 0
            self.target_exit = path[-1]
            self.last_navigation_step = simulation.current_step

    def _navigate_explore(self, simulation) -> None:
        """Random walk with wall-following when no exits are known."""
        if self.explore_steps_remaining <= 0 or self.explore_direction is None:
            # pick a new random direction
            angle = np.random.uniform(0, 2 * np.pi)
            self.explore_direction = np.array([np.cos(angle), np.sin(angle)])
            self.explore_steps_remaining = np.random.randint(10, 30)
        self.explore_steps_remaining -= 1

    def _navigate_follow(self, simulation) -> None:
        """Follow the average direction of nearby agents."""
        if simulation.spatial_index is None:
            return
        neighbors = simulation.spatial_index.neighbor_agents(self.pos, radius=3.0)
        if not neighbors:
            self._navigate_explore(simulation) # fallback to exploring
            return

        # follow the average velocity of nearby agents (herding)
        avg_direction = np.zeros(2)
        count = 0
        for other in neighbors:
            if hasattr(other, 'current_path') and other.current_path and other.path_index < len(other.current_path):
                wp = other.current_path[other.path_index]
                d = np.array([wp[0] + 0.5, wp[1] + 0.5]) - other.pos
                norm = np.linalg.norm(d)
                if norm > 1e-6:
                    avg_direction += d / norm
                    count += 1
        if count > 0:
            avg_direction /= count
            # create a synthetic path a few steps in that direction
            target = self.pos + avg_direction * 3.0
            tx, ty = int(round(target[0])), int(round(target[1]))
            if simulation.layout.is_walkable((tx, ty)):
                self.current_path = [(tx, ty)]
                self.path_index = 0

    def step(self, dt: float, simulation) -> None:
        if self.has_evacuated or not self.is_released(simulation):
            return
        if self.physiology.incapacitated or self.intention == INTENTION_FREEZE:
            self.current_speed = 0.0
            return

        waypoint = None

        # exploration mode: use explore_direction
        if self.intention == INTENTION_EXPLORE and self.explore_direction is not None:
            desired_step = self.explore_direction * self.speed() * dt
            neighbors = (
                simulation.spatial_index.neighbor_positions(self.pos, radius=1.0)
                if simulation.spatial_index is not None
                else np.zeros((0, 2), dtype=float)
            )
            adjusted = adjusted_step(
                current_pos=self.pos,
                desired_step=desired_step,
                neighbors=neighbors,
                walls=[],
                dt=dt,
            )
            new_pos = self.pos + adjusted
            nx, ny = int(round(new_pos[0])), int(round(new_pos[1]))
            if simulation.layout.is_walkable((nx, ny)):
                self.pos = new_pos
            else:
                # bounce off wall — reverse direction
                self.explore_direction = -self.explore_direction
                self.explore_steps_remaining = 0
            return

        # path-following mode (EVACUATE, FOLLOW, ASSIST)
        while self.current_path and self.path_index < len(self.current_path):
            candidate = self.current_path[self.path_index]
            target = np.array([candidate[0] + 0.5, candidate[1] + 0.5], dtype=float)
            if np.linalg.norm(target - self.pos) < 0.2:
                self.path_index += 1
                continue
            waypoint = candidate
            break

        if waypoint is not None:
            target = np.array([waypoint[0] + 0.5, waypoint[1] + 0.5], dtype=float)
            direction = target - self.pos
            dist = np.linalg.norm(direction)
            if dist > 1e-6:
                direction = direction / dist

            # leader following
            if self.leader_id is not None:
                leader = simulation.agent_lookup.get(self.leader_id)
                if leader is not None and leader.is_released(simulation) and not leader.has_evacuated:
                    leader_delta = leader.pos - self.pos
                    leader_dist = np.linalg.norm(leader_delta)
                    if leader_dist > 1.5:
                        direction = 0.65 * direction + 0.35 * (leader_delta / leader_dist)
                        dir_norm = np.linalg.norm(direction)
                        if dir_norm > 1e-6:
                            direction = direction / dir_norm

            # assist partner
            if self.assisted_agent_id is not None:
                partner = simulation.agent_lookup.get(self.assisted_agent_id)
                if partner is not None and partner.is_released(simulation) and not partner.has_evacuated:
                    partner_delta = partner.pos - self.pos
                    partner_dist = np.linalg.norm(partner_delta)
                    if partner_dist > 2.5:
                        direction = partner_delta / max(partner_dist, 1e-6)

            desired_step = direction * self.speed() * dt
            neighbors = (
                simulation.spatial_index.neighbor_positions(self.pos, radius=1.0)
                if simulation.spatial_index is not None
                else np.zeros((0, 2), dtype=float)
            )
            adjusted = adjusted_step(
                current_pos=self.pos,
                desired_step=desired_step,
                neighbors=neighbors,
                walls=[],
                dt=dt,
            )
            new_pos = self.pos + adjusted

            if np.linalg.norm(target - new_pos) < 0.2:
                self.path_index += 1

            if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                self.pos = new_pos
        else:
            # no path — fallback to nearest known exit
            exits = self.beliefs.known_exits() or simulation.layout.exit_positions()
            if exits:
                goal = np.array(exits[0], dtype=float)
                direction = goal - self.pos
                dist = np.linalg.norm(direction)
                if dist > 1e-6:
                    direction = direction / dist
                noise = np.random.randn(2) * 0.03
                desired_step = (direction + noise) * self.speed() * dt
                neighbors = (
                    simulation.spatial_index.neighbor_positions(self.pos, radius=1.0)
                    if simulation.spatial_index is not None
                    else np.zeros((0, 2), dtype=float)
                )
                adjusted = adjusted_step(
                    current_pos=self.pos,
                    desired_step=desired_step,
                    neighbors=neighbors,
                    walls=[],
                    dt=dt,
                )
                new_pos = self.pos + adjusted
                if simulation.layout.is_walkable((int(round(new_pos[0])), int(round(new_pos[1])))):
                    self.pos = new_pos
