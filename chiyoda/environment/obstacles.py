from __future__ import annotations

from dataclasses import dataclass
import json
import math
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence, Tuple

import numpy as np


Cell = Tuple[int, int]
Point = Tuple[float, float]

WALL_TOKEN = "X"
EMPTY_TOKEN = "."
EXIT_TOKEN = "E"
PERSON_TOKEN = "@"

WALKABLE_ROLES = {"walkable", "floor", "room", "corridor", "space"}
OBSTACLE_ROLES = {"obstacle", "wall", "blocked", "blocker"}
EXIT_ROLES = {"exit", "egress", "entrance"}
SPAWN_ROLES = {"spawn", "person", "agent", "start"}

OSM_WALKABLE_INDOOR = {"area", "corridor", "level", "room"}
OSM_OBSTACLE_INDOOR = {"column", "wall"}
OSM_WALKABLE_HIGHWAY = {
    "corridor",
    "elevator",
    "footway",
    "path",
    "pedestrian",
    "steps",
}
OSM_OBSTACLE_BARRIERS = {"block", "fence", "wall"}
GTFS_WALKABLE_LOCATION_TYPES = {"0", "3", "4"}
GTFS_EXIT_LOCATION_TYPES = {"2"}
GTFS_WALKABLE_PATHWAY_MODES = {"1", "2", "3", "4", "5", "6", "7"}


@dataclass(frozen=True)
class ObstacleSpec:
    shape: str
    fill: str = WALL_TOKEN
    cells: Tuple[Cell, ...] = ()
    points: Tuple[Point, ...] = ()
    holes: Tuple[Tuple[Point, ...], ...] = ()
    center: Point = (0.0, 0.0)
    radius: float = 0.0
    thickness: float = 1.0

    @classmethod
    def from_config(cls, config: Mapping[str, Any]) -> "ObstacleSpec":
        shape = str(config.get("shape", config.get("type", "rectangle"))).lower()
        fill = str(config.get("fill", WALL_TOKEN))

        if shape in {"cells", "cell"}:
            cells = tuple(_normalize_cell(cell) for cell in config.get("cells", []))
            if not cells:
                raise ValueError("Cell obstacles require a non-empty cells list")
            return cls(shape="cells", fill=fill, cells=cells)

        if shape in {"rectangle", "rect"}:
            min_x, min_y, max_x, max_y = _rectangle_bounds(config)
            return cls(
                shape="rectangle",
                fill=fill,
                points=((min_x, min_y), (max_x, max_y)),
            )

        if shape in {"circle", "disk"}:
            center = _normalize_point(config.get("center", (config.get("x"), config.get("y"))))
            radius = float(config.get("radius", 0.0))
            return cls(shape="circle", fill=fill, center=center, radius=radius)

        if shape == "point":
            center = _normalize_point(config.get("point", config.get("center", config.get("coordinates"))))
            return cls(shape="point", fill=fill, center=center)

        if shape in {"polygon", "poly"}:
            rings = tuple(tuple(_normalize_point(point) for point in ring) for ring in _polygon_rings(config))
            if not rings or len(rings[0]) < 3:
                raise ValueError("Polygon obstacles require at least three points")
            return cls(
                shape="polygon",
                fill=fill,
                points=rings[0],
                holes=rings[1:],
            )

        if shape in {"line", "polyline"}:
            points = tuple(_normalize_point(point) for point in config.get("points", []))
            if len(points) < 2:
                raise ValueError("Line obstacles require at least two points")
            return cls(
                shape="polyline",
                fill=fill,
                points=points,
                thickness=float(config.get("thickness", 1.0)),
            )

        raise ValueError(f"Unsupported obstacle shape: {shape}")

    def rasterize(
        self,
        width: int,
        height: int,
        *,
        origin: Point = (0.0, 0.0),
        cell_size: float = 1.0,
    ) -> set[Cell]:
        if width <= 0 or height <= 0:
            return set()

        if self.shape == "cells":
            return {
                (x, y)
                for x, y in self.cells
                if 0 <= x < width and 0 <= y < height
            }

        if self.shape == "point":
            return {
                _point_to_cell(self.center, origin=origin, cell_size=cell_size, width=width, height=height)
            }

        if self.shape == "rectangle":
            (min_x, min_y), (max_x, max_y) = self.points
            return {
                (x, y)
                for x, y, center_x, center_y in _candidate_cells(
                    width,
                    height,
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                    origin=origin,
                    cell_size=cell_size,
                )
                if min_x <= center_x <= max_x and min_y <= center_y <= max_y
            }

        if self.shape == "circle":
            min_x = self.center[0] - self.radius
            min_y = self.center[1] - self.radius
            max_x = self.center[0] + self.radius
            max_y = self.center[1] + self.radius
            radius_sq = self.radius * self.radius
            return {
                (x, y)
                for x, y, center_x, center_y in _candidate_cells(
                    width,
                    height,
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                    origin=origin,
                    cell_size=cell_size,
                )
                if ((center_x - self.center[0]) ** 2 + (center_y - self.center[1]) ** 2) <= radius_sq
            }

        if self.shape == "polygon":
            xs = [point[0] for point in self.points]
            ys = [point[1] for point in self.points]
            min_x, max_x = min(xs), max(xs)
            min_y, max_y = min(ys), max(ys)
            return {
                (x, y)
                for x, y, center_x, center_y in _candidate_cells(
                    width,
                    height,
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                    origin=origin,
                    cell_size=cell_size,
                )
                if _point_in_ring((center_x, center_y), self.points)
                and not any(_point_in_ring((center_x, center_y), hole) for hole in self.holes)
            }

        if self.shape == "polyline":
            xs = [point[0] for point in self.points]
            ys = [point[1] for point in self.points]
            half_thickness = max(self.thickness, cell_size) / 2.0
            min_x, max_x = min(xs) - half_thickness, max(xs) + half_thickness
            min_y, max_y = min(ys) - half_thickness, max(ys) + half_thickness
            return {
                (x, y)
                for x, y, center_x, center_y in _candidate_cells(
                    width,
                    height,
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                    origin=origin,
                    cell_size=cell_size,
                )
                if any(
                    _point_segment_distance((center_x, center_y), start, end) <= half_thickness
                    for start, end in zip(self.points[:-1], self.points[1:])
                )
            }

        raise ValueError(f"Unsupported obstacle shape: {self.shape}")


