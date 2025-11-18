from __future__ import annotations

from dataclasses import dataclass
from typing import Tuple, Dict, Any
import numpy as np


@dataclass
class Hazard:
    pos: Tuple[float, float]
    kind: str
    radius: float = 0.0
    severity: float = 0.5
    spread_rate: float = 0.0

    def step(self, dt: float, simulation) -> None:
        # Basic spread for gas-like hazards
        if self.kind.upper() == "GAS":
            self.radius += self.spread_rate * dt

    def affects(self, point: np.ndarray) -> bool:
        return np.linalg.norm(point - np.array(self.pos)) <= self.radius

    def snapshot(self) -> Dict[str, Any]:
        return {
            "pos": self.pos,
            "kind": self.kind,
            "radius": self.radius,
            "severity": self.severity,
        }
