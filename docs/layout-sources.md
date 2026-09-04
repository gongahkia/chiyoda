# Open-layout source observations

Chiyoda can inspect a content-locked OpenStreetMap (OSM) XML extract as a
starting inventory for scenario authoring. It does not import a station, create
Chiyoda source, project latitude/longitude into metres, or infer floors,
connectivity, capacities, obstacles, demand, destinations, or accessibility.
Those would be unsafe guesses from an incomplete public map.

The command is useful when the novelty of a structural experiment depends on a
real public layout source but the simulator is intentionally uncalibrated. It
keeps the map snapshot, attribution, tag observations, and authoring gap
visible instead of turning them into undocumented model facts.

## Source contract

Acquire an OSM XML (`.osm` or `.xml`) station-area extract and declare it in an
`uncalibrated_reference` evidence catalog. The catalog must have exactly one
source file, `license: "ODbL-1.0"`, and non-empty attribution containing
`OpenStreetMap contributors`. Record the exact query or extract URL, the local
byte count, and SHA-256. For example, the important fields are:

```json
{
  "schema_version": "0.1",
  "purpose": "uncalibrated_reference",
  "dataset_id": "reviewed-station-layout",
  "title": "Reviewed OSM station-area extract",
  "landing_page": "https://www.openstreetmap.org/",
  "license": "ODbL-1.0",
  "redistributable": true,
  "attribution": "© OpenStreetMap contributors",
  "citation": "OpenStreetMap contributors",
  "files": [{
    "id": "station-area",
    "source_url": "https://EXACT-EXTRACT-OR-QUERY-URL",
    "local_path": "reviewed-station-layout/station.osm",
    "sha256": "EXACT-64-CHARACTER-SHA256",
    "size_bytes": 1,
    "transformation": "Preserve the OSM XML unchanged; inspect recognized tags as geographic source observations only."
  }],
  "supported_primitives": "Mapped station, entrance, pedestrian-way, step, elevator, platform, building, and indoor tags only.",
  "exclusions": "No map-completeness, geometry, elevation, capacity, connectivity, demand, behavior, validation, operational, or safety claim."
}
```

Replace every all-caps placeholder with the acquired file's actual metadata
before validation; the example itself is intentionally not a usable catalog.
The OSM data license and attribution requirements are authoritative at
[OpenStreetMap's legal FAQ](https://wiki.openstreetmap.org/wiki/Legal_FAQ).
An extract URL can be mutable, so the report's source SHA-256 and byte count,
not a later re-download, identify the inspected snapshot.

## Inspect an extract

```console
$ PYTHONPATH=python/src python3 -m chiyoda_analysis.evidence_cli fetch \
    my-layout-catalog.json --data-root data/raw
$ cargo run -p chiyoda -- evidence lock my-layout-catalog.json
$ cargo run -p chiyoda -- layout osm my-layout-catalog.json \
    -o out/station-layout-observations.json
$ cargo run -p chiyoda -- layout verify-osm my-layout-catalog.json \
    out/station-layout-observations.json
```

`layout osm` verifies the catalog and exact source bytes before XML parsing. It
uses station-sized resource bounds by default: 250,000 nodes, 50,000 ways,
10,000 node references per way, and 128 tags per object. A larger reviewed
extract can raise only the first two with `--max-nodes` and `--max-ways`; a
regional extract should normally be reduced to the intended station area
instead.

`layout verify-osm` reads the report's persisted bounds, rebuilds it from the
same content-locked extract, and requires an exact match. It catches a modified
report or a changed source; it still does not establish that the extract is a
complete or correct facility survey.

The report content-locks the catalog and source, carries the required OSM
attribution, and lists recognized `node` and `way` observations. Categories are
limited to `station`, `platform`, `entrance`, `pedestrian_way`, `steps`,
`elevator`, `building`, and `indoor_area`. Geographic coordinates are retained
as WGS84 latitude/longitude only; ways are represented by their geographic
bounds, not an asserted walkable polygon.

Tag meaning itself remains conditional. OSM's indoor guidance and Simple Indoor
Tagging define optional conventions for `level`, rooms, corridors, steps, and
elevators; they do not establish that any particular building is completely or
correctly mapped. See [Indoor mapping](https://wiki.openstreetmap.org/wiki/Indoors)
and [Simple Indoor Tagging](https://wiki.openstreetmap.org/wiki/Simple_Indoor_Tagging).

## Human authoring boundary

Use the observation report as an audit trail, then independently verify and
author the scenario in the following order:

1. Establish a local metre coordinate system and record the survey or
   transformation; never use latitude/longitude degrees as DSL lengths.
2. Confirm every walkable boundary, obstacle, entrance, platform, connector,
   elevation, direction, width, and accessibility property needed by the
   scenario.
3. Author explicit capacities, demand, releases, destinations, and behavioral
   assumptions. A map tag is not evidence for any of them.
4. Validate the resulting `.chy` source and use a declared sensitivity study
   for material best guesses.

The output status is always `source_observation_only`. It cannot be passed to a
calibration adapter or empirical benchmark workflow and does not validate the
reference runtime against the mapped facility.
