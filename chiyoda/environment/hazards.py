"""
Multi-hazard physics engine with advection-diffusion, visibility effects,
and physiological impact tables for ITED CBRN scenarios.
"""

from __future__ import annotations

import csv
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np

# physiological effect profiles: (speed_factor, rationality_factor, vision_factor)
HAZARD_PROFILES = {
    "GAS": {  # nerve agent (sarin-like)
        "speed_decay": 0.8,  # strong motor impairment
        "vision_decay": 0.6,  # miosis (pupil constriction)
        "rationality_decay": 0.5,
        "incapacitation_threshold": 4.0,  # cumulative exposure units
    },
    "SMOKE": {  # obscurant
        "speed_decay": 0.3,  # mild physical effect
        "vision_decay": 0.9,  # severe visibility reduction
        "rationality_decay": 0.2,
        "incapacitation_threshold": 10.0,
    },
    "FIRE": {  # thermal
        "speed_decay": 0.5,
        "vision_decay": 0.4,
        "rationality_decay": 0.3,
        "incapacitation_threshold": 3.0,
    },
    "WILDFIRE": {
        "speed_decay": 0.55,
        "vision_decay": 0.5,
        "rationality_decay": 0.35,
        "incapacitation_threshold": 3.0,
    },
    "EMBER": {
        "speed_decay": 0.35,
        "vision_decay": 0.25,
        "rationality_decay": 0.25,
        "incapacitation_threshold": 4.0,
    },
    "FLOOD": {
        "speed_decay": 0.7,
        "vision_decay": 0.15,
        "rationality_decay": 0.25,
        "incapacitation_threshold": 4.0,
    },
    "EARTHQUAKE": {
        "speed_decay": 0.45,
        "vision_decay": 0.1,
        "rationality_decay": 0.55,
        "incapacitation_threshold": 3.5,
    },
    "AFTERSHOCK": {
        "speed_decay": 0.4,
        "vision_decay": 0.1,
        "rationality_decay": 0.45,
        "incapacitation_threshold": 4.0,
    },
    "CRUSH": {  # crowd crush (density-induced)
        "speed_decay": 0.9,
        "vision_decay": 0.1,
        "rationality_decay": 0.4,
        "incapacitation_threshold": 5.0,
    },
    "SHOOTER": {
        "speed_decay": 0.2,
        "vision_decay": 0.0,
        "rationality_decay": 0.7,
        "incapacitation_threshold": 2.0,
    },
}


