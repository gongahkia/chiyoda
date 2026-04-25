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

# SFM parameters (Helbing & Molnar 1995, calibrated)
TAU = 0.5           # relaxation time (s)
A_AGENT = 2.1       # agent repulsion strength
B_AGENT = 0.3       # agent repulsion range
A_WALL = 5.0        # wall repulsion strength
B_WALL = 0.2        # wall repulsion range
COUNTER_FLOW_K = 1.5 # friction coefficient for opposing flow


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
    # driving force: tendency toward desired velocity
    f_drive = (desired_velocity - current_velocity) / TAU

    # agent-agent repulsion (exponential)
    f_agents = np.zeros(2)
    for i, n_pos in enumerate(neighbors):
        delta = current_pos - n_pos
        dist = np.linalg.norm(delta) + 1e-6
        if dist < 3.0: # only consider nearby agents
            n_hat = delta / dist
            f_repel = A_AGENT * np.exp((0.6 - dist) / B_AGENT) * n_hat
            f_agents += f_repel

            # counter-flow friction: if neighbor moving in opposite direction
            if counter_flow and neighbor_velocities is not None and i < len(neighbor_velocities):
                n_vel = neighbor_velocities[i]
                dot = np.dot(desired_velocity, n_vel)
                if dot < 0 and np.linalg.norm(n_vel) > 0.1: # opposing flow
                    tangent = np.array([-n_hat[1], n_hat[0]])
                    f_friction = COUNTER_FLOW_K * abs(dot) * tangent * np.sign(np.dot(tangent, desired_velocity))
                    f_agents += f_friction

    # wall repulsion (simplified — uses wall positions if provided)
    f_walls = np.zeros(2)
    if walls:
        for wall_pos in walls:
            delta = current_pos - np.array(wall_pos, dtype=float)
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
    current_velocity = desired_velocity * 0.8 # approximate current velocity

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
