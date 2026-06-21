# Standards Research Notes

This repo now treats station geometry as explicit per-floor raster input plus
typed vertical connectors. The implementation is not a standards importer, but
the schema is aligned with the parts of public station/indoor standards that
matter for simulation.

## Relevant Sources

- GTFS Pathways documents station-internal paths, `pathways.txt`, `levels.txt`,
  pathway modes, physical characteristics, and estimated navigation time:
  https://gtfs.org/documentation/schedule/examples/pathways/
- The GTFS schedule reference is the current canonical field reference:
  https://gtfs.org/documentation/schedule/reference/
- OSM Simple Indoor Tagging defines indoor spaces and `level`-scoped mapping:
  https://wiki.openstreetmap.org/wiki/Simple_Indoor_Tagging
- OSM `indoor=*` documents the indoor key used by Simple Indoor Tagging:
  https://wiki.openstreetmap.org/wiki/Key:indoor
- TRB's Fruin pedestrian level-of-service work is the historical basis for
  qualitative walkway/stair crowding categories:
  https://trid.trb.org/View/116491

## Implementation Mapping

- `layout.floors[]` preserves floor identity with `id`, `z`, and raster `text`.
- `layout.connectors[]` models GTFS/indoor-style vertical links as `stairs`,
  `ramp`, `elevator`, or `escalator`.
- Connector endpoints use `{floor, x, y}` mappings rather than ambiguous 2D
  tuples.
- Hazards are positioned in `[x, y, z]` world coordinates and use 3D distance.
- Study and agent telemetry exports floor ID, `z`, and per-floor cell grids.

## Limits

- GeoJSON/CAD ingestion remains a compatibility rasterizer, not a full
  standards-complete indoor importer.
- Viewer authoring edits the primary floor only. Export preserves all runtime
  floors, but non-primary floor editing is not implemented.
- Elevator behavior is capacity/dwell/travel-time holding, not a physical car
  dispatch model.
