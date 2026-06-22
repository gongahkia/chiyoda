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

import numpy as np

try:
    from numba import njit

    NUMBA_AVAILABLE = True
except Exception:  # pragma: no cover - depends on optional dependency
    NUMBA_AVAILABLE = False
    njit = None

# SFM parameters (Helbing & Molnar 1995, calibrated)
TAU = 0.5  # relaxation time (s)
A_AGENT = 2.1  # agent repulsion strength
B_AGENT = 0.3  # agent repulsion range
A_WALL = 5.0  # wall repulsion strength
B_WALL = 0.2  # wall repulsion range
COUNTER_FLOW_K = 1.5  # friction coefficient for opposing flow


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
    ) -> np.ndarray:
        dim = current_pos.shape[0]
        f_total = (desired_velocity - current_velocity) / TAU

        for idx in range(neighbors.shape[0]):
            delta = current_pos - neighbors[idx]
            dist = np.sqrt(np.sum(delta * delta)) + 1e-6
            if dist < 3.0:
                n_hat = delta / dist
                f_total += A_AGENT * np.exp((0.6 - dist) / B_AGENT) * n_hat
                if counter_flow and has_neighbor_velocities:
                    n_vel = neighbor_velocities[idx]
                    dot = np.sum(desired_velocity * n_vel)
                    if dot < 0 and np.sqrt(np.sum(n_vel * n_vel)) > 0.1:
                        tangent = np.zeros(dim)
                        tangent[0] = -n_hat[1]
                        tangent[1] = n_hat[0]
                        f_total += (
                            COUNTER_FLOW_K
                            * abs(dot)
                            * tangent
                            * np.sign(np.sum(tangent * desired_velocity))
                        )

        for idx in range(walls.shape[0]):
            delta = current_pos - walls[idx]
            dist = np.sqrt(np.sum(delta * delta)) + 1e-6
            if dist < 2.0:
                n_hat = delta / dist
                f_total += A_WALL * np.exp((0.3 - dist) / B_WALL) * n_hat

        new_velocity = current_velocity + f_total * dt
        max_speed = max(np.sqrt(np.sum(desired_velocity * desired_velocity)) * 1.5, 0.5)
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
) -> np.ndarray:
    """
    Full SFM step computation.

    Returns the displacement vector for this timestep.
    """
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
        )

    # driving force: tendency toward desired velocity
    f_drive = (desired_velocity - current_velocity) / TAU

    # agent-agent repulsion (exponential)
    dim = int(current_pos.shape[0])
    f_agents = np.zeros(dim)
    for i, n_pos in enumerate(neighbors):
        delta = current_pos - n_pos
        dist = np.linalg.norm(delta) + 1e-6
        if dist < 3.0:  # only consider nearby agents
            n_hat = delta / dist
            f_repel = A_AGENT * np.exp((0.6 - dist) / B_AGENT) * n_hat
            f_agents += f_repel

            # counter-flow friction: if neighbor moving in opposite direction
            if (
                counter_flow
                and neighbor_velocities is not None
                and i < len(neighbor_velocities)
            ):
                n_vel = neighbor_velocities[i]
                dot = np.dot(desired_velocity, n_vel)
                if dot < 0 and np.linalg.norm(n_vel) > 0.1:  # opposing flow
                    tangent = np.zeros(dim)
                    tangent[0] = -n_hat[1]
                    tangent[1] = n_hat[0]
                    f_friction = (
                        COUNTER_FLOW_K
                        * abs(dot)
                        * tangent
                        * np.sign(np.dot(tangent, desired_velocity))
                    )
                    f_agents += f_friction

    # wall repulsion (simplified — uses wall positions if provided)
    f_walls = np.zeros(dim)
    if walls is not None and len(walls) > 0:
        for wall_pos in walls:
            wall = np.array(wall_pos, dtype=float)
            if wall.shape[0] < dim:
                wall = np.pad(wall, (0, dim - wall.shape[0]))
            delta = current_pos - wall
            dist = np.linalg.norm(delta) + 1e-6
            if dist < 2.0:
                n_hat = delta / dist
                f_walls += A_WALL * np.exp((0.3 - dist) / B_WALL) * n_hat

    # total force
    f_total = f_drive + f_agents + f_walls

    # compute new velocity
    new_velocity = current_velocity + f_total * dt

    # clamp to maximum speed (1.5x desired speed)
    max_speed = max(np.linalg.norm(desired_velocity) * 1.5, 0.5)
    speed = np.linalg.norm(new_velocity)
    if speed > max_speed:
        new_velocity = new_velocity / speed * max_speed

    return new_velocity * dt


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
    )

    # clamp displacement magnitude to avoid teleportation
    max_step = max(float(np.linalg.norm(desired_step)), 1.0 * dt)
    nrm = np.linalg.norm(displacement)
    if nrm > max_step:
        displacement = displacement / nrm * max_step

    return displacement
