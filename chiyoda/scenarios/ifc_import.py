from __future__ import annotations

import importlib
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

from chiyoda.environment.layout import EMPTY, EXIT, WALL

WALKABLE_TYPES = ("IfcSpace", "IfcSlab")
WALL_TYPES = ("IfcWall", "IfcWallStandardCase", "IfcColumn", "IfcCurtainWall")
EXIT_TYPES = ("IfcDoor",)
CONNECTOR_TYPES = ("IfcStair", "IfcRamp", "IfcTransportElement")


@dataclass(frozen=True)
class _FloorSpec:
    id: str
    z: float


@dataclass(frozen=True)
class _IfcFeature:
    role: str
    ifc_type: str
    name: str
    bounds: tuple[float, float, float, float, float, float]


def strict_layout_from_ifc(
    source: str | Path,
    *,
    cell_size: float = 1.0,
    padding: int = 1,
    add_border_walls: bool = True,
) -> dict[str, Any]:
    """Lower IFC element geometry to strict layout.floors."""
    if cell_size <= 0:
        raise ValueError("cell_size must be positive")
    if padding < 0:
        raise ValueError("padding must be non-negative")

    ifcopenshell, geom = _import_ifcopenshell()
    model = ifcopenshell.open(str(source))
    settings = geom.settings()
    _set_geom_setting(settings, "use-world-coords", True)

    features = _collect_features(model, geom, settings)
    if not features:
        raise ValueError("IFC source did not contain supported element geometry")

    floors = _model_floors(model) or _infer_floors(features)
    if not floors:
        raise ValueError("IFC source did not contain usable floor elevations")

    origin, shape = _layout_bounds(features, cell_size=cell_size, padding=padding)
    floor_features = _features_by_floor(features, floors)
    grids: dict[str, list[list[str]]] = {}

    for floor in floors:
        items = floor_features.get(floor.id, [])
        has_walkable = any(item.role in {"walkable", "connector"} for item in items)
        fill = WALL if has_walkable else EMPTY
        grid = np.full(shape, fill, dtype="<U1")
        if add_border_walls:
            _add_border_walls(grid)
        for role in ("walkable", "connector", "wall", "exit"):
            token = _role_token(role)
            for feature in items:
                if feature.role != role:
                    continue
                _paint_bounds(grid, feature.bounds, origin, cell_size, token)
        grids[floor.id] = grid.tolist()

    connectors = _connector_payloads(features, floors, origin, cell_size, grids)
    floor_payloads = [
        {
            "id": floor.id,
            "z": floor.z,
            "text": "\n".join("".join(row) for row in grids[floor.id]),
        }
        for floor in floors
    ]
    return {
        "cell_size": float(cell_size),
        "origin": [float(origin[0]), float(origin[1])],
        "floors": floor_payloads,
        "connectors": connectors,
    }


def _import_ifcopenshell():
    try:
        ifcopenshell = importlib.import_module("ifcopenshell")
        geom = importlib.import_module("ifcopenshell.geom")
    except ModuleNotFoundError as exc:
        raise ImportError(
            "IFC import requires optional dependency 'ifcopenshell'"
        ) from exc
    return ifcopenshell, geom


def _collect_features(model, geom, settings) -> list[_IfcFeature]:
    features: list[_IfcFeature] = []
    seen: set[int] = set()
    for role, type_names in (
        ("walkable", WALKABLE_TYPES),
        ("wall", WALL_TYPES),
        ("exit", EXIT_TYPES),
        ("connector", CONNECTOR_TYPES),
    ):
        for type_name in type_names:
            for element in _by_type(model, type_name):
                key = id(element)
                if key in seen:
                    continue
                seen.add(key)
                bounds = _element_bounds(element, geom, settings)
                if bounds is None:
                    continue
                features.append(
                    _IfcFeature(
                        role=role,
                        ifc_type=str(_element_type(element, type_name)),
                        name=str(getattr(element, "Name", None) or type_name),
                        bounds=bounds,
                    )
                )
    return features


def _by_type(model, type_name: str) -> list[Any]:
    try:
        return list(model.by_type(type_name))
    except (AttributeError, RuntimeError, ValueError):
        return []


def _element_type(element, fallback: str) -> str:
    try:
        return str(element.is_a())
    except TypeError:
        return fallback


def _element_bounds(
    element, geom, settings
) -> tuple[float, float, float, float, float, float] | None:
    try:
        shape = geom.create_shape(settings, element)
        verts = np.array(shape.geometry.verts, dtype=float).reshape((-1, 3))
    except Exception:
        return None
    if verts.size == 0:
        return None
    mins = np.min(verts, axis=0)
    maxs = np.max(verts, axis=0)
    return (
        float(mins[0]),
        float(mins[1]),
        float(maxs[0]),
        float(maxs[1]),
        float(mins[2]),
        float(maxs[2]),
    )


def _set_geom_setting(settings, key: str, value: Any) -> None:
    try:
        settings.set(key, value)
    except Exception:
        return


def _model_floors(model) -> list[_FloorSpec]:
    floors = []
    for index, storey in enumerate(_by_type(model, "IfcBuildingStorey")):
        floor_id = _floor_id(storey, index)
        floors.append(_FloorSpec(id=floor_id, z=_storey_z(storey, default=index * 3.0)))
    return sorted(floors, key=lambda floor: floor.z)


def _floor_id(storey, index: int) -> str:
    for attr in ("LongName", "Name", "GlobalId"):
        value = getattr(storey, attr, None)
        if value:
            return str(value)
    return str(index)


def _storey_z(storey, *, default: float) -> float:
    value = getattr(storey, "Elevation", None)
    if value is not None:
        return float(value)
    return float(default)


