from __future__ import annotations

import csv
import math
from pathlib import Path
from typing import Any

GTFS_CONNECTOR_MODES = {
    "2": "stairs",
    "3": "ramp",
    "4": "escalator",
    "5": "elevator",
}


def strict_scenario_from_gtfs_pathways(
    feed_dir: str | Path,
    *,
    name: str = "gtfs_pathways_import",
    cell_size: float = 1.0,
    padding: int = 1,
) -> dict[str, Any]:
    layout, metadata = strict_layout_from_gtfs_pathways(
        feed_dir, cell_size=cell_size, padding=padding
    )
    return {
        "scenario": {
            "name": name,
            "metadata": {"gtfs_pathways": metadata},
            "layout": layout,
            "population": {"total": 0},
            "simulation": {"max_steps": 1, "random_seed": 42},
        }
    }


def strict_layout_from_gtfs_pathways(
    feed_dir: str | Path,
    *,
    cell_size: float = 1.0,
    padding: int = 1,
) -> tuple[dict[str, Any], dict[str, Any]]:
    root = Path(feed_dir)
    stops = _read_table(root / "stops.txt")
    pathways = _read_table(root / "pathways.txt")
    levels = _read_table(root / "levels.txt") if (root / "levels.txt").exists() else []
    if not stops:
        raise ValueError("GTFS Pathways import requires stops.txt")
    if not pathways:
        raise ValueError("GTFS Pathways import requires pathways.txt")

    stops_by_id = {str(row["stop_id"]): row for row in stops if row.get("stop_id")}
    referenced_ids = {
        stop_id
        for row in pathways
        for stop_id in (row.get("from_stop_id"), row.get("to_stop_id"))
        if stop_id
    }
    nodes = [
        stops_by_id[stop_id] for stop_id in referenced_ids if stop_id in stops_by_id
    ]
    if not nodes:
        raise ValueError("pathways.txt did not reference known stops")

    points = _project_points(nodes)
    level_rows = _level_rows(levels, nodes)
    z_by_level = _z_by_level(level_rows)
    bounds = _bounds(points.values())
    width = max(3, math.ceil(bounds[2] / cell_size) + 1 + padding * 2)
    height = max(3, math.ceil(bounds[3] / cell_size) + 1 + padding * 2)
    grids = {
        level["level_id"]: [["X" for _ in range(width)] for _ in range(height)]
        for level in level_rows
    }
    cells = {
        stop_id: _point_to_cell(point, cell_size=cell_size, padding=padding)
        for stop_id, point in points.items()
    }

    for node in nodes:
        stop_id = str(node["stop_id"])
        level_id = _node_level(node)
        if level_id not in grids:
            continue
        token = "E" if str(node.get("location_type", "")) == "2" else "."
        _mark_cell(grids[level_id], cells[stop_id], token)

    connectors: list[dict[str, Any]] = []
    for pathway in pathways:
        from_id = str(pathway.get("from_stop_id", ""))
        to_id = str(pathway.get("to_stop_id", ""))
        if from_id not in stops_by_id or to_id not in stops_by_id:
            continue
        from_level = _node_level(stops_by_id[from_id])
        to_level = _node_level(stops_by_id[to_id])
        if from_level not in grids or to_level not in grids:
            continue
        from_cell = cells[from_id]
        to_cell = cells[to_id]
        mode = str(pathway.get("pathway_mode", "1") or "1")
        if from_level == to_level or mode not in GTFS_CONNECTOR_MODES:
            _mark_line(grids[from_level], from_cell, to_cell)
            continue
        _mark_cell(grids[from_level], from_cell, ".")
        _mark_cell(grids[to_level], to_cell, ".")
        connector: dict[str, Any] = {
            "id": str(pathway.get("pathway_id") or f"pathway_{len(connectors) + 1}"),
            "type": GTFS_CONNECTOR_MODES[mode],
            "from": {"floor": from_level, "x": from_cell[0], "y": from_cell[1]},
            "to": {"floor": to_level, "x": to_cell[0], "y": to_cell[1]},
            "bidirectional": str(pathway.get("is_bidirectional", "1")) != "0",
            "width": float(pathway.get("min_width") or 1.0),
        }
        if pathway.get("traversal_time"):
            connector["travel_s"] = float(pathway["traversal_time"])
        connectors.append(connector)

    if not any("E" in row for grid in grids.values() for row in grid):
        first_id = next(iter(cells))
        _mark_cell(grids[_node_level(stops_by_id[first_id])], cells[first_id], "E")

    floors = [
        {
            "id": level["level_id"],
            "z": z_by_level[level["level_id"]],
            "text": "\n".join("".join(row) for row in grids[level["level_id"]]),
        }
        for level in level_rows
    ]
    metadata = {
        "source": "GTFS Pathways",
        "source_files": ["stops.txt", "pathways.txt"]
        + (["levels.txt"] if levels else []),
        "levels": level_rows,
        "nodes": [
            {
                "stop_id": str(node["stop_id"]),
                "stop_name": str(node.get("stop_name", "")),
                "location_type": str(node.get("location_type", "")),
                "level_id": _node_level(node),
                "cell": {
                    "floor": _node_level(node),
                    "x": cells[str(node["stop_id"])][0],
                    "y": cells[str(node["stop_id"])][1],
                },
            }
            for node in nodes
        ],
        "pathways": [
            {
                "pathway_id": str(row.get("pathway_id", "")),
                "from_stop_id": str(row.get("from_stop_id", "")),
                "to_stop_id": str(row.get("to_stop_id", "")),
                "pathway_mode": str(row.get("pathway_mode", "")),
                "is_bidirectional": str(row.get("is_bidirectional", "")),
            }
            for row in pathways
        ],
    }
    return {
        "cell_size": cell_size,
        "origin": [0.0, 0.0],
        "floors": floors,
        "connectors": connectors,
    }, metadata


