from __future__ import annotations

from dataclasses import dataclass
from typing import List, Tuple, Optional
import numpy as np


WALL = "X"
PERSON = "@"
EXIT = "E"
EMPTY = "."


@dataclass
class Layout:
    grid: np.ndarray  # 2D array of str tokens
    origin: Tuple[float, float] = (0.0, 0.0)
    cell_size: float = 1.0

    @classmethod
    def from_text(cls, text: str) -> "Layout":
        lines = [list(line.rstrip("\n")) for line in text.splitlines() if line.strip()]
        max_len = max(len(r) for r in lines)
        padded = [r + [EMPTY] * (max_len - len(r)) for r in lines]
        grid = np.array(padded, dtype="<U1")
        return cls(grid=grid)

    @classmethod
    def from_file(cls, path: str) -> "Layout":
        with open(path, "r") as f:
            return cls.from_text(f.read())

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
    def height(self) -> int:
        return int(self.grid.shape[0])

    @property
    def width(self) -> int:
        return int(self.grid.shape[1])

    def is_walkable(self, pos: Tuple[int, int]) -> bool:
        y, x = pos[1], pos[0]
        if y < 0 or y >= self.height or x < 0 or x >= self.width:
            return False
        return self.grid[y, x] != WALL

    def is_exit(self, pos: np.ndarray | Tuple[float, float]) -> bool:
        x, y = int(round(pos[0])), int(round(pos[1]))
        if y < 0 or y >= self.height or x < 0 or x >= self.width:
            return False
        return self.grid[y, x] == EXIT

    def people_positions(self) -> List[Tuple[int, int]]:
        ys, xs = np.where(self.grid == PERSON)
        return list(zip(xs.tolist(), ys.tolist()))

    def exit_positions(self) -> List[Tuple[int, int]]:
        ys, xs = np.where(self.grid == EXIT)
        return list(zip(xs.tolist(), ys.tolist()))

    def random_walkable_position(self) -> Tuple[int, int]:
        ys, xs = np.where(self.grid != WALL)
        idx = np.random.randint(0, len(xs))
        return int(xs[idx]), int(ys[idx])

    def clone(self) -> "Layout":
        return Layout(
            grid=np.array(self.grid, copy=True),
            origin=tuple(self.origin),
            cell_size=float(self.cell_size),
        )
