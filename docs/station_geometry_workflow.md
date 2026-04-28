# Station Geometry Workflow

Chiyoda treats station geometry as a calibrated scenario input, not as a claim
that the simulator is operationally validated for a real station. The goal of
this workflow is to convert auditable indoor or pathway data into the existing
`layout.geojson` or `layout.cad` ingestion paths, then record what was kept,
discarded, simplified, or hand-audited.

## Supported Inputs

Chiyoda can load:

- Text grids through `layout.file`, `layout.text`, or `layout.grid`.
- GeoJSON through `layout.geojson`.
- A small DXF subset through `layout.cad`.

GeoJSON features can use explicit Chiyoda roles:

| Role | Token | Meaning |
| --- | --- | --- |
| `walkable`, `floor`, `corridor`, `space`, `room` | `.` | Walkable interior cell |
| `obstacle`, `wall`, `blocked`, `blocker` | `X` | Blocked cell |
| `exit`, `egress`, `entrance` | `E` | Exit cell |
| `spawn`, `person`, `agent`, `start` | `@` | Initial population seed |

Explicit `role` or `chiyoda_role` properties are preferred for paper-facing
work. When those are absent, GeoJSON ingestion also recognizes common
OpenStreetMap/OpenStationMap and GTFS Pathways fields:

- OSM Simple Indoor Tagging: `indoor=corridor`, `indoor=area`,
  `indoor=room`, and `indoor=level` become walkable; `indoor=wall` and
  `indoor=column` become blocked.
- OSM station access: truthy `entrance=*`, `railway=subway_entrance`, and
  `railway=train_station_entrance` become exits.
- OSM pedestrian features: `highway=footway`, `highway=pedestrian`,
  `highway=path`, `highway=steps`, `highway=elevator`,
  `highway=corridor`, and matching `area:highway=*` values become walkable.
- OSM platforms: `railway=platform`, `public_transport=platform`, and
  `public_transport=stop_position` become walkable.
- GTFS Pathways: `location_type=2` becomes an exit, `location_type=0`, `3`,
  or `4` becomes walkable, and `pathway_mode=1..7` becomes walkable.

The source conventions behind those mappings are documented in OSM Simple
Indoor Tagging, OpenStationMap, and GTFS Pathways:

- https://wiki.openstreetmap.org/wiki/Simple_Indoor_Tagging
- https://wiki.openstreetmap.org/wiki/OpenStationMap
- https://gtfs.org/documentation/schedule/reference/
- https://gtfs.org/documentation/schedule/examples/pathways/

## Calibration Path

1. Pick one physical level. Chiyoda currently rasterizes one 2D grid per
   scenario. For multi-level stations, make a separate single-level scenario
   or flatten levels only when that simplification is part of the experiment.

2. Export or convert geometry to GeoJSON or DXF. For OSM/OpenStationMap data,
   preserve `indoor`, `level`, `entrance`, `railway`, `public_transport`,
   `door`, and `highway` tags. For GTFS, convert `stops.txt`, `pathways.txt`,
   and `levels.txt` into point and line GeoJSON that preserves `location_type`,
   `pathway_mode`, `level_id`, `length`, `min_width`, and relevant signage
   fields.

3. Normalize roles. Add explicit `role` or `chiyoda_role` properties when the
   source semantics are ambiguous. In particular, decide whether a room is
   public walkable space, staff-only blocked space, or irrelevant.

4. Choose `cell_size`. Start near 1 meter for station-scale studies. Decrease
   only when the scenario question depends on narrow doors, platform edges, or
   queue geometry. Record the choice because it changes bottleneck detection.

5. Audit exits and spawns. Every station scenario should have at least one
   exit and a documented population origin. If imported geometry lacks
   population origins, use explicit cohort `spawn_cells` or add synthetic
   `role=spawn` points.

6. Run a geometry smoke check:

   ```sh
   PYTHONPATH=. .venv/bin/python -m chiyoda.cli run scenarios/edge_bottleneck_station.yaml -o out/edge_bottleneck_station_smoke
   ```

7. Inspect the serialized layout in the exported `metadata.json` and confirm
   the grid has plausible exits, walkable areas, walls, and bottlenecks before
   using it in a study.

8. Record provenance. For paper-facing scenarios, record the source URL or
   file, license, access date, station, level, coordinate transform,
   `cell_size`, known missing data, and every manual edit.

## Manual Fallbacks

Indoor station data is often incomplete. When a source does not contain enough
topology for an auditable scenario, prefer a clear manual fallback over a
hidden repair:

- Use a text layout when the experiment only needs abstract geometry.
- Use `layout.obstacles` to add manually audited walls, blockages, exits, or
  temporary closures after loading a base grid.
- Use explicit cohort `spawn_cells` when source data does not contain platform
  or queue origins.
- Keep missing elevators, stairs, fare gates, or platform subdivisions out of
  the scenario unless they are visible in the source or documented as a manual
  assumption.

The synthetic scenario `scenarios/edge_bottleneck_station.yaml` demonstrates
this fallback pattern. Its GeoJSON fixture uses OSM-style indoor tags,
GTFS-style pathway properties, explicit spawn points, an imbalanced exit pair,
a central bottleneck, and a disconnected-looking service island. It is meant
for ingestion and regression testing, not for real-world validation.

## Review Checklist

Before promoting imported geometry into a paper artifact, confirm:

- Source license permits use in the intended artifact.
- Imported features are limited to the studied level or flattening is stated.
- Exit cells match public egress points in the source.
- Walkable cells do not silently cross walls, fare barriers, shafts, or tracks.
- Bottlenecks visible in the source remain visible after rasterization.
- Disconnected-looking interior features are either true obstacles or explicitly
  removed.
- Population origins and cohort mixes are documented separately from geometry.
- The paper still describes Chiyoda as a stylized simulator unless external
  trajectory, drill, incident, or expert-coded validation has been added.