def obstacles_from_config(configs: Sequence[Mapping[str, Any]]) -> list[ObstacleSpec]:
    return [ObstacleSpec.from_config(config) for config in configs]


def apply_obstacles_to_grid(
    grid: np.ndarray,
    obstacles: Sequence[ObstacleSpec],
    *,
    origin: Point = (0.0, 0.0),
    cell_size: float = 1.0,
) -> np.ndarray:
    updated = np.array(grid, copy=True)
    height, width = updated.shape
    for obstacle in obstacles:
        for x, y in obstacle.rasterize(width, height, origin=origin, cell_size=cell_size):
            updated[y, x] = obstacle.fill
    return updated


def rasterize_geojson_layout(
    source: str | Path | Mapping[str, Any],
    *,
    cell_size: float = 1.0,
    padding: int = 1,
    role_property: str = "role",
    default_token: str | None = None,
    add_border_walls: bool = False,
) -> tuple[np.ndarray, Point, float]:
    payload = _load_geojson_payload(source)
    features = _geojson_features(payload)
    if not features:
        raise ValueError("GeoJSON layout must contain at least one feature")

    min_x, min_y, max_x, max_y = _feature_bounds(features)
    origin = (min_x - (padding * cell_size), min_y - (padding * cell_size))
    width = max(1, int(math.ceil((max_x - min_x) / cell_size)) + (padding * 2) + 1)
    height = max(1, int(math.ceil((max_y - min_y) / cell_size)) + (padding * 2) + 1)

    roles = [_feature_role(feature, role_property=role_property) for feature in features]
    fill = default_token
    if fill is None:
        fill = WALL_TOKEN if any(role in WALKABLE_ROLES for role in roles) else EMPTY_TOKEN

    grid = np.full((height, width), fill, dtype="<U1")
    if add_border_walls and height >= 2 and width >= 2:
        grid[0, :] = WALL_TOKEN
        grid[-1, :] = WALL_TOKEN
        grid[:, 0] = WALL_TOKEN
        grid[:, -1] = WALL_TOKEN

    for feature in features:
        role = _feature_role(feature, role_property=role_property)
        token = _role_token(role)
        for obstacle in _feature_obstacles(feature, fill=token):
            for x, y in obstacle.rasterize(width, height, origin=origin, cell_size=cell_size):
                grid[y, x] = token

    return grid, origin, cell_size