@dataclass
class Hazard:
    pos: tuple[float, ...]
    kind: str
    radius: float = 0.0
    severity: float = 0.5
    spread_rate: float = 0.0
    wind_vector: tuple[float, float] = (0.0, 0.0)  # advection direction
    diffusion_rate: float = 0.1  # isotropic diffusion coefficient
    visibility_reduction: float = 0.0  # how much this hazard reduces visibility [0,1]
    range_m: float = 8.0
    accuracy: float = 0.35
    height_aware: bool = False
    layer_base_m: float | None = None
    layer_top_m: float | None = None
    vertical_decay_m: float = 1.0
    gas_density: float = 1.0
    ember_spotting_rate: float = 0.0
    ember_ignition_radius: float = 0.0
    ember_decay_rate: float = 0.15
    ember_cell_size: float = 1.0
    ember_origin: tuple[float, float] = (0.0, 0.0)
    ember_field: dict[tuple[int, int], float] = field(default_factory=dict)
    flow_vector: tuple[float, float] = (0.0, 0.0)
    inundation_depth_m: float = 0.0
    inundation_rise_rate_mps: float = 0.0
    inundation_decay_rate: float = 0.0
    flood_depth_threshold_m: float = 0.6
    max_depth_m: float = 2.0
    inundation_cell_size: float = 1.0
    inundation_origin: tuple[float, float] = (0.0, 0.0)
    inundation_field: dict[tuple[int, int], float] = field(default_factory=dict)
    aftershock_schedule: tuple[int, ...] = field(default_factory=tuple)
    aftershock_decay_rate: float = 0.12
    aftershock_damage_increment: float = 0.35
    damage_radius: float = 0.0
    re_evacuation_radius: float = 0.0
    aftershock_index: int = 0
    shock_intensity: float = 0.0
    active: bool = True

    def step(self, dt: float, simulation) -> None:
        if not self.active:
            return
        # advection-diffusion spread
        if self.kind.upper() in ("GAS", "SMOKE"):
            self.radius += self.spread_rate * dt + self.diffusion_rate * dt
            # advect center position
            z = float(self.pos[2]) if len(self.pos) >= 3 else 0.0
            self.pos = (
                self.pos[0] + self.wind_vector[0] * dt,
                self.pos[1] + self.wind_vector[1] * dt,
                z,
            )
        elif self.kind.upper() == "FIRE":
            self.radius += self.spread_rate * dt * 0.5  # fire spreads slower
        elif self.kind.upper() in {"WILDFIRE", "EMBER"}:
            wind_speed = float(np.linalg.norm(np.array(self.wind_vector, dtype=float)))
            self.radius += (self.spread_rate + 0.05 * wind_speed) * dt
            z = float(self.pos[2]) if len(self.pos) >= 3 else 0.0
            self.pos = (
                self.pos[0] + self.wind_vector[0] * dt * 0.15,
                self.pos[1] + self.wind_vector[1] * dt * 0.15,
                z,
            )
            self._step_ember_field(dt, simulation)
        elif self.kind.upper() == "FLOOD":
            flow = np.array(self.flow_vector or self.wind_vector, dtype=float)
            flow_speed = float(np.linalg.norm(flow))
            self.radius += (
                self.spread_rate + self.diffusion_rate + 0.03 * flow_speed
            ) * dt
            self.inundation_depth_m = min(
                self.max_depth_m,
                max(0.0, self.inundation_depth_m + self.inundation_rise_rate_mps * dt),
            )
            z = float(self.pos[2]) if len(self.pos) >= 3 else 0.0
            self.pos = (
                self.pos[0] + float(flow[0]) * dt * 0.1,
                self.pos[1] + float(flow[1]) * dt * 0.1,
                z,
            )
            self._step_inundation_field(dt, simulation, flow)
        elif self.kind.upper() in {"EARTHQUAKE", "AFTERSHOCK"}:
            self.shock_intensity *= max(0.0, 1.0 - self.aftershock_decay_rate * dt)
            self._step_aftershocks(simulation)

    def intensity_at(self, point: np.ndarray) -> float:
        if not self.active:
            return 0.0
        sample = _point3(point)
        origin = _point3(self.pos)
        dist = (
            float(np.linalg.norm(sample[:2] - origin[:2]))
            if self.height_aware
            else float(np.linalg.norm(sample - origin))
        )
        effective_radius = (
            self.range_m if self.kind.upper() == "SHOOTER" else self.radius
        )
        ember = (
            self._ember_intensity_at(sample)
            if self.kind.upper() in {"WILDFIRE", "EMBER"}
            else 0.0
        )
        flood = (
            self._flood_intensity_at(sample) if self.kind.upper() == "FLOOD" else 0.0
        )
        shock = (
            self._shock_intensity_at(sample)
            if self.kind.upper() in {"EARTHQUAKE", "AFTERSHOCK"}
            else 0.0
        )
        if flood > 0.0 or shock > 0.0:
            return min(1.0, flood + shock)
        if effective_radius <= 1e-6:
            base = float(self.severity) if dist <= 0.75 else 0.0
            return min(1.0, base * _height_factor(sample[2] - origin[2], self) + ember)
        if dist <= effective_radius:
            base = float(self.severity) * max(0.0, 1.0 - (dist / effective_radius))
            return min(1.0, base * _height_factor(sample[2] - origin[2], self) + ember)
        return ember

    def visibility_at(self, point: np.ndarray) -> float:
        """Returns visibility factor [0,1] at a point (1=clear, 0=opaque)."""
        if not self.active or self.visibility_reduction <= 0:
            return 1.0
        sample = _point3(point)
        origin = _point3(self.pos)
        dist = (
            float(np.linalg.norm(sample[:2] - origin[:2]))
            if self.height_aware
            else float(np.linalg.norm(sample - origin))
        )
        height_factor = _height_factor(sample[2] - origin[2], self)
        if self.radius <= 1e-6:
            return (
                1.0 - self.visibility_reduction * height_factor if dist <= 0.75 else 1.0
            )
        if dist <= self.radius:
            return (
                1.0
                - self.visibility_reduction
                * max(0.0, 1.0 - (dist / self.radius))
                * height_factor
            )
        return 1.0

    def affects(self, point: np.ndarray) -> bool:
        return np.linalg.norm(_point3(point) - _point3(self.pos)) <= self.radius

    def profile(self) -> dict[str, float]:
        return HAZARD_PROFILES.get(self.kind.upper(), HAZARD_PROFILES["GAS"])

    def snapshot(self) -> dict[str, Any]:
        return {
            "pos": self.pos,
            "kind": self.kind,
            "radius": self.radius,
            "severity": self.severity,
            "wind_vector": self.wind_vector,
            "visibility_reduction": self.visibility_reduction,
            "range_m": self.range_m,
            "accuracy": self.accuracy,
            "height_aware": self.height_aware,
            "layer_base_m": self.layer_base_m,
            "layer_top_m": self.layer_top_m,
            "vertical_decay_m": self.vertical_decay_m,
            "gas_density": self.gas_density,
            "ember_spotting_rate": self.ember_spotting_rate,
            "ember_ignition_radius": self.ember_ignition_radius,
            "ember_cell_size": self.ember_cell_size,
            "ember_origin": self.ember_origin,
            "ember_cell_count": len(self.ember_field),
            "max_ember_intensity": max(self.ember_field.values(), default=0.0),
            "flow_vector": self.flow_vector,
            "inundation_depth_m": self.inundation_depth_m,
            "inundation_rise_rate_mps": self.inundation_rise_rate_mps,
            "flood_depth_threshold_m": self.flood_depth_threshold_m,
            "max_depth_m": self.max_depth_m,
            "inundation_cell_count": len(self.inundation_field),
            "max_inundation_depth_m": max(self.inundation_field.values(), default=0.0),
            "aftershock_schedule": self.aftershock_schedule,
            "aftershock_index": self.aftershock_index,
            "shock_intensity": self.shock_intensity,
        }

    def _step_ember_field(self, dt: float, simulation) -> None:
        if self.ember_spotting_rate <= 0.0:
            return
        decay = max(0.0, 1.0 - self.ember_decay_rate * dt)
        self.ember_field = {
            cell: value * decay
            for cell, value in self.ember_field.items()
            if value * decay > 0.01
        }
        wind = np.array(self.wind_vector, dtype=float)
        wind_norm = float(np.linalg.norm(wind))
        if wind_norm <= 1e-6:
            wind_dir = np.array([1.0, 0.0], dtype=float)
        else:
            wind_dir = wind / wind_norm
        origin = _point3(self.pos)
        spot_distance = max(
            float(self.ember_ignition_radius), self.radius + wind_norm * dt
        )
        centers = [
            origin[:2],
            origin[:2] + wind_dir * spot_distance,
        ]
        cell_size = max(
            float(getattr(simulation.layout, "cell_size", self.ember_cell_size)), 1e-6
        )
        self.ember_cell_size = cell_size
        layout_origin = getattr(simulation.layout, "origin", (0.0, 0.0))
        self.ember_origin = (float(layout_origin[0]), float(layout_origin[1]))
        ember_origin = np.array(self.ember_origin, dtype=float)
        for center in centers:
            radius = max(1.0, self.radius * 0.5 + self.ember_ignition_radius * 0.25)
            min_x = int(np.floor((center[0] - radius - ember_origin[0]) / cell_size))
            max_x = int(np.ceil((center[0] + radius - ember_origin[0]) / cell_size))
            min_y = int(np.floor((center[1] - radius - ember_origin[1]) / cell_size))
            max_y = int(np.ceil((center[1] + radius - ember_origin[1]) / cell_size))
            for x in range(min_x, max_x + 1):
                for y in range(min_y, max_y + 1):
                    point = ember_origin + np.array(
                        [(x + 0.5) * cell_size, (y + 0.5) * cell_size], dtype=float
                    )
                    dist = float(np.linalg.norm(point - center))
                    if dist > radius:
                        continue
                    value = (
                        self.severity
                        * self.ember_spotting_rate
                        * dt
                        * max(0.0, 1.0 - dist / radius)
                    )
                    key = (x, y)
                    self.ember_field[key] = min(
                        1.0, self.ember_field.get(key, 0.0) + value
                    )

    def _ember_intensity_at(self, sample: np.ndarray) -> float:
        if not self.ember_field:
            return 0.0
        cell_size = max(float(self.ember_cell_size), 1e-6)
        key = (
            int(np.floor((float(sample[0]) - self.ember_origin[0]) / cell_size)),
            int(np.floor((float(sample[1]) - self.ember_origin[1]) / cell_size)),
        )
        return float(self.ember_field.get(key, 0.0))

    def _step_inundation_field(self, dt: float, simulation, flow: np.ndarray) -> None:
        decay = max(0.0, 1.0 - self.inundation_decay_rate * dt)
        self.inundation_field = {
            cell: value * decay
            for cell, value in self.inundation_field.items()
            if value * decay > 0.01
        }
        flow_norm = float(np.linalg.norm(flow))
        flow_dir = (
            flow / flow_norm if flow_norm > 1e-6 else np.array([1.0, 0.0], dtype=float)
        )
        origin = _point3(self.pos)
        centers = [
            origin[:2],
            origin[:2] + flow_dir * max(0.0, self.radius * 0.6 + flow_norm * dt),
        ]
        cell_size = max(
            float(getattr(simulation.layout, "cell_size", self.inundation_cell_size)),
            1e-6,
        )
        self.inundation_cell_size = cell_size
        layout_origin = getattr(simulation.layout, "origin", (0.0, 0.0))
        self.inundation_origin = (float(layout_origin[0]), float(layout_origin[1]))
        field_origin = np.array(self.inundation_origin, dtype=float)
        base_depth = min(
            self.max_depth_m,
            max(self.inundation_depth_m, self.severity * self.flood_depth_threshold_m),
        )
        for center in centers:
            radius = max(1.0, self.radius)
            min_x = int(np.floor((center[0] - radius - field_origin[0]) / cell_size))
            max_x = int(np.ceil((center[0] + radius - field_origin[0]) / cell_size))
            min_y = int(np.floor((center[1] - radius - field_origin[1]) / cell_size))
            max_y = int(np.ceil((center[1] + radius - field_origin[1]) / cell_size))
            for x in range(min_x, max_x + 1):
                for y in range(min_y, max_y + 1):
                    point = field_origin + np.array(
                        [(x + 0.5) * cell_size, (y + 0.5) * cell_size], dtype=float
                    )
                    dist = float(np.linalg.norm(point - center))
                    if dist > radius:
                        continue
                    depth = base_depth * max(0.0, 1.0 - dist / radius)
                    key = (x, y)
                    self.inundation_field[key] = min(
                        self.max_depth_m,
                        max(self.inundation_field.get(key, 0.0), depth),
                    )

    def _flood_intensity_at(self, sample: np.ndarray) -> float:
        depth = self._inundation_depth_at(sample)
        if depth <= 0.0:
            return 0.0
        threshold = max(float(self.flood_depth_threshold_m), 1e-6)
        return float(np.clip(self.severity * (depth / threshold), 0.0, 1.0))

    def _inundation_depth_at(self, sample: np.ndarray) -> float:
        if not self.inundation_field:
            return 0.0
        cell_size = max(float(self.inundation_cell_size), 1e-6)
        key = (
            int(np.floor((float(sample[0]) - self.inundation_origin[0]) / cell_size)),
            int(np.floor((float(sample[1]) - self.inundation_origin[1]) / cell_size)),
        )
        return float(self.inundation_field.get(key, 0.0))

    def _step_aftershocks(self, simulation) -> None:
        step = int(getattr(simulation, "current_step", 0))
        schedule = tuple(int(value) for value in self.aftershock_schedule)
        while (
            self.aftershock_index < len(schedule)
            and step >= schedule[self.aftershock_index]
        ):
            pulse_scale = max(
                0.2, float(np.exp(-self.aftershock_decay_rate * self.aftershock_index))
            )
            radius = max(float(self.damage_radius), float(self.radius), 1.0)
            damage = (
                float(self.aftershock_damage_increment)
                * float(self.severity)
                * pulse_scale
            )
            center = _point3(self.pos)
            terrain = (
                simulation.apply_terrain_damage(
                    center, radius, damage, source=str(self.kind).lower()
                )
                if hasattr(simulation, "apply_terrain_damage")
                else {"affected_cells": 0, "max_damage": 0.0}
            )
            wave_radius = max(float(self.re_evacuation_radius), radius)
            triggered = (
                simulation.trigger_re_evacuation_wave(
                    center, wave_radius, source=str(self.kind).lower()
                )
                if hasattr(simulation, "trigger_re_evacuation_wave")
                else 0
            )
            if hasattr(simulation, "aftershock_events"):
                simulation.aftershock_events.append(
                    {
                        "step": step,
                        "time_s": float(getattr(simulation, "time_s", 0.0)),
                        "hazard_kind": str(self.kind),
                        "aftershock_index": int(self.aftershock_index),
                        "radius": radius,
                        "damage_increment": damage,
                        "affected_cells": int(terrain.get("affected_cells", 0)),
                        "max_damage": float(terrain.get("max_damage", 0.0)),
                        "triggered_agents": int(triggered),
                    }
                )
            self.shock_intensity = max(
                self.shock_intensity, float(self.severity) * pulse_scale
            )
            self.aftershock_index += 1

    def _shock_intensity_at(self, sample: np.ndarray) -> float:
        if self.shock_intensity <= 0.01:
            return 0.0
        origin = _point3(self.pos)
        dist = float(np.linalg.norm(sample - origin))
        radius = max(float(self.radius), float(self.damage_radius), 1e-6)
        if dist > radius:
            return 0.0
        return float(
            np.clip(self.shock_intensity * max(0.0, 1.0 - dist / radius), 0.0, 1.0)
        )