def _read_table(path: Path) -> list[dict[str, str]]:
    with path.open(newline="") as handle:
        return [dict(row) for row in csv.DictReader(handle)]


def _node_level(node: dict[str, str]) -> str:
    return str(node.get("level_id") or "0")


def _level_rows(
    levels: list[dict[str, str]], nodes: list[dict[str, str]]
) -> list[dict[str, Any]]:
    if levels:
        rows = [
            {
                "level_id": str(row.get("level_id", "")),
                "level_index": float(row.get("level_index") or index),
                "level_name": str(row.get("level_name", "")),
            }
            for index, row in enumerate(levels)
            if row.get("level_id")
        ]
    else:
        ids = sorted({_node_level(node) for node in nodes})
        rows = [
            {"level_id": level_id, "level_index": float(index), "level_name": level_id}
            for index, level_id in enumerate(ids)
        ]
    return sorted(rows, key=lambda row: row["level_index"])


def _z_by_level(levels: list[dict[str, Any]]) -> dict[str, float]:
    return {row["level_id"]: float(row["level_index"]) * 3.0 for row in levels}


def _project_points(nodes: list[dict[str, str]]) -> dict[str, tuple[float, float]]:
    lat_lon = [
        (float(node["stop_lat"]), float(node["stop_lon"]))
        for node in nodes
        if node.get("stop_lat") and node.get("stop_lon")
    ]
    if not lat_lon:
        raise ValueError("GTFS Pathways stops need stop_lat and stop_lon")
    mean_lat = math.radians(sum(lat for lat, _ in lat_lon) / len(lat_lon))
    min_lat = min(lat for lat, _ in lat_lon)
    min_lon = min(lon for _, lon in lat_lon)
    return {
        str(node["stop_id"]): (
            (float(node["stop_lon"]) - min_lon) * 111_320.0 * math.cos(mean_lat),
            (float(node["stop_lat"]) - min_lat) * 110_540.0,
        )
        for node in nodes
    }


def _bounds(points) -> tuple[float, float, float, float]:
    xs = [point[0] for point in points]
    ys = [point[1] for point in points]
    return min(xs), min(ys), max(xs), max(ys)


def _point_to_cell(
    point: tuple[float, float], *, cell_size: float, padding: int
) -> tuple[int, int]:
    return (
        int(math.floor(point[0] / cell_size)) + padding,
        int(math.floor(point[1] / cell_size)) + padding,
    )


def _mark_cell(grid: list[list[str]], cell: tuple[int, int], token: str) -> None:
    x, y = cell
    if 0 <= y < len(grid) and 0 <= x < len(grid[y]):
        grid[y][x] = token


def _mark_line(
    grid: list[list[str]], start: tuple[int, int], end: tuple[int, int]
) -> None:
    x0, y0 = start
    x1, y1 = end
    dx = abs(x1 - x0)
    dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    error = dx + dy
    while True:
        _mark_cell(grid, (x0, y0), ".")
        if x0 == x1 and y0 == y1:
            break
        twice = 2 * error
        if twice >= dy:
            error += dy
            x0 += sx
        if twice <= dx:
            error += dx
            y0 += sy