def rasterize_dxf_layout(
    source: str | Path | Mapping[str, Any],
    *,
    cell_size: float = 1.0,
    padding: int = 1,
    role_layers: Mapping[str, Sequence[str]] | None = None,
    default_role: str = "obstacle",
    default_token: str | None = None,
    add_border_walls: bool = False,
    line_thickness: float = 1.0,
) -> tuple[np.ndarray, Point, float]:
    entities = _load_dxf_entities(source)
    if not entities:
        raise ValueError("DXF layout must contain at least one supported entity")

    min_x, min_y, max_x, max_y = _dxf_entity_bounds(entities)
    origin = (min_x - (padding * cell_size), min_y - (padding * cell_size))
    width = max(1, int(math.ceil((max_x - min_x) / cell_size)) + (padding * 2) + 1)
    height = max(1, int(math.ceil((max_y - min_y) / cell_size)) + (padding * 2) + 1)

    normalized_role_layers = _normalize_role_layers(role_layers)
    resolved_roles = [
        _resolve_cad_role(entity["layer"], normalized_role_layers, default_role)
        for entity in entities
    ]

    fill = default_token
    if fill is None:
        fill = WALL_TOKEN if any(role in WALKABLE_ROLES for role in resolved_roles) else EMPTY_TOKEN

    grid = np.full((height, width), fill, dtype="<U1")
    if add_border_walls and height >= 2 and width >= 2:
        grid[0, :] = WALL_TOKEN
        grid[-1, :] = WALL_TOKEN
        grid[:, 0] = WALL_TOKEN
        grid[:, -1] = WALL_TOKEN

    for entity, role in zip(entities, resolved_roles):
        token = _role_token(role)
        for obstacle in _dxf_entity_obstacles(
            entity,
            fill=token,
            line_thickness=line_thickness,
            fill_closed=(role in WALKABLE_ROLES or role in OBSTACLE_ROLES),
        ):
            for x, y in obstacle.rasterize(width, height, origin=origin, cell_size=cell_size):
                grid[y, x] = token

    return grid, origin, cell_size


def _load_geojson_payload(source: str | Path | Mapping[str, Any]) -> Mapping[str, Any]:
    if isinstance(source, Mapping):
        return source

    path = Path(source).resolve()
    with path.open("r") as handle:
        return json.load(handle)


def _load_dxf_entities(source: str | Path | Mapping[str, Any]) -> list[dict[str, Any]]:
    if isinstance(source, Mapping):
        text = str(source.get("text", ""))
    else:
        path = Path(source)
        if path.exists():
            text = path.read_text()
        else:
            text = str(source)

    pairs = _dxf_pairs(text)
    entities: list[dict[str, Any]] = []
    in_entities = False
    index = 0

    while index < len(pairs):
        code, value = pairs[index]
        if code == "0" and value == "SECTION":
            index += 1
            if index < len(pairs) and pairs[index][0] == "2":
                in_entities = pairs[index][1].upper() == "ENTITIES"
            index += 1
            continue

        if code == "0" and value == "ENDSEC":
            in_entities = False
            index += 1
            continue

        if not in_entities or code != "0":
            index += 1
            continue

        entity_type = value.upper()
        if entity_type == "LINE":
            entity, index = _parse_simple_dxf_entity(pairs, index + 1, entity_type)
            if entity is not None:
                entities.append(entity)
            continue

        if entity_type == "CIRCLE":
            entity, index = _parse_simple_dxf_entity(pairs, index + 1, entity_type)
            if entity is not None:
                entities.append(entity)
            continue

        if entity_type == "POINT":
            entity, index = _parse_simple_dxf_entity(pairs, index + 1, entity_type)
            if entity is not None:
                entities.append(entity)
            continue

        if entity_type == "LWPOLYLINE":
            entity, index = _parse_lwpolyline_entity(pairs, index + 1)
            if entity is not None:
                entities.append(entity)
            continue

        if entity_type == "POLYLINE":
            entity, index = _parse_polyline_entity(pairs, index + 1)
            if entity is not None:
                entities.append(entity)
            continue

        index += 1
        while index < len(pairs) and pairs[index][0] != "0":
            index += 1

    return entities


