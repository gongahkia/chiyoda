from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any, Mapping

from chiyoda.environment.obstacles import rasterize_geojson_layout


GTFS_CONNECTOR_MODES = {
    "2": "stairs",
    "3": "ramp",
    "4": "escalator",
    "5": "elevator",
}


def strict_layout_from_geojson(
    source: str | Path | Mapping[str, Any],
    *,
    cell_size: float = 1.0,
    padding: int = 1,
    role_property: str = "role",
    add_border_walls: bool = True,
) -> dict[str, Any]:
    payload = _load_geojson(source)
    features = [
        feature for feature in payload.get("features", []) if isinstance(feature, dict)
    ]
    if not features:
        raise ValueError("GeoJSON source must contain features")

    bounds = _bounds(features)
    floor_ids = _floor_ids(features)
    z_by_floor = _z_by_floor(floor_ids)
    floors = []
    floor_grids: dict[str, list[list[str]]] = {}
    origin = None
    resolved_cell_size = cell_size

    for floor_id in floor_ids:
        floor_features = [
            feature
            for feature in features
            if floor_id in _feature_levels(feature.get("properties", {}) or {})
            and not _connector_type(feature.get("properties", {}) or {})
        ]
        if not floor_features:
            floor_features = [
                _point_feature(
                    (bounds[0], bounds[1]), {"role": "walkable", "level": floor_id}
                )
            ]
        grid, origin, resolved_cell_size = rasterize_geojson_layout(
            {"type": "FeatureCollection", "features": floor_features},
            cell_size=cell_size,
            padding=padding,
            role_property=role_property,
            add_border_walls=add_border_walls,
            bounds=bounds,
        )
        rows = [
            ["." if token not in {"X", "E", "@"} else str(token) for token in row]
            for row in grid.tolist()
        ]
        floor_grids[floor_id] = rows
        floors.append(
            {
                "id": floor_id,
                "z": z_by_floor[floor_id],
                "text": "\n".join("".join(row) for row in rows),
            }
        )

    connectors = []
    for feature in features:
        properties = feature.get("properties", {}) or {}
        connector_type = _connector_type(properties)
        if connector_type is None:
            continue
        levels = _connector_levels(properties)
        if len(levels) < 2:
            continue
        endpoints = _line_endpoints(feature.get("geometry", {}) or {})
        if endpoints is None or origin is None:
            continue
        source_cell = _point_to_cell(endpoints[0], origin, resolved_cell_size)
        target_cell = _point_to_cell(endpoints[1], origin, resolved_cell_size)
        from_floor, to_floor = levels[0], levels[-1]
        if from_floor not in floor_grids or to_floor not in floor_grids:
            continue
        _mark_walkable(floor_grids[from_floor], source_cell)
        _mark_walkable(floor_grids[to_floor], target_cell)
        connector = {
            "id": str(
                properties.get(
                    "id",
                    properties.get(
                        "pathway_id",
                        properties.get(
                            "osm_id", f"{connector_type}_{len(connectors)+1}"
                        ),
                    ),
                )
            ),
            "type": connector_type,
            "from": {"floor": from_floor, "x": source_cell[0], "y": source_cell[1]},
            "to": {"floor": to_floor, "x": target_cell[0], "y": target_cell[1]},
            "bidirectional": str(properties.get("is_bidirectional", "1")) != "0",
            "width": float(
                properties.get("width", properties.get("min_width", 1.0)) or 1.0
            ),
        }
        if connector_type == "elevator":
            if properties.get("capacity") is not None:
                connector["capacity"] = int(properties["capacity"])
            if properties.get("dwell_s") is not None:
                connector["dwell_s"] = float(properties["dwell_s"])
            if properties.get("travel_s") is not None:
                connector["travel_s"] = float(properties["travel_s"])
        connectors.append(connector)

    for floor in floors:
        grid = floor_grids[floor["id"]]
        floor["text"] = "\n".join("".join(row) for row in grid)

    return {
        "cell_size": resolved_cell_size,
        "origin": (
            [float(origin[0]), float(origin[1])] if origin is not None else [0.0, 0.0]
        ),
        "floors": floors,
        "connectors": connectors,
    }


def strict_scenario_from_geojson(
    source: str | Path | Mapping[str, Any],
    *,
    name: str = "converted_station",
    cell_size: float = 1.0,
    padding: int = 1,
) -> dict[str, Any]:
    return {
        "scenario": {
            "name": name,
            "layout": strict_layout_from_geojson(
                source, cell_size=cell_size, padding=padding
            ),
            "population": {"total": 0},
            "simulation": {"max_steps": 1, "random_seed": 42},
        }
    }


def _load_geojson(source: str | Path | Mapping[str, Any]) -> Mapping[str, Any]:
    if isinstance(source, Mapping):
        return source
    return json.loads(Path(source).read_text())


