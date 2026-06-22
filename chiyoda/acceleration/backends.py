from __future__ import annotations

import json
import warnings
from dataclasses import dataclass
from pathlib import Path

import numpy as np


@dataclass
class AccelerationBackend:
    name: str
    requested_backend: str
    fallback_reason: str | None = None

    def aggregate_step_grids(
        self,
        width: int,
        height: int,
        positions: np.ndarray,
        densities: np.ndarray,
        speeds: np.ndarray,
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
        raise NotImplementedError

    def hazard_intensities(
        self,
        positions: np.ndarray,
        hazard_positions: np.ndarray,
        radii: np.ndarray,
        severities: np.ndarray,
    ) -> np.ndarray:
        raise NotImplementedError

    def social_force_steps(
        self,
        current_positions: np.ndarray,
        desired_velocities: np.ndarray,
        current_velocities: np.ndarray,
        neighbor_positions: np.ndarray,
        neighbor_counts: np.ndarray,
        neighbor_velocities: np.ndarray | None = None,
        walls: np.ndarray | None = None,
        dt: float = 0.1,
        counter_flow: bool = False,
    ) -> np.ndarray:
        raise NotImplementedError


class PythonAccelerationBackend(AccelerationBackend):
    def __init__(
        self, requested_backend: str = "python", fallback_reason: str | None = None
    ) -> None:
        super().__init__(
            name="python",
            requested_backend=requested_backend,
            fallback_reason=fallback_reason,
        )

    def aggregate_step_grids(
        self,
        width: int,
        height: int,
        positions: np.ndarray,
        densities: np.ndarray,
        speeds: np.ndarray,
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
        occupancy = np.zeros((height, width), dtype=int)
        density_sum = np.zeros((height, width), dtype=float)
        speed_sum = np.zeros((height, width), dtype=float)

        if positions.size == 0:
            return occupancy, density_sum, speed_sum

        xs = np.clip(np.rint(positions[:, 0]).astype(int), 0, width - 1)
        ys = np.clip(np.rint(positions[:, 1]).astype(int), 0, height - 1)

        np.add.at(occupancy, (ys, xs), 1)
        np.add.at(density_sum, (ys, xs), densities)
        np.add.at(speed_sum, (ys, xs), speeds)

        density_grid = np.divide(
            density_sum,
            occupancy,
            out=np.zeros_like(density_sum),
            where=occupancy > 0,
        )
        speed_grid = np.divide(
            speed_sum,
            occupancy,
            out=np.zeros_like(speed_sum),
            where=occupancy > 0,
        )
        return occupancy, density_grid, speed_grid

    def hazard_intensities(
        self,
        positions: np.ndarray,
        hazard_positions: np.ndarray,
        radii: np.ndarray,
        severities: np.ndarray,
    ) -> np.ndarray:
        if positions.size == 0:
            return np.zeros((0,), dtype=float)
        if hazard_positions.size == 0:
            return np.zeros((positions.shape[0],), dtype=float)

        deltas = positions[:, None, :] - hazard_positions[None, :, :]
        distances = np.linalg.norm(deltas, axis=2)
        intensities = np.zeros((positions.shape[0],), dtype=float)

        finite_radii = radii > 1e-6
        if np.any(finite_radii):
            weighted = severities[finite_radii] * np.clip(
                1.0 - (distances[:, finite_radii] / radii[finite_radii]),
                a_min=0.0,
                a_max=None,
            )
            intensities += weighted.sum(axis=1)

        zero_radii = ~finite_radii
        if np.any(zero_radii):
            intensities += (
                severities[zero_radii] * (distances[:, zero_radii] <= 0.75)
            ).sum(axis=1)

        return intensities

    def social_force_steps(
        self,
        current_positions: np.ndarray,
        desired_velocities: np.ndarray,
        current_velocities: np.ndarray,
        neighbor_positions: np.ndarray,
        neighbor_counts: np.ndarray,
        neighbor_velocities: np.ndarray | None = None,
        walls: np.ndarray | None = None,
        dt: float = 0.1,
        counter_flow: bool = False,
    ) -> np.ndarray:
        from chiyoda.navigation.social_force import social_force_step

        positions = np.asarray(current_positions, dtype=float)
        desired = np.asarray(desired_velocities, dtype=float)
        current = np.asarray(current_velocities, dtype=float)
        neighbors = np.asarray(neighbor_positions, dtype=float)
        counts = np.asarray(neighbor_counts, dtype=int)
        velocities = (
            None
            if neighbor_velocities is None
            else np.asarray(neighbor_velocities, dtype=float)
        )
        wall_array = np.asarray(walls if walls is not None else [], dtype=float)
        results = np.zeros_like(positions, dtype=float)
        for idx in range(positions.shape[0]):
            count = int(counts[idx])
            agent_neighbors = neighbors[idx, :count, :]
            agent_neighbor_velocities = (
                None if velocities is None else velocities[idx, :count, :]
            )
            results[idx] = social_force_step(
                current_pos=positions[idx],
                desired_velocity=desired[idx],
                current_velocity=current[idx],
                neighbors=agent_neighbors,
                neighbor_velocities=agent_neighbor_velocities,
                walls=wall_array.tolist(),
                dt=dt,
                counter_flow=counter_flow,
            )
        return results


class JuliaAccelerationBackend(AccelerationBackend):
    def __init__(self, requested_backend: str = "julia") -> None:
        try:
            from juliacall import Main as jl
        except Exception as exc:  # pragma: no cover - exercised in fallback tests
            raise RuntimeError(f"Unable to import juliacall: {exc}") from exc

        super().__init__(
            name="julia", requested_backend=requested_backend, fallback_reason=None
        )
        module_path = Path(__file__).resolve().parent / "julia" / "telemetry.jl"
        jl.seval(f"include({json.dumps(str(module_path))})")
        self._module = jl.seval("ChiyodaAccel")

    def aggregate_step_grids(
        self,
        width: int,
        height: int,
        positions: np.ndarray,
        densities: np.ndarray,
        speeds: np.ndarray,
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
        occupancy, density_grid, speed_grid = self._module.aggregate_step_grids(
            int(width),
            int(height),
            np.asarray(positions, dtype=float),
            np.asarray(densities, dtype=float),
            np.asarray(speeds, dtype=float),
        )
        return np.asarray(occupancy), np.asarray(density_grid), np.asarray(speed_grid)

    def hazard_intensities(
        self,
        positions: np.ndarray,
        hazard_positions: np.ndarray,
        radii: np.ndarray,
        severities: np.ndarray,
    ) -> np.ndarray:
        return np.asarray(
            self._module.hazard_intensities(
                np.asarray(positions, dtype=float),
                np.asarray(hazard_positions, dtype=float),
                np.asarray(radii, dtype=float),
                np.asarray(severities, dtype=float),
            ),
            dtype=float,
        )

    def social_force_steps(
        self,
        current_positions: np.ndarray,
        desired_velocities: np.ndarray,
        current_velocities: np.ndarray,
        neighbor_positions: np.ndarray,
        neighbor_counts: np.ndarray,
        neighbor_velocities: np.ndarray | None = None,
        walls: np.ndarray | None = None,
        dt: float = 0.1,
        counter_flow: bool = False,
    ) -> np.ndarray:
        positions = np.asarray(current_positions, dtype=float)
        neighbors = np.asarray(neighbor_positions, dtype=float)
        velocities = (
            np.zeros_like(neighbors, dtype=float)
            if neighbor_velocities is None
            else np.asarray(neighbor_velocities, dtype=float)
        )
        wall_array = np.asarray(walls if walls is not None else [], dtype=float)
        if wall_array.size == 0:
            wall_array = np.zeros((0, positions.shape[1]), dtype=float)
        return np.asarray(
            self._module.social_force_steps(
                positions,
                np.asarray(desired_velocities, dtype=float),
                np.asarray(current_velocities, dtype=float),
                neighbors,
                np.asarray(neighbor_counts, dtype=int),
                velocities,
                wall_array,
                float(dt),
                bool(counter_flow),
            ),
            dtype=float,
        )


def create_acceleration_backend(preferred: str | None = None) -> AccelerationBackend:
    requested = str(preferred or "auto").lower()
    if requested not in {"auto", "python", "julia"}:
        raise ValueError("acceleration_backend must be one of auto, python, or julia")

    if requested == "python":
        return PythonAccelerationBackend(requested_backend=requested)

    try:
        return JuliaAccelerationBackend(requested_backend=requested)
    except Exception as exc:
        if requested == "julia":
            warnings.warn(
                f"Julia acceleration requested but unavailable; falling back to Python ({exc})",
                RuntimeWarning,
            )
        return PythonAccelerationBackend(
            requested_backend=requested,
            fallback_reason=str(exc),
        )