def _dxf_pairs(text: str) -> list[tuple[str, str]]:
    lines = [line.rstrip("\r") for line in text.splitlines()]
    if len(lines) % 2 != 0:
        raise ValueError("DXF content must contain code/value pairs")
    return [(lines[i].strip(), lines[i + 1].strip()) for i in range(0, len(lines), 2)]


def _parse_simple_dxf_entity(
    pairs: Sequence[tuple[str, str]],
    index: int,
    entity_type: str,
) -> tuple[dict[str, Any] | None, int]:
    data: dict[str, Any] = {"type": entity_type, "layer": "0"}
    while index < len(pairs):
        code, value = pairs[index]
        if code == "0":
            break
        if code == "8":
            data["layer"] = value
        elif code in {"10", "20", "11", "21", "40"}:
            data[code] = float(value)
        index += 1

    if entity_type == "LINE":
        if not {"10", "20", "11", "21"}.issubset(data):
            return None, index
        return (
            {
                "type": "LINE",
                "layer": data["layer"],
                "points": ((data["10"], data["20"]), (data["11"], data["21"])),
            },
            index,
        )

    if entity_type == "CIRCLE":
        if not {"10", "20", "40"}.issubset(data):
            return None, index
        return (
            {
                "type": "CIRCLE",
                "layer": data["layer"],
                "center": (data["10"], data["20"]),
                "radius": float(data["40"]),
            },
            index,
        )

    if entity_type == "POINT":
        if not {"10", "20"}.issubset(data):
            return None, index
        return (
            {
                "type": "POINT",
                "layer": data["layer"],
                "point": (data["10"], data["20"]),
            },
            index,
        )

    return None, index


def _parse_lwpolyline_entity(
    pairs: Sequence[tuple[str, str]],
    index: int,
) -> tuple[dict[str, Any] | None, int]:
    layer = "0"
    closed = False
    points: list[Point] = []
    current_x: float | None = None

    while index < len(pairs):
        code, value = pairs[index]
        if code == "0":
            break
        if code == "8":
            layer = value
        elif code == "70":
            closed = (int(float(value)) & 1) == 1
        elif code == "10":
            current_x = float(value)
        elif code == "20" and current_x is not None:
            points.append((current_x, float(value)))
            current_x = None
        index += 1

    if len(points) < 2:
        return None, index
    return (
        {
            "type": "LWPOLYLINE",
            "layer": layer,
            "points": tuple(points),
            "closed": closed,
        },
        index,
    )


def _parse_polyline_entity(
    pairs: Sequence[tuple[str, str]],
    index: int,
) -> tuple[dict[str, Any] | None, int]:
    layer = "0"
    closed = False
    points: list[Point] = []

    while index < len(pairs):
        code, value = pairs[index]
        if code == "0":
            break
        if code == "8":
            layer = value
        elif code == "70":
            closed = (int(float(value)) & 1) == 1
        index += 1

    while index < len(pairs):
        code, value = pairs[index]
        if code != "0":
            index += 1
            continue
        marker = value.upper()
        if marker == "VERTEX":
            vertex, index = _parse_polyline_vertex(pairs, index + 1)
            if vertex is not None:
                points.append(vertex)
            continue
        if marker == "SEQEND":
            index += 1
            break
        break

    if len(points) < 2:
        return None, index
    return (
        {
            "type": "POLYLINE",
            "layer": layer,
            "points": tuple(points),
            "closed": closed,
        },
        index,
    )


def _parse_polyline_vertex(
    pairs: Sequence[tuple[str, str]],
    index: int,
) -> tuple[Point | None, int]:
    x: float | None = None
    y: float | None = None
    while index < len(pairs):
        code, value = pairs[index]
        if code == "0":
            break
        if code == "10":
            x = float(value)
        elif code == "20":
            y = float(value)
        index += 1
    if x is None or y is None:
        return None, index
    return (x, y), index


def _normalize_role_layers(
    role_layers: Mapping[str, Sequence[str]] | None,
) -> dict[str, set[str]]:
    normalized: dict[str, set[str]] = {
        "walkable": set(),
        "obstacle": set(),
        "exit": set(),
        "spawn": set(),
    }
    if role_layers is None:
        return normalized
    for role, layers in role_layers.items():
        normalized[str(role).lower()] = {str(layer).upper() for layer in layers}
    return normalized


