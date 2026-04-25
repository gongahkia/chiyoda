"""
Multi-hazard physics engine with advection-diffusion, visibility effects,
and physiological impact tables for ITED CBRN scenarios.
"""
from __future__ import annotations
from dataclasses import dataclass, field
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
