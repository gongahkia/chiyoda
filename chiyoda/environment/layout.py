from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable, List, Mapping, Tuple

import numpy as np


WALL = "X"
PERSON = "@"
EXIT = "E"
EMPTY = "."
BEACON = "S"  # signage / PA system
VENTILATION = "V"  # ventilation source (affects hazard advection)
RESPONDER_ENTRY = "R"  # first responder entry point

Cell = Tuple[str, int, int]


@dataclass(frozen=True)
class Floor:
    id: str
    z: float
    grid: np.ndarray
    cell_heights: np.ndarray | None = None


@dataclass(frozen=True)
class Connector:
    id: str
    type: str
    from_cell: Cell
    to_cell: Cell
    bidirectional: bool = True
    width: float = 1.0
    speed_multiplier: float = 0.7
    capacity: int | None = None
    flow_rate: float | None = None
    queue_mode: str = "fifo"
    panic_jam_density: float | None = None
    jam_flow_multiplier: float = 0.35
    dwell_s: float = 0.0
    travel_s: float = 0.0
    height_delta_m: float = 0.0


class Layout:
    """Strict 3D floor layout with a primary-floor compatibility surface."""

    def __init__(
        self,
        grid: np.ndarray | None = None,
        *,
        origin: Tuple[float, float] = (0.0, 0.0),
        cell_size: float = 1.0,
        floors: Mapping[str, Floor | Mapping[str, Any] | np.ndarray] | None = None,
        connectors: Iterable[Connector | Mapping[str, Any]] | None = None,
        primary_floor_id: str | None = None,
    ) -> None:
        if floors is None:
            if grid is None:
                raise ValueError("Layout requires grid or floors")
            floors = {"0": Floor(id="0", z=0.0, grid=np.array(grid, copy=True))}
        parsed: dict[str, Floor] = {}
        for floor_id, floor in floors.items():
            fid = str(floor_id)
            if isinstance(floor, Floor):
                heights = None if floor.cell_heights is None else np.array(floor.cell_heights, dtype=float, copy=True)
                parsed[fid] = Floor(
                    id=fid,
                    z=float(floor.z),
                    grid=np.array(floor.grid, copy=True),
                    cell_heights=heights,
                )
            elif isinstance(floor, np.ndarray):
                parsed[fid] = Floor(id=fid, z=0.0, grid=np.array(floor, copy=True))
            else:
                grid_data = np.array(floor["grid"], dtype="<U1")
                parsed[fid] = Floor(
                    id=fid,
                    z=float(floor.get("z", 0.0)),
                    grid=grid_data,
                    cell_heights=_coerce_height_grid(floor, grid_data.shape),
                )
        if not parsed:
            raise ValueError("Layout must contain at least one floor")
        self.floors = parsed
        self.primary_floor_id = str(primary_floor_id or next(iter(parsed)))
        if self.primary_floor_id not in self.floors:
            raise ValueError(f"Unknown primary floor: {self.primary_floor_id}")
        self.connectors = [self._coerce_connector(item) for item in (connectors or [])]
        self.origin = tuple(float(v) for v in origin)
        self.cell_size = float(cell_size)

    @classmethod
    def from_text(
        cls,
        text: str,
        *,
        floor_id: str = "0",
        z: float = 0.0,
        cell_height_m: float | None = None,
    ) -> "Layout":
        lines = [list(line.rstrip("\n")) for line in text.splitlines() if line.strip()]
        if not lines:
            raise ValueError("Layout text must contain at least one non-empty row")
        max_len = max(len(row) for row in lines)
        padded = [row + [EMPTY] * (max_len - len(row)) for row in lines]
        grid = np.array(padded, dtype="<U1")
        heights = None if cell_height_m is None else np.full(grid.shape, float(cell_height_m), dtype=float)
        return cls(
            floors={str(floor_id): Floor(id=str(floor_id), z=float(z), grid=grid, cell_heights=heights)},
            primary_floor_id=str(floor_id),
        )

    @classmethod
    def from_file(cls, path: str, *, floor_id: str = "0", z: float = 0.0) -> "Layout":
        with open(path, "r") as handle:
            return cls.from_text(handle.read(), floor_id=floor_id, z=z)

    @classmethod
    def from_floors(
        cls,
        floors: Iterable[Mapping[str, Any]],
        *,
        connectors: Iterable[Mapping[str, Any]] | None = None,
        cell_size: float = 1.0,
        origin: Tuple[float, float] = (0.0, 0.0),
    ) -> "Layout":
        parsed: dict[str, Floor] = {}
        for floor in floors:
            floor_id = str(floor.get("id", floor.get("level", "")))
            if not floor_id:
                raise ValueError("Each layout floor requires id")
            if "text" not in floor:
                raise ValueError(f"Floor {floor_id} requires text")
            base_floor = cls.from_text(
                str(floor["text"]),
                floor_id=floor_id,
                z=float(floor.get("z", 0.0)),
            ).floors[floor_id]
            parsed[floor_id] = Floor(
                id=base_floor.id,
                z=base_floor.z,
                grid=base_floor.grid,
                cell_heights=_coerce_height_grid(floor, base_floor.grid.shape),
            )
        return cls(
            floors=parsed,
            connectors=list(connectors or []),
            primary_floor_id=next(iter(parsed)),
            cell_size=cell_size,
            origin=origin,
        )

    @classmethod
    def from_geojson(
        cls,
        source,
        *,
        cell_size: float = 1.0,
        padding: int = 1,
        role_property: str = "role",
        default_token: str | None = None,
        add_border_walls: bool = False,
    ) -> "Layout":
        from chiyoda.environment.obstacles import rasterize_geojson_layout

        grid, origin, resolved_cell_size = rasterize_geojson_layout(
            source,
            cell_size=cell_size,
            padding=padding,
            role_property=role_property,
            default_token=default_token,
            add_border_walls=add_border_walls,
        )
        return cls(grid=grid, origin=origin, cell_size=resolved_cell_size)

    @classmethod
    def from_cad(
        cls,
        source,
        *,
        cell_size: float = 1.0,
        padding: int = 1,
        role_layers: dict[str, list[str]] | None = None,
        default_role: str = "obstacle",
        default_token: str | None = None,
        add_border_walls: bool = False,
        line_thickness: float = 1.0,
    ) -> "Layout":
        from chiyoda.environment.obstacles import rasterize_dxf_layout

        grid, origin, resolved_cell_size = rasterize_dxf_layout(
            source,
            cell_size=cell_size,
            padding=padding,
            role_layers=role_layers,
            default_role=default_role,
            default_token=default_token,
            add_border_walls=add_border_walls,
            line_thickness=line_thickness,
        )
        return cls(grid=grid, origin=origin, cell_size=resolved_cell_size)

    @property
    def grid(self) -> np.ndarray:
        return self.floors[self.primary_floor_id].grid

    @grid.setter
    def grid(self, value: np.ndarray) -> None:
        floor = self.floors[self.primary_floor_id]
        self.floors[self.primary_floor_id] = Floor(id=floor.id, z=floor.z, grid=np.array(value, copy=True))

    @property
    def height(self) -> int:
        return int(self.grid.shape[0])

    @property
    def width(self) -> int:
        return int(self.grid.shape[1])

    def floor_ids(self) -> list[str]:
        return list(self.floors.keys())

    def floor_z(self, floor_id: str) -> float:
        return float(self.floors[str(floor_id)].z)

    def cell_height(self, cell: Cell) -> float:
        floor_id, x, y = self.cell(cell)
        floor = self.floors[str(floor_id)]
        if floor.cell_heights is None:
            return 0.0
        if y < 0 or y >= floor.cell_heights.shape[0] or x < 0 or x >= floor.cell_heights.shape[1]:
            return 0.0
        return float(floor.cell_heights[y, x])

    def floor_for_z(self, z: float) -> str:
        return min(self.floors.values(), key=lambda floor: abs(float(floor.z) - float(z))).id

    def cell(self, value: Any, *, floor_id: str | None = None) -> Cell:
        if isinstance(value, np.ndarray):
            if value.shape[0] >= 3:
                fid = floor_id or self.floor_for_z(float(value[2]))
            else:
                fid = floor_id or self.primary_floor_id
            return (str(fid), int(np.floor(float(value[0]))), int(np.floor(float(value[1]))))
        if len(value) >= 3 and isinstance(value[0], str):
            return (str(value[0]), int(value[1]), int(value[2]))
        if len(value) >= 3 and floor_id is None:
            return (self.floor_for_z(float(value[2])), int(value[0]), int(value[1]))
        return (str(floor_id or self.primary_floor_id), int(value[0]), int(value[1]))

    def world_position(self, cell: Cell, *, height_offset: float = 0.0) -> np.ndarray:
        floor_id, x, y = self.cell(cell)
        z = self.floor_z(floor_id) + self.cell_height((floor_id, x, y)) + float(height_offset)
        return np.array([float(x) + 0.5, float(y) + 0.5, z], dtype=float)

    def is_walkable(self, pos: Any, *, floor_id: str | None = None) -> bool:
        fid, x, y = self.cell(pos, floor_id=floor_id)
        floor = self.floors.get(fid)
        if floor is None:
            return False
        if y < 0 or y >= floor.grid.shape[0] or x < 0 or x >= floor.grid.shape[1]:
            return False
        return floor.grid[y, x] != WALL

    def is_exit(self, pos: Any, *, floor_id: str | None = None) -> bool:
        fid, x, y = self.cell(pos, floor_id=floor_id)
        floor = self.floors.get(fid)
        if floor is None:
            return False
        if y < 0 or y >= floor.grid.shape[0] or x < 0 or x >= floor.grid.shape[1]:
            return False
        return floor.grid[y, x] == EXIT

    def positions_for_token(self, token: str) -> List[Cell]:
        cells: list[Cell] = []
        for floor_id, floor in self.floors.items():
            ys, xs = np.where(floor.grid == token)
            cells.extend((floor_id, int(x), int(y)) for x, y in zip(xs.tolist(), ys.tolist()))
        return cells

    def people_positions(self) -> List[Cell]:
        return self.positions_for_token(PERSON)

    def responder_positions(self) -> List[Cell]:
        return self.positions_for_token(RESPONDER_ENTRY)

    def exit_positions(self) -> List[Cell]:
        return self.positions_for_token(EXIT)

    def beacon_positions(self) -> List[Cell]:
        return self.positions_for_token(BEACON)

    def all_walkable_cells(self) -> List[Cell]:
        cells: list[Cell] = []
        for floor_id, floor in self.floors.items():
            ys, xs = np.where(floor.grid != WALL)
            cells.extend((floor_id, int(x), int(y)) for x, y in zip(xs.tolist(), ys.tolist()))
        return cells

    def random_walkable_position(self) -> Cell:
        cells = self.all_walkable_cells()
        if not cells:
            raise ValueError("Layout has no walkable cells")
        idx = np.random.randint(0, len(cells))
        return cells[int(idx)]

    def connector_for_edge(self, source: Cell, target: Cell) -> Connector | None:
        source = self.cell(source)
        target = self.cell(target)
        for connector in self.connectors:
            if connector.from_cell == source and connector.to_cell == target:
                return connector
            if connector.bidirectional and connector.to_cell == source and connector.from_cell == target:
                return connector
        return None

    def connector_edges(self) -> list[tuple[Cell, Cell, Connector]]:
        edges = []
        for connector in self.connectors:
            edges.append((connector.from_cell, connector.to_cell, connector))
            if connector.bidirectional:
                edges.append((connector.to_cell, connector.from_cell, connector))
        return edges

    def clone(self) -> "Layout":
        return Layout(
            floors={
                key: Floor(
                    id=floor.id,
                    z=floor.z,
                    grid=np.array(floor.grid, copy=True),
                    cell_heights=None if floor.cell_heights is None else np.array(floor.cell_heights, dtype=float, copy=True),
                )
                for key, floor in self.floors.items()
            },
            connectors=list(self.connectors),
            origin=tuple(self.origin),
            cell_size=float(self.cell_size),
            primary_floor_id=self.primary_floor_id,
        )

    def _coerce_connector(self, raw: Connector | Mapping[str, Any]) -> Connector:
        if isinstance(raw, Connector):
            return raw
        ctype = str(raw.get("type", "stairs")).lower()
        if ctype not in {"stairs", "ramp", "elevator", "escalator"}:
            raise ValueError(f"Unsupported connector type: {ctype}")
        from_cell = _parse_endpoint(raw.get("from"))
        to_cell = _parse_endpoint(raw.get("to"))
        if not self.is_walkable(from_cell) or not self.is_walkable(to_cell):
            raise ValueError(f"Connector {raw.get('id', ctype)} endpoints must be walkable")
        return Connector(
            id=str(raw.get("id", ctype)),
            type=ctype,
            from_cell=from_cell,
            to_cell=to_cell,
            bidirectional=bool(raw.get("bidirectional", True)),
            width=float(raw.get("width", 1.0)),
            speed_multiplier=float(raw.get("speed_multiplier", _default_connector_speed(ctype))),
            capacity=None if raw.get("capacity") is None else int(raw["capacity"]),
            flow_rate=None if raw.get("flow_rate") is None else float(raw["flow_rate"]),
            queue_mode=str(raw.get("queue_mode", "fifo")),
            panic_jam_density=None if raw.get("panic_jam_density") is None else float(raw["panic_jam_density"]),
            jam_flow_multiplier=float(raw.get("jam_flow_multiplier", 0.35)),
            dwell_s=float(raw.get("dwell_s", 0.0)),
            travel_s=float(raw.get("travel_s", 0.0)),
            height_delta_m=abs(
                (self.floor_z(to_cell[0]) + self.cell_height(to_cell))
                - (self.floor_z(from_cell[0]) + self.cell_height(from_cell))
            ),
        )


def _parse_endpoint(raw: Any) -> Cell:
    if not isinstance(raw, Mapping):
        raise ValueError("Connector endpoint must be a mapping with floor, x, y")
    return (str(raw["floor"]), int(raw["x"]), int(raw["y"]))


def _default_connector_speed(connector_type: str) -> float:
    if connector_type == "ramp":
        return 0.8
    if connector_type == "escalator":
        return 0.9
    if connector_type == "elevator":
        return 1.0
    return 0.65


def _coerce_height_grid(raw: Mapping[str, Any], shape: tuple[int, int]) -> np.ndarray | None:
    if "cell_heights" in raw:
        heights = np.array(raw["cell_heights"], dtype=float)
    elif "height_grid" in raw:
        heights = np.array(raw["height_grid"], dtype=float)
    elif "cell_height_m" in raw:
        heights = np.full(shape, float(raw["cell_height_m"]), dtype=float)
    else:
        return None
    if heights.shape != shape:
        raise ValueError("Floor cell height grid must match floor text shape")
    return heights
