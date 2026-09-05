# Open-layout source observations

Chiyoda can inspect a content-locked OpenStreetMap (OSM) XML extract as a
starting inventory for scenario authoring. It does not import a station, create
Chiyoda source, or infer floors, connectivity, capacities, obstacles, demand,
destinations, or accessibility. It can additionally make an explicit,
reproducible local east/north projection for authoring reference; that still
does not establish facility geometry or convert a map into a scenario.

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
10,000 node references per way, and 128 tags per object. Schema `0.2` also
caps the total selected way-node output at `--max-nodes`, so a reviewed source
cannot turn a small report into an unbounded duplicated geometry payload. A
larger reviewed extract can raise only the first two with `--max-nodes` and
`--max-ways`; a regional extract should normally be reduced to the intended
station area instead.

`layout verify-osm` reads the report's persisted inspection limits, rebuilds it from the
same content-locked extract, and requires an exact match. It catches a modified
report or a changed source; it still does not establish that the extract is a
complete or correct facility survey.

The report content-locks the catalog and source, carries the required OSM
attribution, and lists recognized `node` and `way` observations. Categories are
limited to `station`, `platform`, `entrance`, `pedestrian_way`, `steps`,
`elevator`, `building`, and `indoor_area`. Geographic coordinates are retained
as WGS84 latitude/longitude only. Current schema `0.2` reports preserve each
selected way's OSM node identifiers and ordered geographic coordinates. This is
source geometry, not an asserted walkable polygon, corridor, obstacle, or
scenario path. Schema `0.1` reports retained only geographic bounds; they
remain verifiable for audit and are never silently upgraded.

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

## Derive a local coordinate reference

After reviewing the OSM observation report and deliberately choosing an anchor
in the same WGS84 coordinate reference, derive a separate local-reference
artifact:

```console
$ cargo run -p chiyoda -- layout project-osm my-layout-catalog.json \
    out/station-layout-observations.json \
    --origin-latitude 1.300000 --origin-longitude 103.800000 \
    -o out/station-layout-local-reference.json
$ cargo run -p chiyoda -- layout verify-projection my-layout-catalog.json \
    out/station-layout-observations.json \
    out/station-layout-local-reference.json
```

`layout project-osm` first rebuilds and verifies the source-observation report
from the content-locked XML. It then converts the report's WGS84 geographic
coordinates at ellipsoidal height `0 m` to Earth-centred, Earth-fixed (ECEF)
coordinates and into an East/North/Up tangent plane at the supplied origin.
It persists only the East and North values, rounded to one micrometre for
reproducible artifact verification. The transformation is the geographic to
topocentric sequence identified by PROJ as EPSG:9837; its WGS84 ellipsoid
constants are the NGA-published semi-major axis `6378137.0 m` and inverse
flattening `298.257223563`. See PROJ's [geocentric-to-topocentric
conversion](https://proj.org/en/stable/operations/conversions/topocentric.html)
and the [NGA WGS84 definition](https://earth-info.nga.mil/GandG/wgs84/gravitymod/egm2008/index.html).

The origin is an authored transformation parameter, not automatically selected
from the source. Its `0 m` ellipsoidal height is a calculation convention, not
a surveyed ground or floor height. The output intentionally has no vertical
coordinate, and its metre precision is not an accuracy claim.

Point observations become local points. For current schema `0.2` reports, each
selected OSM way node becomes a local source-node sequence in the same order.
It does **not** assert that sequence is a usable line, polygon, corridor,
walkable area, obstacle, or projected extent. Legacy schema `0.1` reports keep
their four projected geographic-bound corners instead; an
antimeridian-ambiguous legacy bound is rejected rather than guessed. `layout
verify-projection` rebuilds the locked observation report and then reconstructs
the projection using the persisted origin, so it detects a changed source
report, map extract, origin, or derived coordinate.

This provides a reproducible reference frame for independent authoring. Before
using any value in `.chy`, still survey or verify the facility and author its
local geometry, elevations, connectivity, widths, accessibility, capacities,
demand, destinations, and behavioral assumptions explicitly.