@dataclass
class ImportedHazardField:
    """Static hazard field imported from an external gas/smoke reference grid."""

    kind: str
    intensity_grid: np.ndarray
    origin: tuple[float, float] = (0.0, 0.0)
    cell_size: float = 1.0
    visibility_grid: np.ndarray | None = None
    source: dict[str, Any] = field(default_factory=dict)
    base_z: float = 0.0
    height_aware: bool = False
    layer_base_m: float | None = None
    layer_top_m: float | None = None
    vertical_decay_m: float = 1.0
    gas_density: float = 1.0
    active: bool = True

    @property
    def pos(self) -> tuple[float, float]:
        height, width = self.intensity_grid.shape
        return (
            self.origin[0] + (width * self.cell_size / 2.0),
            self.origin[1] + (height * self.cell_size / 2.0),
        )

    @property
    def radius(self) -> float:
        height, width = self.intensity_grid.shape
        return max(width, height) * self.cell_size / 2.0

    @property
    def severity(self) -> float:
        if self.intensity_grid.size == 0:
            return 0.0
        return float(np.nanmax(self.intensity_grid))

    @classmethod
    def from_file(cls, path: str | Path, *, kind: str = "GAS") -> ImportedHazardField:
        source = Path(path)
        suffix = source.suffix.lower()
        if suffix == ".json":
            return cls.from_json(source, kind=kind)
        if suffix == ".csv":
            return cls.from_csv(source, kind=kind)
        raise ValueError(f"Unsupported hazard field format: {source}")

    @classmethod
    def from_json(cls, path: str | Path, *, kind: str = "GAS") -> ImportedHazardField:
        source = Path(path)
        payload = json.loads(source.read_text())
        intensity = _numeric_grid(
            payload.get("intensity") or payload.get("intensity_grid")
        )
        visibility_payload = payload.get("visibility") or payload.get("visibility_grid")
        visibility = (
            _numeric_grid(visibility_payload)
            if visibility_payload is not None
            else None
        )
        if visibility is not None and visibility.shape != intensity.shape:
            raise ValueError("Visibility grid must match intensity grid shape")
        return cls(
            kind=str(payload.get("kind", kind)),
            intensity_grid=intensity,
            visibility_grid=visibility,
            origin=tuple(float(v) for v in payload.get("origin", (0.0, 0.0))),
            cell_size=float(payload.get("cell_size", 1.0)),
            source=dict(payload.get("source", {})),
            base_z=float(payload.get("base_z", payload.get("z", 0.0))),
            height_aware=bool(payload.get("height_aware", False)),
            layer_base_m=_optional_float(payload.get("layer_base_m")),
            layer_top_m=_optional_float(payload.get("layer_top_m")),
            vertical_decay_m=float(payload.get("vertical_decay_m", 1.0)),
            gas_density=float(payload.get("gas_density", 1.0)),
        )

    @classmethod
    def from_csv(cls, path: str | Path, *, kind: str = "GAS") -> ImportedHazardField:
        source = Path(path)
        with source.open(newline="") as handle:
            rows = list(csv.DictReader(handle))
        if not rows:
            raise ValueError(f"Hazard field CSV has no rows: {source}")

        xs = sorted({int(row["x"]) for row in rows})
        ys = sorted({int(row["y"]) for row in rows})
        x_index = {x: idx for idx, x in enumerate(xs)}
        y_index = {y: idx for idx, y in enumerate(ys)}
        intensity = np.zeros((len(ys), len(xs)), dtype=float)
        visibility = np.ones((len(ys), len(xs)), dtype=float)
        has_visibility = False
        for row in rows:
            x = x_index[int(row["x"])]
            y = y_index[int(row["y"])]
            intensity[y, x] = float(row.get("intensity", 0.0) or 0.0)
            if row.get("visibility") not in (None, ""):
                visibility[y, x] = float(row["visibility"])
                has_visibility = True
        return cls(
            kind=kind,
            intensity_grid=intensity,
            visibility_grid=visibility if has_visibility else None,
            origin=(float(min(xs)), float(min(ys))),
            cell_size=1.0,
            source={"path": str(source)},
        )

    def step(self, dt: float, simulation) -> None:
        return None

    def intensity_at(self, point: np.ndarray) -> float:
        if not self.active:
            return 0.0
        cell = self._cell_for_point(point)
        if cell is None:
            return 0.0
        x, y = cell
        return float(self.intensity_grid[y, x]) * _height_factor(
            float(_point3(point)[2]) - self.base_z, self
        )

    def visibility_at(self, point: np.ndarray) -> float:
        if not self.active or self.visibility_grid is None:
            return 1.0
        cell = self._cell_for_point(point)
        if cell is None:
            return 1.0
        x, y = cell
        obscuration = 1.0 - float(np.clip(self.visibility_grid[y, x], 0.0, 1.0))
        return float(
            np.clip(
                1.0
                - obscuration
                * _height_factor(float(_point3(point)[2]) - self.base_z, self),
                0.0,
                1.0,
            )
        )

    def affects(self, point: np.ndarray) -> bool:
        return self.intensity_at(point) > 0.0

    def profile(self) -> dict[str, float]:
        return HAZARD_PROFILES.get(self.kind.upper(), HAZARD_PROFILES["GAS"])

    def snapshot(self) -> dict[str, Any]:
        return {
            "pos": self.pos,
            "kind": self.kind,
            "radius": self.radius,
            "severity": self.severity,
            "origin": self.origin,
            "cell_size": self.cell_size,
            "field_shape": tuple(int(v) for v in self.intensity_grid.shape),
            "source": dict(self.source),
            "imported_field": True,
            "height_aware": self.height_aware,
            "layer_base_m": self.layer_base_m,
            "layer_top_m": self.layer_top_m,
            "vertical_decay_m": self.vertical_decay_m,
            "gas_density": self.gas_density,
        }

    def _cell_for_point(self, point: np.ndarray) -> tuple[int, int] | None:
        x = int(np.floor((float(point[0]) - self.origin[0]) / self.cell_size))
        y = int(np.floor((float(point[1]) - self.origin[1]) / self.cell_size))
        height, width = self.intensity_grid.shape
        if x < 0 or y < 0 or x >= width or y >= height:
            return None
        return (x, y)


