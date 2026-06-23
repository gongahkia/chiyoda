"""
Social Force Model v2 for ITED framework.

Proper SFM implementation with:
- desired velocity driving force
- exponential agent-agent repulsion
- wall repulsion
- counter-flow friction (opposing desired directions)
- panic force amplification
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
import yaml

try:
    from numba import njit

    NUMBA_AVAILABLE = True
except Exception:  # pragma: no cover - depends on optional dependency
    NUMBA_AVAILABLE = False
    njit = None

# SFM parameters (legacy Chiyoda defaults)
TAU = 0.5  # relaxation time (s)
A_AGENT = 2.1  # agent repulsion strength
B_AGENT = 0.3  # agent repulsion range
A_WALL = 5.0  # wall repulsion strength
B_WALL = 0.2  # wall repulsion range
COUNTER_FLOW_K = 1.5  # friction coefficient for opposing flow
BODY_DIAMETER = 0.6  # combined pedestrian body diameter in repulsion term
WALL_DISTANCE = 0.3  # wall contact distance in repulsion term
AGENT_INTERACTION_RADIUS = 3.0
WALL_INTERACTION_RADIUS = 2.0
MAX_SPEED_MULTIPLIER = 1.5
BASE_VISION_RADIUS = 5.0
VISUAL_RANGE = 0.0
VISUAL_FIELD_DEGREES = 360.0
REAR_REPULSION_WEIGHT = 1.0
COUNTER_FLOW_AVOIDANCE_K = 0.0
COUNTER_FLOW_AVOIDANCE_RANGE = 2.0
_CALIBRATION_DIR = Path(__file__).resolve().parents[2] / "data" / "sfm_calibrations"


@dataclass(frozen=True)
class SocialForceCalibration:
    profile: str
    desired_speed_mps: float
    relaxation_time_s: float
    agent_repulsion_strength: float
    agent_repulsion_range_m: float
    wall_repulsion_strength: float
    wall_repulsion_range_m: float
    counter_flow_friction: float
    body_diameter_m: float = BODY_DIAMETER
    wall_distance_m: float = WALL_DISTANCE
    agent_interaction_radius_m: float = AGENT_INTERACTION_RADIUS
    wall_interaction_radius_m: float = WALL_INTERACTION_RADIUS
    max_speed_multiplier: float = MAX_SPEED_MULTIPLIER
    base_vision_radius_m: float = BASE_VISION_RADIUS
    visual_range_m: float = VISUAL_RANGE
    visual_field_degrees: float = VISUAL_FIELD_DEGREES
    rear_repulsion_weight: float = REAR_REPULSION_WEIGHT
    counter_flow_avoidance_strength: float = COUNTER_FLOW_AVOIDANCE_K
    counter_flow_avoidance_range_m: float = COUNTER_FLOW_AVOIDANCE_RANGE
    provenance: Mapping[str, Any] | None = None

    def with_overrides(self, values: Mapping[str, Any]) -> SocialForceCalibration:
        data = self.to_parameters()
        for key, value in values.items():
            if key not in data:
                raise ValueError(f"Unknown social force calibration parameter: {key}")
            data[key] = _parameter_value(value)
        return SocialForceCalibration(
            profile=self.profile,
            provenance=self.provenance,
            **data,
        )

    def to_parameters(self) -> dict[str, float]:
        return {
            "desired_speed_mps": float(self.desired_speed_mps),
            "relaxation_time_s": float(self.relaxation_time_s),
            "agent_repulsion_strength": float(self.agent_repulsion_strength),
            "agent_repulsion_range_m": float(self.agent_repulsion_range_m),
            "wall_repulsion_strength": float(self.wall_repulsion_strength),
            "wall_repulsion_range_m": float(self.wall_repulsion_range_m),
            "counter_flow_friction": float(self.counter_flow_friction),
            "body_diameter_m": float(self.body_diameter_m),
            "wall_distance_m": float(self.wall_distance_m),
            "agent_interaction_radius_m": float(self.agent_interaction_radius_m),
            "wall_interaction_radius_m": float(self.wall_interaction_radius_m),
            "max_speed_multiplier": float(self.max_speed_multiplier),
            "base_vision_radius_m": float(self.base_vision_radius_m),
            "visual_range_m": float(self.visual_range_m),
            "visual_field_degrees": float(self.visual_field_degrees),
            "rear_repulsion_weight": float(self.rear_repulsion_weight),
            "counter_flow_avoidance_strength": float(
                self.counter_flow_avoidance_strength
            ),
            "counter_flow_avoidance_range_m": float(
                self.counter_flow_avoidance_range_m
            ),
        }

    def provenance_for(self, parameter: str) -> Any:
        provenance = self.provenance or {}
        return provenance.get(parameter)


GENERIC_LEGACY = SocialForceCalibration(
    profile="generic_legacy",
    desired_speed_mps=1.34,
    relaxation_time_s=TAU,
    agent_repulsion_strength=A_AGENT,
    agent_repulsion_range_m=B_AGENT,
    wall_repulsion_strength=A_WALL,
    wall_repulsion_range_m=B_WALL,
    counter_flow_friction=COUNTER_FLOW_K,
)

YOLOV5_MDPI_2024 = SocialForceCalibration(
    profile="yolov5_mdpi_2024",
    desired_speed_mps=1.37,
    relaxation_time_s=0.53,
    agent_repulsion_strength=10.25,
    agent_repulsion_range_m=0.28,
    wall_repulsion_strength=A_WALL,
    wall_repulsion_range_m=B_WALL,
    counter_flow_friction=COUNTER_FLOW_K,
)

_EMBEDDED_PROFILES = {
    GENERIC_LEGACY.profile: GENERIC_LEGACY,
    YOLOV5_MDPI_2024.profile: YOLOV5_MDPI_2024,
}


def load_social_force_calibration(config: str | Mapping[str, Any] | None = None):
    if config is None:
        profile = "generic_legacy"
        overrides: Mapping[str, Any] = {}
    elif isinstance(config, str):
        profile = config
        overrides = {}
    else:
        profile = str(config.get("profile", config.get("name", "generic_legacy")))
        overrides = config.get("parameters", {}) or {}
    calibration = _load_profile(profile)
    return calibration.with_overrides(overrides) if overrides else calibration


def _load_profile(profile: str) -> SocialForceCalibration:
    path = _CALIBRATION_DIR / f"{profile}.yaml"
    if not path.exists():
        if profile in _EMBEDDED_PROFILES:
            return _EMBEDDED_PROFILES[profile]
        raise ValueError(f"Unknown social force calibration profile: {profile}")
    payload = yaml.safe_load(path.read_text()) or {}
    parameters = payload.get("parameters", {}) or {}
    values = {key: _parameter_value(value) for key, value in parameters.items()}
    base = _EMBEDDED_PROFILES.get(profile, GENERIC_LEGACY)
    merged = base.to_parameters()
    merged.update(values)
    provenance = {
        key: value.get("provenance")
        for key, value in parameters.items()
        if isinstance(value, Mapping)
    }
    return SocialForceCalibration(
        profile=str(payload.get("profile", profile)),
        provenance=provenance,
        **merged,
    )


def _parameter_value(value: Any) -> float:
    if isinstance(value, Mapping):
        value = value.get("value")
    return float(value)


if NUMBA_AVAILABLE:

    @njit(cache=True)
    def _social_force_step_njit(
        current_pos: np.ndarray,
        desired_velocity: np.ndarray,
        current_velocity: np.ndarray,
        neighbors: np.ndarray,
        neighbor_velocities: np.ndarray,
        walls: np.ndarray,
        dt: float,
        counter_flow: bool,
        has_neighbor_velocities: bool,
        relaxation_time_s: float,
        agent_repulsion_strength: float,
        agent_repulsion_range_m: float,
        wall_repulsion_strength: float,
        wall_repulsion_range_m: float,
        counter_flow_friction: float,
        body_diameter_m: float,
        wall_distance_m: float,
        agent_interaction_radius_m: float,
        wall_interaction_radius_m: float,
        max_speed_multiplier: float,
        visual_range_m: float,
        visual_field_cosine: float,
        rear_repulsion_weight: float,
        counter_flow_avoidance_strength: float,
        counter_flow_avoidance_range_m: float,
    ) -> np.ndarray:
        dim = current_pos.shape[0]
        f_total = (desired_velocity - current_velocity) / relaxation_time_s
        desired_speed = np.sqrt(np.sum(desired_velocity * desired_velocity))

        for idx in range(neighbors.shape[0]):
            delta = current_pos - neighbors[idx]
            dist = np.sqrt(np.sum(delta * delta)) + 1e-6
            if dist < agent_interaction_radius_m:
                if visual_range_m > 0.0 and dist > visual_range_m:
                    continue
                visual_weight = 1.0
                if desired_speed > 1e-6:
                    to_neighbor = -delta / dist
                    cos_angle = np.sum(desired_velocity * to_neighbor) / desired_speed
                    if cos_angle < visual_field_cosine:
                        visual_weight = rear_repulsion_weight
                n_hat = delta / dist
                f_total += (
                    visual_weight *
                    agent_repulsion_strength
                    * np.exp((body_diameter_m - dist) / agent_repulsion_range_m)
                    * n_hat
                )
                if counter_flow and has_neighbor_velocities:
                    n_vel = neighbor_velocities[idx]
                    dot = np.sum(desired_velocity * n_vel)
                    n_speed = np.sqrt(np.sum(n_vel * n_vel))
                    if dot < 0 and n_speed > 0.1:
                        tangent = np.zeros(dim)
                        tangent[0] = -n_hat[1]
                        tangent[1] = n_hat[0]
                        f_total += (
                            visual_weight *
                            counter_flow_friction
                            * abs(dot)
                            * tangent
                            * np.sign(np.sum(tangent * desired_velocity))
                        )
                        if (
                            desired_speed > 1e-6
                            and counter_flow_avoidance_strength > 0.0
                            and counter_flow_avoidance_range_m > 1e-6
                        ):
                            lateral_axis = np.zeros(dim)
                            lateral_axis[0] = -desired_velocity[1] / desired_speed
                            lateral_axis[1] = desired_velocity[0] / desired_speed
                            lateral_offset = np.sum(delta * lateral_axis)
                            lateral_sign = 1.0
                            if abs(lateral_offset) > 1e-6:
                                lateral_sign = np.sign(lateral_offset)
                            approach = min(1.0, -dot / (desired_speed * n_speed))
                            f_total += (
                                visual_weight
                                * counter_flow_avoidance_strength
                                * approach
                                * np.exp(
                                    (counter_flow_avoidance_range_m - dist)
                                    / counter_flow_avoidance_range_m
                                )
                                * lateral_sign
                                * lateral_axis
                            )

        for idx in range(walls.shape[0]):
            delta = current_pos - walls[idx]
            dist = np.sqrt(np.sum(delta * delta)) + 1e-6
            if dist < wall_interaction_radius_m:
                n_hat = delta / dist
                f_total += (
                    wall_repulsion_strength
                    * np.exp((wall_distance_m - dist) / wall_repulsion_range_m)
                    * n_hat
                )

        new_velocity = current_velocity + f_total * dt
        max_speed = max(
            np.sqrt(np.sum(desired_velocity * desired_velocity))
            * max_speed_multiplier,
            0.5,
        )
        speed = np.sqrt(np.sum(new_velocity * new_velocity))
        if speed > max_speed:
            new_velocity = new_velocity / speed * max_speed
        return new_velocity * dt

else:
    _social_force_step_njit = None


def social_force_step(
    current_pos: np.ndarray,
    desired_velocity: np.ndarray,
    current_velocity: np.ndarray,
    neighbors: np.ndarray,
    neighbor_velocities: np.ndarray | None = None,
    walls: list | None = None,
    dt: float = 0.1,
    counter_flow: bool = False,
    parameters: SocialForceCalibration | Mapping[str, Any] | None = None,
) -> np.ndarray:
    """
    Full SFM step computation.

    Returns the displacement vector for this timestep.
    """
    calibration = _coerce_calibration(parameters)
    params = calibration.to_parameters()
    visual_field_cosine = float(
        np.cos(np.deg2rad(params["visual_field_degrees"] * 0.5))
    )
    if _social_force_step_njit is not None:
        dim = int(current_pos.shape[0])
        return _social_force_step_njit(
            np.asarray(current_pos, dtype=float),
            np.asarray(desired_velocity, dtype=float),
            np.asarray(current_velocity, dtype=float),
            np.asarray(neighbors, dtype=float),
            (
                np.asarray(neighbor_velocities, dtype=float)
                if neighbor_velocities is not None
                else np.zeros((0, dim), dtype=float)
            ),
            _walls_to_array(walls, dim),
            float(dt),
            bool(counter_flow),
            neighbor_velocities is not None,
            params["relaxation_time_s"],
            params["agent_repulsion_strength"],
            params["agent_repulsion_range_m"],
            params["wall_repulsion_strength"],
            params["wall_repulsion_range_m"],
            params["counter_flow_friction"],
            params["body_diameter_m"],
            params["wall_distance_m"],
            params["agent_interaction_radius_m"],
            params["wall_interaction_radius_m"],
            params["max_speed_multiplier"],
            params["visual_range_m"],
            visual_field_cosine,
            params["rear_repulsion_weight"],
            params["counter_flow_avoidance_strength"],
            params["counter_flow_avoidance_range_m"],
        )

    # driving force: tendency toward desired velocity
    f_drive = (desired_velocity - current_velocity) / params["relaxation_time_s"]

    # agent-agent repulsion (exponential)
    dim = int(current_pos.shape[0])
    f_agents = np.zeros(dim)
    desired_speed = np.linalg.norm(desired_velocity)
    for i, n_pos in enumerate(neighbors):
        delta = current_pos - n_pos
        dist = np.linalg.norm(delta) + 1e-6
        if dist < params["agent_interaction_radius_m"]:
            if params["visual_range_m"] > 0.0 and dist > params["visual_range_m"]:
                continue
            visual_weight = 1.0
            if desired_speed > 1e-6:
                to_neighbor = -delta / dist
                cos_angle = float(np.dot(desired_velocity, to_neighbor) / desired_speed)
                if cos_angle < visual_field_cosine:
                    visual_weight = params["rear_repulsion_weight"]
            n_hat = delta / dist
            f_repel = (
                visual_weight *
                params["agent_repulsion_strength"]
                * np.exp(
                    (params["body_diameter_m"] - dist)
                    / params["agent_repulsion_range_m"]
                )
                * n_hat
            )
            f_agents += f_repel

            # counter-flow friction: if neighbor moving in opposite direction
            if (
                counter_flow
                and neighbor_velocities is not None
                and i < len(neighbor_velocities)
            ):
                n_vel = neighbor_velocities[i]
                dot = np.dot(desired_velocity, n_vel)
                n_speed = np.linalg.norm(n_vel)
                if dot < 0 and n_speed > 0.1:  # opposing flow
                    tangent = np.zeros(dim)
                    tangent[0] = -n_hat[1]
                    tangent[1] = n_hat[0]
                    f_friction = (
                        visual_weight *
                        params["counter_flow_friction"]
                        * abs(dot)
                        * tangent
                        * np.sign(np.dot(tangent, desired_velocity))
                    )
                    f_agents += f_friction
                    if (
                        desired_speed > 1e-6
                        and params["counter_flow_avoidance_strength"] > 0.0
                        and params["counter_flow_avoidance_range_m"] > 1e-6
                    ):
                        lateral_axis = np.zeros(dim)
                        lateral_axis[0] = -desired_velocity[1] / desired_speed
                        lateral_axis[1] = desired_velocity[0] / desired_speed
                        lateral_offset = float(np.dot(delta, lateral_axis))
                        lateral_sign = (
                            np.sign(lateral_offset)
                            if abs(lateral_offset) > 1e-6
                            else 1.0
                        )
                        approach = min(1.0, -dot / (desired_speed * n_speed))
                        f_agents += (
                            visual_weight
                            * params["counter_flow_avoidance_strength"]
                            * approach
                            * np.exp(
                                (
                                    params["counter_flow_avoidance_range_m"]
                                    - dist
                                )
                                / params["counter_flow_avoidance_range_m"]
                            )
                            * lateral_sign
                            * lateral_axis
                        )

    # wall repulsion (simplified — uses wall positions if provided)
    f_walls = np.zeros(dim)
    if walls is not None and len(walls) > 0:
        for wall_pos in walls:
            wall = np.array(wall_pos, dtype=float)
            if wall.shape[0] < dim:
                wall = np.pad(wall, (0, dim - wall.shape[0]))
            delta = current_pos - wall
            dist = np.linalg.norm(delta) + 1e-6
            if dist < params["wall_interaction_radius_m"]:
                n_hat = delta / dist
                f_walls += (
                    params["wall_repulsion_strength"]
                    * np.exp(
                        (params["wall_distance_m"] - dist)
                        / params["wall_repulsion_range_m"]
                    )
                    * n_hat
                )

    # total force
    f_total = f_drive + f_agents + f_walls

    # compute new velocity
    new_velocity = current_velocity + f_total * dt

    # clamp to maximum speed (1.5x desired speed)
    max_speed = max(
        np.linalg.norm(desired_velocity) * params["max_speed_multiplier"], 0.5
    )
    speed = np.linalg.norm(new_velocity)
    if speed > max_speed:
        new_velocity = new_velocity / speed * max_speed

    return new_velocity * dt


def _coerce_calibration(
    parameters: SocialForceCalibration | Mapping[str, Any] | None,
) -> SocialForceCalibration:
    if parameters is None:
        return GENERIC_LEGACY
    if isinstance(parameters, SocialForceCalibration):
        return parameters
    return GENERIC_LEGACY.with_overrides(parameters)


def _walls_to_array(walls: list | None, dim: int) -> np.ndarray:
    if walls is None or len(walls) == 0:
        return np.zeros((0, dim), dtype=float)
    wall_array = np.zeros((len(walls), dim), dtype=float)
    for idx, wall_pos in enumerate(walls):
        wall = np.asarray(wall_pos, dtype=float)
        limit = min(dim, int(wall.shape[0]))
        wall_array[idx, :limit] = wall[:limit]
    return wall_array


def adjusted_step(
    current_pos: np.ndarray,
    desired_step: np.ndarray,
    neighbors: np.ndarray,
    walls: list,
    dt: float,
    counter_flow: bool = False,
    parameters: SocialForceCalibration | Mapping[str, Any] | None = None,
) -> np.ndarray:
    """
    Backward-compatible wrapper around the full SFM.

    Provides the same interface as v1 for existing code that calls adjusted_step.
    """
    desired_velocity = desired_step / dt if dt > 1e-9 else desired_step
    current_velocity = desired_velocity * 0.8  # approximate current velocity

    displacement = social_force_step(
        current_pos=current_pos,
        desired_velocity=desired_velocity,
        current_velocity=current_velocity,
        neighbors=neighbors,
        neighbor_velocities=None,
        walls=walls,
        dt=dt,
        counter_flow=counter_flow,
        parameters=parameters,
    )

    # clamp displacement magnitude to avoid teleportation
    max_step = max(float(np.linalg.norm(desired_step)), 1.0 * dt)
    nrm = np.linalg.norm(displacement)
    if nrm > max_step:
        displacement = displacement / nrm * max_step

    return displacement