def _resolve_cad_role(
    layer: str,
    role_layers: Mapping[str, set[str]],
    default_role: str,
) -> str:
    normalized_layer = str(layer).upper()
    for role, layers in role_layers.items():
        if normalized_layer in layers:
            return role
    return default_role.lower()


def _dxf_entity_obstacles(
    entity: Mapping[str, Any],
    *,
    fill: str,
    line_thickness: float,
    fill_closed: bool,
) -> list[ObstacleSpec]:
    entity_type = str(entity.get("type", "")).upper()
    if entity_type == "LINE":
        return [
            ObstacleSpec(
                shape="polyline",
                fill=fill,
                points=tuple(entity["points"]),
                thickness=float(line_thickness),
            )
        ]
    if entity_type in {"LWPOLYLINE", "POLYLINE"}:
        points = tuple(entity["points"])
        if entity.get("closed", False) and fill_closed and len(points) >= 3:
            return [ObstacleSpec(shape="polygon", fill=fill, points=points)]
        return [
            ObstacleSpec(
                shape="polyline",
                fill=fill,
                points=points,
                thickness=float(line_thickness),
            )
        ]
    if entity_type == "CIRCLE":
        return [
            ObstacleSpec(
                shape="circle",
                fill=fill,
                center=tuple(entity["center"]),
                radius=float(entity["radius"]),
            )
        ]
    if entity_type == "POINT":
        return [
            ObstacleSpec(
                shape="point",
                fill=fill,
                center=tuple(entity["point"]),
            )
        ]
    return []


def _dxf_entity_bounds(entities: Sequence[Mapping[str, Any]]) -> tuple[float, float, float, float]:
    xs: list[float] = []
    ys: list[float] = []
    for entity in entities:
        entity_type = str(entity.get("type", "")).upper()
        if entity_type in {"LINE", "LWPOLYLINE", "POLYLINE"}:
            for x, y in entity.get("points", ()):
                xs.append(float(x))
                ys.append(float(y))
        elif entity_type == "POINT":
            x, y = entity["point"]
            xs.append(float(x))
            ys.append(float(y))
        elif entity_type == "CIRCLE":
            x, y = entity["center"]
            radius = float(entity["radius"])
            xs.extend([float(x) - radius, float(x) + radius])
            ys.extend([float(y) - radius, float(y) + radius])
    if not xs or not ys:
        raise ValueError("DXF entities do not contain any coordinates")
    return min(xs), min(ys), max(xs), max(ys)


def _geojson_features(payload: Mapping[str, Any]) -> list[Mapping[str, Any]]:
    payload_type = str(payload.get("type", "")).lower()
    if payload_type == "featurecollection":
        return [feature for feature in payload.get("features", []) if isinstance(feature, Mapping)]
    if payload_type == "feature":
        return [payload]
    if "geometry" in payload:
        return [{"type": "Feature", "geometry": payload["geometry"], "properties": payload.get("properties", {})}]
    raise ValueError("Unsupported GeoJSON payload")


def _feature_role(feature: Mapping[str, Any], *, role_property: str) -> str:
    properties = feature.get("properties", {}) or {}
    candidates = [
        properties.get(role_property),
        properties.get("chiyoda_role"),
        properties.get("kind"),
        properties.get("feature"),
        properties.get("usage"),
    ]
    for candidate in candidates:
        normalized = _tag_value(candidate)
        if normalized:
            return normalized

    inferred = _infer_station_feature_role(properties)
    if inferred is not None:
        return inferred
    return "obstacle"


def _infer_station_feature_role(properties: Mapping[str, Any]) -> str | None:
    """Infer Chiyoda layout roles from common station-geometry tags."""
    if _truthy_tag(properties.get("entrance")):
        return "exit"

    location_type = _tag_value(properties.get("location_type"))
    if location_type in GTFS_EXIT_LOCATION_TYPES:
        return "exit"
    if location_type in GTFS_WALKABLE_LOCATION_TYPES:
        return "walkable"

    pathway_mode = _tag_value(properties.get("pathway_mode"))
    if pathway_mode in GTFS_WALKABLE_PATHWAY_MODES:
        return "walkable"

    indoor = _tag_value(properties.get("indoor"))
    if indoor in OSM_OBSTACLE_INDOOR:
        return "obstacle"
    if indoor in OSM_WALKABLE_INDOOR:
        return "walkable"

    barrier = _tag_value(properties.get("barrier"))
    if barrier in OSM_OBSTACLE_BARRIERS:
        return "obstacle"

    highway = _tag_value(properties.get("highway"))
    if highway in OSM_WALKABLE_HIGHWAY:
        return "walkable"

    area_highway = _tag_value(properties.get("area:highway"))
    if area_highway in OSM_WALKABLE_HIGHWAY:
        return "walkable"

    railway = _tag_value(properties.get("railway"))
    if railway in {"platform", "subway_entrance", "train_station_entrance"}:
        return "exit" if railway.endswith("_entrance") else "walkable"

    public_transport = _tag_value(properties.get("public_transport"))
    if public_transport in {"platform", "stop_position"}:
        return "walkable"
    if public_transport in {"entrance", "station_entrance"}:
        return "exit"

    door = _tag_value(properties.get("door"))
    if door and door != "no":
        return "walkable"

    return None