def _floor_ids(features: list[dict[str, Any]]) -> list[str]:
    floors: set[str] = set()
    for feature in features:
        floors.update(_feature_levels(feature.get("properties", {}) or {}))
        floors.update(_connector_levels(feature.get("properties", {}) or {}))
    return sorted(floors or {"0"}, key=_floor_sort_key)


def _feature_levels(properties: Mapping[str, Any]) -> list[str]:
    raw = properties.get(
        "level", properties.get("level_id", properties.get("floor", "0"))
    )
    return _parse_levels(raw)


def _connector_levels(properties: Mapping[str, Any]) -> list[str]:
    if (
        properties.get("from_level") is not None
        and properties.get("to_level") is not None
    ):
        return [str(properties["from_level"]), str(properties["to_level"])]
    if (
        properties.get("from_floor") is not None
        and properties.get("to_floor") is not None
    ):
        return [str(properties["from_floor"]), str(properties["to_floor"])]
    return _parse_levels(properties.get("level", properties.get("level_id", "")))


def _parse_levels(raw: Any) -> list[str]:
    text = str(raw).strip()
    if not text:
        return []
    if ";" in text:
        return [part.strip() for part in text.split(";") if part.strip()]
    if "-" in text and all(
        part.strip().lstrip("-").isdigit() for part in text.split("-", 1)
    ):
        start, end = [int(part) for part in text.split("-", 1)]
        step = 1 if end >= start else -1
        return [str(value) for value in range(start, end + step, step)]
    return [text]


def _z_by_floor(floor_ids: list[str]) -> dict[str, float]:
    values: dict[str, float] = {}
    for index, floor_id in enumerate(floor_ids):
        try:
            values[floor_id] = float(floor_id) * 3.0
        except ValueError:
            values[floor_id] = float(index) * 3.0
    return values


def _floor_sort_key(value: str) -> tuple[int, float | str]:
    try:
        return (0, float(value))
    except ValueError:
        return (1, value)


def _connector_type(properties: Mapping[str, Any]) -> str | None:
    explicit = properties.get("connector_type", properties.get("chiyoda_connector"))
    if explicit:
        return str(explicit).lower()
    mode = str(properties.get("pathway_mode", "")).strip()
    if mode in GTFS_CONNECTOR_MODES:
        return GTFS_CONNECTOR_MODES[mode]
    highway = str(properties.get("highway", "")).lower()
    conveying = str(properties.get("conveying", "")).lower()
    if highway == "elevator":
        return "elevator"
    if highway == "steps" and conveying in {"yes", "forward", "backward", "reversible"}:
        return "escalator"
    if highway == "steps":
        return "stairs"
    if str(properties.get("ramp", "")).lower() in {"yes", "true", "1"}:
        return "ramp"
    return None


def _line_endpoints(
    geometry: Mapping[str, Any],
) -> tuple[tuple[float, float], tuple[float, float]] | None:
    if geometry.get("type") == "LineString":
        coords = geometry.get("coordinates", [])
        if len(coords) >= 2:
            return _point(coords[0]), _point(coords[-1])
    if geometry.get("type") == "Point":
        point = _point(geometry.get("coordinates", [0.0, 0.0]))
        return point, point
    return None


def _point(value: Any) -> tuple[float, float]:
    return (float(value[0]), float(value[1]))


def _point_to_cell(
    point: tuple[float, float], origin: tuple[float, float], cell_size: float
) -> tuple[int, int]:
    return (
        int(math.floor((point[0] - origin[0]) / cell_size)),
        int(math.floor((point[1] - origin[1]) / cell_size)),
    )


def _mark_walkable(grid: list[list[str]], cell: tuple[int, int]) -> None:
    x, y = cell
    if 0 <= y < len(grid) and 0 <= x < len(grid[y]) and grid[y][x] == "X":
        grid[y][x] = "."


def _bounds(features: list[dict[str, Any]]) -> tuple[float, float, float, float]:
    xs: list[float] = []
    ys: list[float] = []
    for feature in features:
        _collect_coords(feature.get("geometry", {}) or {}, xs, ys)
    if not xs or not ys:
        raise ValueError("GeoJSON source has no supported coordinates")
    return min(xs), min(ys), max(xs), max(ys)


def _collect_coords(
    geometry: Mapping[str, Any], xs: list[float], ys: list[float]
) -> None:
    coords = geometry.get("coordinates")
    if coords is None:
        return
    if geometry.get("type") == "Point":
        xs.append(float(coords[0]))
        ys.append(float(coords[1]))
        return
    if isinstance(coords, list):
        for item in coords:
            if isinstance(item, list) and item and isinstance(item[0], (int, float)):
                xs.append(float(item[0]))
                ys.append(float(item[1]))
            else:
                _collect_coords({"coordinates": item}, xs, ys)


def _point_feature(
    point: tuple[float, float], properties: dict[str, Any]
) -> dict[str, Any]:
    return {
        "type": "Feature",
        "properties": properties,
        "geometry": {"type": "Point", "coordinates": [point[0], point[1]]},
    }
