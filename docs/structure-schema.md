# Gibson Structure Schema

This document describes the canonical exported artifact, `structure.json`.

Current schema: `gibson.structure.v22`

Generate an export:

```bash
cargo run --release -- --seed ABCD1234 --profile balanced --typology linear-city --headless --export structure.json
```

Example exports are checked in under `examples/exports/` for `balanced`, `decayed`, and `vertical` dense-enclave outputs plus one export for every supported typology.

## Top-Level Shape

- `seed`: 8-character alphanumeric seed used for deterministic generation.
- `size`: horizontal grid width/depth.
- `layers`: vertical layer count.
- `metadata`: aggregate counts, active config snapshot, and schema version.
- `typology_frame`: macro-form contract for the selected megastructure family.
- `typology_quality`: per-typology contract metrics for route continuity, anchors, bands, and missing requirements.
- `construction_history`: deterministic eras explaining foundation, expansion, retrofit, and collapse/informal occupation layers.
- `section_quality`: cross-section metrics for vertical continuity, service separation, evacuation coverage, habitable layers, and deck access.
- `grid`: `[x][z][y]` cell IDs.
- `connections`: legacy connection records retained for compatibility.
- `rooms`: typed semantic rooms, each with an optional `cluster_id`.
- `transit_graph`: first-class route nodes, edges, edge roles, path points, and route-room attachments.
- `districts`: district records with bounds, occupancy, grammar, and generated feature summaries.
- `district_lifecycle`: district age, maintenance, occupancy, control, and derived generation biases.
- `strata`: vertical strata records for underground, surface, midrise, and skyline logic.
- `macro_massing`, `meso_placements`, `micro_details`: multi-scale generation summaries for massing voids/spines, typology frames, route/cluster shaping, and corridor details.
- `district_borders`: generated transition zones between adjacent districts.
- `room_clusters`: grouped rooms such as market strips, habitation blocks, shrine pockets, and data-vault compounds.
- `path_analysis`: traversal quality summary and main service-to-skyline mission path.
- `infrastructure_flows`: route-carried utility systems such as power, data, water, waste, and ventilation.
- `route_simulation`: per-route civilian density, security pressure, blackout risk, market congestion, evacuation viability, and active temporal phases.
- `resource_networks`: power/data/water/air-style constrained networks with load, capacity, outages, overloads, and reroutes.
- `hazard_zones`: exported dynamic/structural risks such as blackouts, sumps, and security sweeps.
- `structural_system`: load-bearing frames, foundations, suspended decks, and stability ratings.
- `failure_zones`: propagated structural failures from collapse scars and unstable spans.
- `rule_packs`: applied procedural grammar and entity weights by profile, optional typology, district, and stratum.
- `rule_influences`: target-level traces explaining which rule pack influenced each exported district, route, cluster, hazard, and landmark.

Editable rule-pack JSON files live under `rules/`. They can define profile, district, stratum, grammar weights, and optional entity weights for future authoring workflows; when no external rule file is supplied, Gibson falls back to the built-in Rust rule packs.
- `factions`: faction definitions and influence summaries.
- `territories`: district/cluster ownership records.
- `contested_borders`: faction conflicts at district transition zones.
- `temporal_state`: deterministic power-cycle phases.
- `narrative_landmarks`: named places attached to routes, clusters, hazards, and borders.
- `entities`: deterministic mobile entity groups such as market crowds, patrols, evacuees, maintenance crawlers, builder swarms, and scavenger drifts.
- `entity_paths`: sampled movement traces through the transit graph.
- `entity_pressure_fields`: aggregate dynamic influence volumes produced by entity movement.
- `layout_mutations`: bounded generation-time geometry edits caused by pressure fields.

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

## Typologies

`metadata.typology` and `typology_frame` describe the high-level megastructure family used by the generator. `dense_enclave` preserves the historical Kowloon-like default. Other supported typologies are `arcology_spire`, `linear_city`, `bridge_void`, `marine_platform`, `orbital_ring`, `underground_hive`, `mountain_burrow`, `desert_arcology`, `airport_city`, `dam_city`, `shipyard_stack`, `volcanic_caldera`, `ice_shelf_city`, `canopy_babel`, `space_elevator_anchor`, `crawler_city`, `reef_atoll_arcology`, `stratosphere_platform`, and `sinkhole_citadel`.

The typology frame exports primary axes, spine anchors, void bands, habitat bands, service anchors, and traversal contracts. Generation phases use this frame to bias district placement, occupancy, macro massing, and route roles such as `linear_express`, `station_loop`, `void_bridge`, `marine_causeway`, `pylon_service`, `rim_loop`, `spoke_transfer`, `hive_trunk`, `cliff_gallery`, `climate_spine`, `runway_spine`, `dam_wall_spine`, `drydock_spine`, `caldera_ring`, `meltwater_spine`, `canopy_walk`, `tether_core`, `crawler_track`, `reef_ring`, `pressure_deck`, and `sinkhole_ring`.

`typology_quality` validates that the selected family meets its native contracts, and `section_quality` validates vertical/cross-section experience. Structural exports now include stress fields and load paths so hazards can be traced back to weak support chains.

## Semantic Layers

District, stratum, route, cluster, faction, temporal, structural, narrative, and entity records are additive views over the same generated grid. Consumers should treat `grid` as geometry and these records as semantic indexes into that geometry.

## Dynamic Entities

Entity dynamics are deterministic export data, not a separate runtime simulation state.

- `entities` define the group kind, origin, destination, linked faction/cluster, active temporal phases, route IDs, and layout influence.
- `entity_paths` contain sampled `[x, y, z]` movement traces over valid route IDs, with congestion and risk scores in `[0.0, 1.0]`. Routes are chosen with transit-graph pathfinding, so hazards, congestion, resource outages, faction control, and phase state can redirect movement.
- `entity_pressure_fields` aggregate paths into spatial pressures such as `market_surge`, `patrol_lockdown`, `evacuation_flow`, `maintenance_crawler`, `builder_swarm`, and `scavenger_drift`.
- `layout_mutations` record conservative grid edits made during generation, including affected routes/rooms, added/removed cell counts, sample points, and the source pressure field.
- Optional rule-pack `typology` targets a pack to a specific megastructure family; omitted `typology` applies to all families. Optional fields `entity_density_weight`, `entity_layout_weight`, `patrol_weight`, `crowd_weight`, and `builder_weight` tune spawn pressure, layout edits, and kind-specific movement pressure. Missing fields default to neutral `1.0`.
- The Macroquad renderer can display animated entity movement with the `V` overlay. Use `U` to pause, `J` to change speed, `N` to scrub temporal phases, and `M`/`K` to select and toggle entity kinds. The animation reads exported paths and does not mutate the grid at runtime.

## Compatibility

`gibson.structure.v17`, `gibson.structure.v18`, `gibson.structure.v19`, `gibson.structure.v20`, and `gibson.structure.v21` JSON can be loaded by current tools. The loader fills missing typology data with `dense_enclave`, missing typology/section quality metrics, construction history, dynamic entity sections, dynamic count metadata, generation config controls, and neutral entity rule weights before deserializing as `gibson.structure.v22`.

## Stability

The schema is intentionally explicit and versioned. When adding or removing public fields, bump `STRUCTURE_SCHEMA_VERSION` and update this document plus the example exports.