def _infer_floors(features: list[_IfcFeature]) -> list[_FloorSpec]:
    elevations = sorted(
        {_nearest_grid_z(_center_z(feature.bounds)) for feature in features}
    )
    return [_FloorSpec(id=str(index), z=z) for index, z in enumerate(elevations)]


def _nearest_grid_z(z: float) -> float:
    return round(float(z) / 3.0) * 3.0


def _center_z(bounds: tuple[float, float, float, float, float, float]) -> float:
    return (bounds[4] + bounds[5]) / 2.0


def _layout_bounds(
    features: list[_IfcFeature],
    *,
    cell_size: float,
    padding: int,
) -> tuple[tuple[float, float], tuple[int, int]]:
    min_x = min(feature.bounds[0] for feature in features) - padding * cell_size
    min_y = min(feature.bounds[1] for feature in features) - padding * cell_size
    max_x = max(feature.bounds[2] for feature in features) + padding * cell_size
    max_y = max(feature.bounds[3] for feature in features) + padding * cell_size
    width = max(1, int(math.ceil((max_x - min_x) / cell_size)))
    height = max(1, int(math.ceil((max_y - min_y) / cell_size)))
    return (float(min_x), float(min_y)), (height, width)


def _features_by_floor(
    features: list[_IfcFeature],
    floors: list[_FloorSpec],
) -> dict[str, list[_IfcFeature]]:
    grouped: dict[str, list[_IfcFeature]] = {floor.id: [] for floor in floors}
    for feature in features:
        floor = min(floors, key=lambda item: abs(item.z - _center_z(feature.bounds)))
        grouped[floor.id].append(feature)
    return grouped


def _role_token(role: str) -> str:
    if role == "wall":
        return WALL
    if role == "exit":
        return EXIT
    return EMPTY


def _paint_bounds(
    grid: np.ndarray,
    bounds: tuple[float, float, float, float, float, float],
    origin: tuple[float, float],
    cell_size: float,
    token: str,
) -> None:
    x0, y0, x1, y1 = _cell_span(bounds, origin, cell_size, grid.shape)
    grid[y0 : y1 + 1, x0 : x1 + 1] = token


def _cell_span(
    bounds: tuple[float, float, float, float, float, float],
    origin: tuple[float, float],
    cell_size: float,
    shape: tuple[int, int],
) -> tuple[int, int, int, int]:
    height, width = shape
    x0 = int(math.floor((bounds[0] - origin[0]) / cell_size))
    y0 = int(math.floor((bounds[1] - origin[1]) / cell_size))
    x1 = max(x0, int(math.ceil((bounds[2] - origin[0]) / cell_size)) - 1)
    y1 = max(y0, int(math.ceil((bounds[3] - origin[1]) / cell_size)) - 1)
    return (
        min(max(x0, 0), width - 1),
        min(max(y0, 0), height - 1),
        min(max(x1, 0), width - 1),
        min(max(y1, 0), height - 1),
    )


def _add_border_walls(grid: np.ndarray) -> None:
    grid[0, :] = WALL
    grid[-1, :] = WALL
    grid[:, 0] = WALL
    grid[:, -1] = WALL


def _connector_payloads(
    features: list[_IfcFeature],
    floors: list[_FloorSpec],
    origin: tuple[float, float],
    cell_size: float,
    grids: dict[str, list[list[str]]],
) -> list[dict[str, Any]]:
    connectors = []
    for feature in features:
        if feature.role != "connector":
            continue
        spanned = _spanned_floors(feature.bounds, floors)
        if len(spanned) < 2:
            continue
        source = _center_cell(feature.bounds, origin, cell_size, grids[spanned[0].id])
        target = _center_cell(feature.bounds, origin, cell_size, grids[spanned[-1].id])
        grids[spanned[0].id][source[1]][source[0]] = EMPTY
        grids[spanned[-1].id][target[1]][target[0]] = EMPTY
        connectors.append(
            {
                "id": feature.name,
                "type": _connector_type(feature.ifc_type),
                "from": {"floor": spanned[0].id, "x": source[0], "y": source[1]},
                "to": {"floor": spanned[-1].id, "x": target[0], "y": target[1]},
                "bidirectional": True,
            }
        )
    return connectors


def _spanned_floors(
    bounds: tuple[float, float, float, float, float, float],
    floors: list[_FloorSpec],
) -> list[_FloorSpec]:
    z0, z1 = sorted((bounds[4], bounds[5]))
    spanned = [floor for floor in floors if z0 <= floor.z <= z1]
    if len(spanned) >= 2:
        return spanned
    nearest_low = min(floors, key=lambda floor: abs(floor.z - z0))
    nearest_high = min(floors, key=lambda floor: abs(floor.z - z1))
    return sorted({nearest_low, nearest_high}, key=lambda floor: floor.z)


def _center_cell(
    bounds: tuple[float, float, float, float, float, float],
    origin: tuple[float, float],
    cell_size: float,
    grid: list[list[str]],
) -> tuple[int, int]:
    center_x = (bounds[0] + bounds[2]) / 2.0
    center_y = (bounds[1] + bounds[3]) / 2.0
    width = len(grid[0])
    height = len(grid)
    x = int(math.floor((center_x - origin[0]) / cell_size))
    y = int(math.floor((center_y - origin[1]) / cell_size))
    return min(max(x, 0), width - 1), min(max(y, 0), height - 1)


def _connector_type(ifc_type: str) -> str:
    text = ifc_type.lower()
    if "ramp" in text:
        return "ramp"
    if "transport" in text:
        return "elevator"
    return "stairs"


__all__ = ["strict_layout_from_ifc"]
