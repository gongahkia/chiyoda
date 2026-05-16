# Gibson Structure Schema

This document describes the canonical exported artifact, `structure.json`.

Current schema: `gibson.structure.v16`

Generate an export:

```bash
cargo run --release -- --seed ABCD1234 --profile balanced --headless --export structure.json
```

Example exports are checked in under `examples/exports/` for `balanced`, `decayed`, and `vertical` profiles.

## Top-Level Shape

- `seed`: 8-character alphanumeric seed used for deterministic generation.
- `size`: horizontal grid width/depth.
- `layers`: vertical layer count.
- `metadata`: aggregate counts, active config snapshot, and schema version.
- `grid`: `[x][z][y]` cell IDs.
- `connections`: legacy connection records retained for compatibility.
- `rooms`: typed semantic rooms, each with an optional `cluster_id`.
- `transit_graph`: first-class route nodes, edges, edge roles, path points, and route-room attachments.
- `districts`: district records with bounds, occupancy, grammar, and generated feature summaries.
- `district_lifecycle`: district age, maintenance, occupancy, control, and derived generation biases.
- `strata`: vertical strata records for underground, surface, midrise, and skyline logic.
- `macro_massing`, `meso_placements`, `micro_details`: multi-scale generation summaries for massing voids/spines, route/cluster shaping, and corridor details.
- `district_borders`: generated transition zones between adjacent districts.
- `room_clusters`: grouped rooms such as market strips, habitation blocks, shrine pockets, and data-vault compounds.
- `path_analysis`: traversal quality summary and main service-to-skyline mission path.
- `infrastructure_flows`: route-carried utility systems such as power, data, water, waste, and ventilation.
- `route_simulation`: per-route civilian density, security pressure, blackout risk, market congestion, evacuation viability, and active temporal phases.
- `resource_networks`: power/data/water/air-style constrained networks with load, capacity, outages, overloads, and reroutes.
- `hazard_zones`: exported dynamic/structural risks such as blackouts, sumps, and security sweeps.
- `structural_system`: load-bearing frames, foundations, suspended decks, and stability ratings.
- `failure_zones`: propagated structural failures from collapse scars and unstable spans.
- `rule_packs`: applied built-in procedural grammar weights by profile, district, and stratum.
- `factions`: faction definitions and influence summaries.
- `territories`: district/cluster ownership records.
- `contested_borders`: faction conflicts at district transition zones.
- `temporal_state`: deterministic power-cycle phases.
- `narrative_landmarks`: named places attached to routes, clusters, hazards, and borders.

## Cell IDs

- `0`: `EMPTY`
- `1`: `VERTICAL`
- `2`: `HORIZONTAL`
- `3`: `BRIDGE`
- `4`: `FACADE`
- `5`: `STAIR`
- `6`: `PIPE`
- `7`: `ANTENNA`
- `8`: `CABLE`
- `9`: `VENT`
- `10`: `ELEVATOR`
- `11`: `DEBRIS`

## Transit Graph

Each transit edge has:

- `kind`: geometric route type, for example `service_tunnel`, `artery`, `skybridge`, `ring_route`, or `mission_vertical_transfer`.
- `role`: planning role, for example `primary_artery`, `service_loop`, `restricted_spine`, `evacuation_route`, `market_run`, or `maintenance_backbone`.
- `points`: ordered `[x, y, z]` samples through the structure.
- `stratum`: dominant vertical stratum.

Valid exports should satisfy the topology quality contract:

- `guaranteed_service_to_skyline`: `true`.
- `alternate_path_count`: at least `3` ring/alternate routes.
- `vertical_transfer_count`: at least `3` vertical transfer routes.
- `reachable_landmark_count`: at least `8` named landmarks attached to valid routes.
- `faction_territory_connectivity`: at least `0.5`.
- `main_path_room_reachability`: `1.0`.
- `quality_score`: aggregate route/component/landmark/faction/main-path score in `[0.0, 1.0]`.

## Semantic Layers

District, stratum, route, cluster, faction, temporal, structural, and narrative records are additive views over the same generated grid. Consumers should treat `grid` as geometry and these records as semantic indexes into that geometry.

## Stability

The schema is intentionally explicit and versioned. When adding or removing public fields, bump `STRUCTURE_SCHEMA_VERSION` and update this document plus the example exports.
