"""
Measurement line abstraction for empirical-grade flow/density/speed capture.

Places virtual sensors at corridor cross-sections following the methodology
of Steffen & Seyfried (2010). Detects agent crossings via dot-product sign
change with the line normal. Density measured in a configurable rectangular
region around the line.

References:
    Steffen, B. & Seyfried, A. "Methods for measuring pedestrian density,
    flow, speed and direction with minimal scatter." arXiv:0911.2165 (2010).
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Sequence, Tuple
import numpy as np

Point = Tuple[float, float]


@dataclass
class MeasurementRecord:
    """Single timestep record from a measurement line."""
    step: int
    time_s: float
    flow: float          # agents/s crossing the line this step
    density: float       # agents/m² in measurement region
    speed: float         # mean speed of agents in measurement region (m/s)
    n_crossing: int      # agents crossing this step
    n_in_region: int     # agents inside measurement region


@dataclass
class MeasurementLine:
    """
    Virtual sensor at a corridor cross-section.

    Defined by two endpoints (p1, p2) forming a line segment.
    Crossing detection: agent moved from one side to the other
    (dot product sign change with line normal).

    Args:
        name: identifier for this sensor
        p1, p2: line endpoints in world coordinates
        depth: half-width of the measurement region (cells on each side)
        cell_size: spatial scale for area calculation
    """
    name: str
    p1: Point
    p2: Point
    depth: float = 2.0
    cell_size: float = 1.0
    _records: List[MeasurementRecord] = field(default_factory=list, repr=False)
    _prev_sides: Dict[int, float] = field(default_factory=dict, repr=False)

    def __post_init__(self):
        p1 = np.array(self.p1, dtype=float)
        p2 = np.array(self.p2, dtype=float)
        direction = p2 - p1
        length = float(np.linalg.norm(direction))
        if length < 1e-8:
            raise ValueError(f"MeasurementLine '{self.name}': p1 and p2 must be distinct")
        self._line_vec = direction / length
        self._normal = np.array([-self._line_vec[1], self._line_vec[0]])
        self._origin = p1
        self._length = length
        # measurement region: rectangle centered on line, extending `depth` on each side
        self._region_area = self._length * (2 * self.depth) * (self.cell_size ** 2)

    @property
    def records(self) -> List[MeasurementRecord]:
        return list(self._records)

    def _signed_distance(self, pos: np.ndarray) -> float:
        """Signed distance from point to the line (not segment — infinite line)."""
        return float(np.dot(pos - self._origin, self._normal))

    def _in_region(self, pos: np.ndarray) -> bool:
        """Check if point is inside the rectangular measurement region."""
        rel = pos - self._origin
        along = float(np.dot(rel, self._line_vec))
        perp = abs(float(np.dot(rel, self._normal)))
        return -0.5 <= along <= self._length + 0.5 and perp <= self.depth

    def record(
        self,
        step: int,
        time_s: float,
        dt: float,
        agents: Sequence[Any],
        previous_positions: Dict[int, np.ndarray],
    ) -> MeasurementRecord:
        """
        Record measurements for one simulation step.

        Args:
            step: current step number
            time_s: simulation time
            dt: timestep duration
            agents: list of active agents (must have .id, .pos, .current_speed)
            previous_positions: {agent_id: previous_pos_array}
        """
        n_crossing = 0
        in_region_speeds: List[float] = []
        n_in_region = 0

        for agent in agents:
            pos = np.array(agent.pos, dtype=float)
            side = self._signed_distance(pos)

            # crossing detection
            prev_side = self._prev_sides.get(agent.id)
            if prev_side is not None and prev_side * side < 0: # sign change
                # verify the crossing happened near the segment (not way off to the side)
                prev_pos = previous_positions.get(agent.id)
                if prev_pos is not None:
                    midpoint = (pos + np.array(prev_pos, dtype=float)) / 2
                    along = float(np.dot(midpoint - self._origin, self._line_vec))
                    if -0.5 <= along <= self._length + 0.5:
                        n_crossing += 1

            self._prev_sides[agent.id] = side

            # region membership
            if self._in_region(pos):
                n_in_region += 1
                in_region_speeds.append(float(getattr(agent, 'current_speed', 0.0)))

        flow = n_crossing / dt if dt > 0 else 0.0
        density = n_in_region / self._region_area if self._region_area > 0 else 0.0
        mean_speed = float(np.mean(in_region_speeds)) if in_region_speeds else 0.0

        rec = MeasurementRecord(
            step=step, time_s=time_s,
            flow=flow, density=density, speed=mean_speed,
            n_crossing=n_crossing, n_in_region=n_in_region,
        )
        self._records.append(rec)
        return rec

    def reset(self) -> None:
        self._records.clear()
        self._prev_sides.clear()

    def to_dataframe(self):
        """Export records as pandas DataFrame."""
        import pandas as pd
        return pd.DataFrame([
            {
                "line_name": self.name,
                "step": r.step, "time_s": r.time_s,
                "flow": r.flow, "density": r.density, "speed": r.speed,
                "n_crossing": r.n_crossing, "n_in_region": r.n_in_region,
            }
            for r in self._records
        ])

    def speed_density_pairs(self, min_agents: int = 3) -> Tuple[np.ndarray, np.ndarray]:
        """
        Extract (density, speed) pairs for fundamental diagram construction.
        Filters out steps with fewer than min_agents in region.
        """
        densities = []
        speeds = []
        for r in self._records:
            if r.n_in_region >= min_agents and r.density > 0.01:
                densities.append(r.density)
                speeds.append(r.speed)
        return np.array(densities, dtype=float), np.array(speeds, dtype=float)