def _tag_value(value: Any) -> str:
    if value is None:
        return ""
    return str(value).strip().lower()


def _truthy_tag(value: Any) -> bool:
    normalized = _tag_value(value)
    return bool(normalized) and normalized not in {"0", "false", "no", "none"}


def _role_token(role: str) -> str:
    if role in WALKABLE_ROLES:
        return EMPTY_TOKEN
    if role in EXIT_ROLES:
        return EXIT_TOKEN
    if role in SPAWN_ROLES:
        return PERSON_TOKEN
    return WALL_TOKEN


def _feature_obstacles(feature: Mapping[str, Any], *, fill: str) -> list[ObstacleSpec]:
    geometry = feature.get("geometry") or {}
    geometry_type = str(geometry.get("type", "")).lower()
    coordinates = geometry.get("coordinates")
    properties = feature.get("properties", {}) or {}
    thickness = float(properties.get("thickness", 1.0))

    if geometry_type == "polygon":
        rings = tuple(tuple(_normalize_point(point) for point in ring) for ring in coordinates or [])
        if not rings:
            return []
        return [
            ObstacleSpec(
                shape="polygon",
                fill=fill,
                points=rings[0],
                holes=rings[1:],
            )
        ]

    if geometry_type == "multipolygon":
        specs: list[ObstacleSpec] = []
        for polygon in coordinates or []:
            rings = tuple(tuple(_normalize_point(point) for point in ring) for ring in polygon)
            if not rings:
                continue
            specs.append(
                ObstacleSpec(
                    shape="polygon",
                    fill=fill,
                    points=rings[0],
                    holes=rings[1:],
                )
            )
        return specs

    if geometry_type == "linestring":
        return [
            ObstacleSpec(
                shape="polyline",
                fill=fill,
                points=tuple(_normalize_point(point) for point in coordinates or []),
                thickness=thickness,
            )
        ]

    if geometry_type == "multilinestring":
        return [
            ObstacleSpec(
                shape="polyline",
                fill=fill,
                points=tuple(_normalize_point(point) for point in line),
                thickness=thickness,
            )
            for line in coordinates or []
            if len(line) >= 2
        ]

    if geometry_type == "point":
        return [ObstacleSpec(shape="point", fill=fill, center=_normalize_point(coordinates))]

    if geometry_type == "multipoint":
        return [
            ObstacleSpec(shape="point", fill=fill, center=_normalize_point(point))
            for point in coordinates or []
        ]

    raise ValueError(f"Unsupported GeoJSON geometry type: {geometry.get('type')}")


def _feature_bounds(features: Sequence[Mapping[str, Any]]) -> tuple[float, float, float, float]:
    xs: list[float] = []
    ys: list[float] = []
    for feature in features:
        for x, y in _iter_geometry_points(feature.get("geometry") or {}):
            xs.append(x)
            ys.append(y)
    if not xs or not ys:
        raise ValueError("GeoJSON features do not contain any coordinates")
    return min(xs), min(ys), max(xs), max(ys)


def _iter_geometry_points(geometry: Mapping[str, Any]) -> Iterable[Point]:
    coordinates = geometry.get("coordinates")
    if coordinates is None:
        return

    def recurse(value: Any) -> Iterable[Point]:
        if isinstance(value, Sequence) and len(value) >= 2 and all(isinstance(v, (int, float)) for v in value[:2]):
            yield _normalize_point(value)
            return
        if isinstance(value, Sequence):
            for item in value:
                yield from recurse(item)

    yield from recurse(coordinates)


