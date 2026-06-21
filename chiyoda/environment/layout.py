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
    dwell_s: float = 0.0
    travel_s: float = 0.0


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
                parsed[fid] = Floor(id=fid, z=float(floor.z), grid=np.array(floor.grid, copy=True))
            elif isinstance(floor, np.ndarray):
                parsed[fid] = Floor(id=fid, z=0.0, grid=np.array(floor, copy=True))
            else:
                parsed[fid] = Floor(
                    id=fid,
                    z=float(floor.get("z", 0.0)),
                    grid=np.array(floor["grid"], dtype="<U1"),
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
    def from_text(cls, text: str, *, floor_id: str = "0", z: float = 0.0) -> "Layout":
        lines = [list(line.rstrip("\n")) for line in text.splitlines() if line.strip()]
        if not lines:
            raise ValueError("Layout text must contain at least one non-empty row")
        max_len = max(len(row) for row in lines)
        padded = [row + [EMPTY] * (max_len - len(row)) for row in lines]
        grid = np.array(padded, dtype="<U1")
        return cls(floors={str(floor_id): Floor(id=str(floor_id), z=float(z), grid=grid)}, primary_floor_id=str(floor_id))

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
            parsed[floor_id] = cls.from_text(
                str(floor["text"]),
                floor_id=floor_id,
                z=float(floor.get("z", 0.0)),
            ).floors[floor_id]
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

    def world_position(self, cell: Cell) -> np.ndarray:
        floor_id, x, y = self.cell(cell)
        return np.array([float(x) + 0.5, float(y) + 0.5, self.floor_z(floor_id)], dtype=float)

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
            floors={key: Floor(id=floor.id, z=floor.z, grid=np.array(floor.grid, copy=True)) for key, floor in self.floors.items()},
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
            dwell_s=float(raw.get("dwell_s", 0.0)),
            travel_s=float(raw.get("travel_s", 0.0)),
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