def _numeric_grid(values: Any) -> np.ndarray:
    if values is None:
        raise ValueError("Hazard field requires an intensity grid")
    grid = np.array(values, dtype=float)
    if grid.ndim != 2:
        raise ValueError("Hazard field grids must be two-dimensional")
    if grid.size == 0:
        raise ValueError("Hazard field grids must not be empty")
    return grid


def _point3(value: Any) -> np.ndarray:
    if len(value) >= 3:
        return np.array(
            [float(value[0]), float(value[1]), float(value[2])], dtype=float
        )
    return np.array([float(value[0]), float(value[1]), 0.0], dtype=float)


def _optional_float(value: Any) -> float | None:
    if value is None:
        return None
    return float(value)


def _height_factor(height_m: float, hazard: Any) -> float:
    if not bool(getattr(hazard, "height_aware", False)):
        return 1.0
    decay = max(float(getattr(hazard, "vertical_decay_m", 1.0)), 1e-6)
    base = getattr(hazard, "layer_base_m", None)
    top = getattr(hazard, "layer_top_m", None)
    if base is not None and top is not None:
        lo, hi = sorted((float(base), float(top)))
        if lo <= height_m <= hi:
            return 1.0
        distance = lo - height_m if height_m < lo else height_m - hi
        return float(np.exp(-max(0.0, distance) / decay))
    density = max(float(getattr(hazard, "gas_density", 1.0)), 1e-6)
    if density > 1.0:
        return float(np.exp(-max(0.0, height_m) * (density - 1.0) / decay))
    if density < 1.0:
        return float(np.exp(-max(0.0, -height_m) * ((1.0 / density) - 1.0) / decay))
    return 1.0