def _polygon_rings(config: Mapping[str, Any]) -> Sequence[Sequence[Sequence[float]]]:
    if "rings" in config:
        return config["rings"]
    if "points" in config:
        return [config["points"]]
    raise ValueError("Polygon obstacles require points or rings")


def _rectangle_bounds(config: Mapping[str, Any]) -> tuple[float, float, float, float]:
    if "bounds" in config:
        bounds = config["bounds"]
        if len(bounds) != 4:
            raise ValueError("Rectangle bounds must contain four values")
        min_x, min_y, max_x, max_y = (float(value) for value in bounds)
        return min(min_x, max_x), min(min_y, max_y), max(min_x, max_x), max(min_y, max_y)

    if all(key in config for key in ("x", "y", "width", "height")):
        x = float(config["x"])
        y = float(config["y"])
        width = float(config["width"])
        height = float(config["height"])
        return x, y, x + width, y + height

    raise ValueError("Rectangle obstacles require bounds or x/y/width/height")


def _normalize_cell(cell: Sequence[Any]) -> Cell:
    if len(cell) < 2:
        raise ValueError("Cell coordinates must contain at least two values")
    return int(cell[0]), int(cell[1])


def _normalize_point(point: Sequence[Any] | None) -> Point:
    if point is None or len(point) < 2:
        raise ValueError("Point coordinates must contain at least two values")
    return float(point[0]), float(point[1])


def _point_to_cell(
    point: Point,
    *,
    origin: Point,
    cell_size: float,
    width: int,
    height: int,
) -> Cell:
    x = int(math.floor((point[0] - origin[0]) / cell_size))
    y = int(math.floor((point[1] - origin[1]) / cell_size))
    return min(max(x, 0), width - 1), min(max(y, 0), height - 1)


def _candidate_cells(
    width: int,
    height: int,
    min_x: float,
    min_y: float,
    max_x: float,
    max_y: float,
    *,
    origin: Point,
    cell_size: float,
) -> Iterable[tuple[int, int, float, float]]:
    start_x = max(0, int(math.floor((min_x - origin[0]) / cell_size)) - 1)
    end_x = min(width - 1, int(math.ceil((max_x - origin[0]) / cell_size)) + 1)
    start_y = max(0, int(math.floor((min_y - origin[1]) / cell_size)) - 1)
    end_y = min(height - 1, int(math.ceil((max_y - origin[1]) / cell_size)) + 1)

    for y in range(start_y, end_y + 1):
        center_y = origin[1] + ((y + 0.5) * cell_size)
        for x in range(start_x, end_x + 1):
            center_x = origin[0] + ((x + 0.5) * cell_size)
            yield x, y, center_x, center_y


def _point_in_ring(point: Point, ring: Sequence[Point]) -> bool:
    inside = False
    px, py = point
    n = len(ring)
    for index in range(n):
        ax, ay = ring[index]
        bx, by = ring[(index + 1) % n]
        if _point_on_segment(point, (ax, ay), (bx, by)):
            return True
        denominator = by - ay
        if abs(denominator) < 1e-12:
            denominator = 1e-12
        intersects = ((ay > py) != (by > py)) and (
            px < ((bx - ax) * (py - ay) / denominator) + ax
        )
        if intersects:
            inside = not inside
    return inside


def _point_on_segment(point: Point, start: Point, end: Point, *, eps: float = 1e-9) -> bool:
    px, py = point
    ax, ay = start
    bx, by = end
    cross = ((px - ax) * (by - ay)) - ((py - ay) * (bx - ax))
    if abs(cross) > eps:
        return False
    dot = ((px - ax) * (bx - ax)) + ((py - ay) * (by - ay))
    if dot < -eps:
        return False
    length_sq = ((bx - ax) ** 2) + ((by - ay) ** 2)
    return dot <= length_sq + eps


def _point_segment_distance(point: Point, start: Point, end: Point) -> float:
    px, py = point
    ax, ay = start
    bx, by = end
    dx = bx - ax
    dy = by - ay
    if abs(dx) < 1e-12 and abs(dy) < 1e-12:
        return math.dist(point, start)
    t = ((px - ax) * dx + (py - ay) * dy) / ((dx * dx) + (dy * dy))
    t = min(max(t, 0.0), 1.0)
    projection = (ax + (t * dx), ay + (t * dy))
    return math.dist(point, projection)
