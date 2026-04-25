"""
Telemetry data structures for ITED framework.

Includes per-step agent telemetry with entropy, belief accuracy,
impairment, and decision mode fields.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any, Dict, Iterable, List, Optional, Tuple
import numpy as np

Cell = Tuple[int, int]

@dataclass(frozen=True)
class BottleneckZone:
    zone_id: str
    cells: Tuple[Cell, ...]
    orientation: str
    centroid: Tuple[float, float]

@dataclass
class BottleneckStepTelemetry:
    occupancy: int = 0
    inflow: int = 0
    outflow: int = 0
    queue_length: int = 0
    mean_dwell_s: float = 0.0
    mean_speed: float = 0.0
    mean_density: float = 0.0

@dataclass
class AgentStepTelemetry:
    agent_id: int
    position: Tuple[float, float]
    cell: Cell
    state: str
    speed: float
    local_density: float
    target_exit: Optional[Cell]
    cohort_name: str
    group_id: Optional[int]
    leader_id: Optional[int]
    hazard_exposure: float
    hazard_load: float
    trail: Tuple[Tuple[float, float], ...] = field(default_factory=tuple)
    # ITED fields
    entropy: float = 0.0
    belief_accuracy: float = 1.0
    impairment: float = 0.0
    decision_mode: str = "EVACUATE"

@dataclass
class StepTelemetry:
    step: int
    time_s: float
    occupancy_grid: np.ndarray
    density_grid: np.ndarray
    speed_grid: np.ndarray
    path_usage_grid: np.ndarray
    agents: List[AgentStepTelemetry]
    exit_flow_cumulative: Dict[str, int]
    exit_flow_step: Dict[str, int]
    bottlenecks: Dict[str, BottleneckStepTelemetry]
    hazards: List[Dict[str, Any]]
    evacuated_total: int
    remaining: int
    pending_release: int
    mean_speed: float
    mean_density: float
    # ITED fields
    global_entropy: float = 0.0


def _walkable_neighbors(layout, cell: Cell) -> List[Cell]:
    x, y = cell
    neighbors: List[Cell] = []
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        nx, ny = x + dx, y + dy
        if layout.is_walkable((nx, ny)):
            neighbors.append((nx, ny))
    return neighbors

def _local_openness(layout, cell: Cell) -> int:
    x, y = cell
    openness = 0
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            if dx == 0 and dy == 0:
                continue
            if layout.is_walkable((x + dx, y + dy)):
                openness += 1
    return openness

def _corridor_orientation(neighbors: List[Cell], cell: Cell) -> Optional[str]:
    x, y = cell
    if len(neighbors) == 1:
        nx, ny = neighbors[0]
        return "horizontal" if nx != x else "vertical"
    if len(neighbors) != 2:
        return None
    xs = {n[0] for n in neighbors}
    ys = {n[1] for n in neighbors}
    if len(xs) == 2 and len(ys) == 1 and y in ys:
        return "horizontal"
    if len(ys) == 2 and len(xs) == 1 and x in xs:
        return "vertical"
    return None

def detect_bottleneck_zones(layout) -> List[BottleneckZone]:
    candidates: List[Tuple[Cell, str]] = []
    exit_cells = set(layout.exit_positions())
    for y in range(layout.height):
        for x in range(layout.width):
            cell = (x, y)
            if not layout.is_walkable(cell) or cell in exit_cells:
                continue
            neighbors = _walkable_neighbors(layout, cell)
            orientation = _corridor_orientation(neighbors, cell)
            if orientation is None:
                continue
            if _local_openness(layout, cell) > 4:
                continue
            candidates.append((cell, orientation))
    if not candidates:
        return []
    orientation_by_cell = {cell: o for cell, o in candidates}
    remaining = {cell for cell, _ in candidates}
    zones: List[BottleneckZone] = []
    while remaining:
        start = min(remaining)
        stack = [start]
        group: List[Cell] = []
        orientation = orientation_by_cell[start]
        remaining.remove(start)
        while stack:
            cell = stack.pop()
            group.append(cell)
            for neighbor in _walkable_neighbors(layout, cell):
                if neighbor in remaining and orientation_by_cell[neighbor] == orientation:
                    remaining.remove(neighbor)
                    stack.append(neighbor)
        cells = tuple(sorted(group))
        centroid = (
            float(np.mean([c[0] + 0.5 for c in cells])),
            float(np.mean([c[1] + 0.5 for c in cells])),
        )
        zones.append(BottleneckZone(
            zone_id=f"bn_{len(zones) + 1}", cells=cells,
            orientation=orientation, centroid=centroid,
        ))
    return zones

def zone_lookup(zones: Iterable[BottleneckZone]) -> Dict[Cell, str]:
    lookup: Dict[Cell, str] = {}
    for zone in zones:
        for cell in zone.cells:
            lookup[cell] = zone.zone_id
    return lookup

def zone_distance(cell: Cell, zone: BottleneckZone) -> int:
    return min(abs(cell[0] - zx) + abs(cell[1] - zy) for zx, zy in zone.cells)
