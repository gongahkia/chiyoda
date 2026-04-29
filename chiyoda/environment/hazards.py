"""
Multi-hazard physics engine with advection-diffusion, visibility effects,
and physiological impact tables for ITED CBRN scenarios.
"""
from __future__ import annotations
from dataclasses import dataclass, field
import csv
import json
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple
import numpy as np

# physiological effect profiles: (speed_factor, rationality_factor, vision_factor)
HAZARD_PROFILES = {
    "GAS": { # nerve agent (sarin-like)
        "speed_decay": 0.8,     # strong motor impairment
        "vision_decay": 0.6,    # miosis (pupil constriction)
        "rationality_decay": 0.5,
        "incapacitation_threshold": 4.0, # cumulative exposure units
    },
    "SMOKE": { # obscurant
        "speed_decay": 0.3,     # mild physical effect
        "vision_decay": 0.9,    # severe visibility reduction
        "rationality_decay": 0.2,
        "incapacitation_threshold": 10.0,
    },
    "FIRE": { # thermal
        "speed_decay": 0.5,
        "vision_decay": 0.4,
        "rationality_decay": 0.3,
        "incapacitation_threshold": 3.0,
    },
    "CRUSH": { # crowd crush (density-induced)
        "speed_decay": 0.9,
        "vision_decay": 0.1,
        "rationality_decay": 0.4,
        "incapacitation_threshold": 5.0,
    },
}

@dataclass
class Hazard:
    pos: Tuple[float, float]
    kind: str
    radius: float = 0.0
    severity: float = 0.5
    spread_rate: float = 0.0
    wind_vector: Tuple[float, float] = (0.0, 0.0) # advection direction
    diffusion_rate: float = 0.1 # isotropic diffusion coefficient
    visibility_reduction: float = 0.0 # how much this hazard reduces visibility [0,1]
    active: bool = True

    def step(self, dt: float, simulation) -> None:
        if not self.active:
            return
        # advection-diffusion spread
        if self.kind.upper() in ("GAS", "SMOKE"):
            self.radius += self.spread_rate * dt + self.diffusion_rate * dt
            # advect center position
            self.pos = (
                self.pos[0] + self.wind_vector[0] * dt,
                self.pos[1] + self.wind_vector[1] * dt,
            )
        elif self.kind.upper() == "FIRE":
            self.radius += self.spread_rate * dt * 0.5 # fire spreads slower

    def intensity_at(self, point: np.ndarray) -> float:
        if not self.active:
            return 0.0
        dist = float(np.linalg.norm(point - np.array(self.pos, dtype=float)))
        if self.radius <= 1e-6:
            return float(self.severity) if dist <= 0.75 else 0.0
        if dist <= self.radius:
            return float(self.severity) * max(0.0, 1.0 - (dist / self.radius))
        return 0.0

    def visibility_at(self, point: np.ndarray) -> float:
        """Returns visibility factor [0,1] at a point (1=clear, 0=opaque)."""
        if not self.active or self.visibility_reduction <= 0:
            return 1.0
        dist = float(np.linalg.norm(point - np.array(self.pos, dtype=float)))
        if self.radius <= 1e-6:
            return 1.0 - self.visibility_reduction if dist <= 0.75 else 1.0
        if dist <= self.radius:
            return 1.0 - self.visibility_reduction * max(0.0, 1.0 - (dist / self.radius))
        return 1.0

    def affects(self, point: np.ndarray) -> bool:
        return np.linalg.norm(point - np.array(self.pos)) <= self.radius

    def profile(self) -> Dict[str, float]:
        return HAZARD_PROFILES.get(self.kind.upper(), HAZARD_PROFILES["GAS"])

    def snapshot(self) -> Dict[str, Any]:
        return {
            "pos": self.pos,
            "kind": self.kind,
            "radius": self.radius,
            "severity": self.severity,
            "wind_vector": self.wind_vector,
            "visibility_reduction": self.visibility_reduction,
        }


@dataclass
class ImportedHazardField:
    """Static hazard field imported from an external gas/smoke reference grid."""

    kind: str
    intensity_grid: np.ndarray
    origin: Tuple[float, float] = (0.0, 0.0)
    cell_size: float = 1.0
    visibility_grid: Optional[np.ndarray] = None
    source: Dict[str, Any] = field(default_factory=dict)
    active: bool = True

    @property
    def pos(self) -> Tuple[float, float]:
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
    def from_file(cls, path: str | Path, *, kind: str = "GAS") -> "ImportedHazardField":
        source = Path(path)
        suffix = source.suffix.lower()
        if suffix == ".json":
            return cls.from_json(source, kind=kind)
        if suffix == ".csv":
            return cls.from_csv(source, kind=kind)
        raise ValueError(f"Unsupported hazard field format: {source}")

    @classmethod
    def from_json(cls, path: str | Path, *, kind: str = "GAS") -> "ImportedHazardField":
        source = Path(path)
        payload = json.loads(source.read_text())
        intensity = _numeric_grid(payload.get("intensity") or payload.get("intensity_grid"))
        visibility_payload = payload.get("visibility") or payload.get("visibility_grid")
        visibility = _numeric_grid(visibility_payload) if visibility_payload is not None else None
        if visibility is not None and visibility.shape != intensity.shape:
            raise ValueError("Visibility grid must match intensity grid shape")
        return cls(
            kind=str(payload.get("kind", kind)),
            intensity_grid=intensity,
            visibility_grid=visibility,
            origin=tuple(float(v) for v in payload.get("origin", (0.0, 0.0))),
            cell_size=float(payload.get("cell_size", 1.0)),
            source=dict(payload.get("source", {})),
        )

    @classmethod
    def from_csv(cls, path: str | Path, *, kind: str = "GAS") -> "ImportedHazardField":
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
        return float(self.intensity_grid[y, x])

    def visibility_at(self, point: np.ndarray) -> float:
        if not self.active or self.visibility_grid is None:
            return 1.0
        cell = self._cell_for_point(point)
        if cell is None:
            return 1.0
        x, y = cell
        return float(np.clip(self.visibility_grid[y, x], 0.0, 1.0))

    def affects(self, point: np.ndarray) -> bool:
        return self.intensity_at(point) > 0.0

    def profile(self) -> Dict[str, float]:
        return HAZARD_PROFILES.get(self.kind.upper(), HAZARD_PROFILES["GAS"])

    def snapshot(self) -> Dict[str, Any]:
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
        }

    def _cell_for_point(self, point: np.ndarray) -> Optional[Tuple[int, int]]:
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
