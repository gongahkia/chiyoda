# Implementation Audit

This is a code-level audit of current Chiyoda behavior. It is not a claim that
the simulator is externally validated for station evacuation prediction.

## Current Runtime Shape

- Scenarios are loaded by `ScenarioManager` from YAML into strict
  `layout.floors` plus optional `layout.connectors`.
- Layout cells use one-character raster tokens on each floor: wall `X`,
  walkable `.`, exit `E`, population spawn `@`, signage `S`, responder entry
  `R`.
- GeoJSON and CAD inputs remain compatibility helpers for raster generation;
  runnable scenarios should use explicit `layout.floors`.
- Pathfinding uses a 4-neighbor NetworkX graph over non-wall cells plus typed
  inter-floor connector edges.
- Agents route toward known exits with density and hazard penalties.
- Hazards are stylized fields or imported scalar fields with 3D point distance;
  this is not CFD.
- Study exports persist telemetry tables, metadata, figures, and a static
  Three.js viewer.
- Runtime assertions can check evacuation counts, travel-time bounds,
  connector usage, and impossible floor jumps.

## Viewer And Authoring

- The viewer replays exported trajectories and renders hazards, bottlenecks,
  messages, runtime floors, source-floor overlays, path-usage heat cells, and
  validation overlays. Its browser-side preview supports multi-floor connector
  traversal but is not the reference simulation engine.
- Author mode edits the primary raster floor and exports strict
  `layout.floors`; it does not edit the original GeoJSON/CAD source.
- Exported viewer YAML should be checked with:

```sh
.venv/bin/python -m chiyoda.cli validate-scenario ~/Downloads/chiyoda_edited_scenario.yaml
```

## Validation Coverage

`validate-scenario` checks:

- at least one walkable cell,
- at least one exit,
- explicit population and responder starts are in bounds,
- explicit starts are not on walls,
- explicit starts can reach an exit through 4-neighbor walkable cells and
  configured connectors,
- disconnected walkable cells are reported.

It does not prove calibration quality, evacuation realism, hazard realism, or
that the source station data is complete.

## Multi-Floor Runtime

The core runtime now preserves floor identity, vertical connector edges,
connector costs, per-floor path usage, per-floor cell telemetry, and 3D hazard
distance. Scenario validation traverses connector edges, so a spawn on one
floor can validate against an exit on another floor when a connector exists.

Connector support is intentionally simple:

- `stairs`, `ramp`, and `escalator` are weighted graph edges.
- `elevator` adds capacity, dwell time, and travel time holds.
- There is no elevator dispatch, door state, car position, or queue discipline
  beyond active transfer capacity.
- Browser authoring can paint any runtime floor, preserve or add connectors,
  and export hostile-channel actors.

## Extended Hazard Runtime

The `Hazard` dataclass in `chiyoda/environment/hazards.py` covers ten kinds
(`HAZARD_PROFILES` keys: GAS, SMOKE, FIRE, WILDFIRE, EMBER, FLOOD, EARTHQUAKE,
AFTERSHOCK, CRUSH, SHOOTER). Each shares the spread/advect/diffuse loop in
`Hazard.step()` but specializes per-kind:

- **WILDFIRE / EMBER (`hazards.py:135-144`)** advect at 15% of wind-vector
  speed while growing radius linearly with `spread_rate + 0.05 * wind_speed`.
  `_step_ember_field` samples a Poisson-thinned spotting distribution out to
  `ember_ignition_radius` and writes ember cell intensities into
  `self.ember_field`. `_ember_intensity_at` adds the ember contribution to the
  main hazard intensity so ember fall on a cell makes that cell harmful even
  outside the parent radius. Ember intensity decays by `ember_decay_rate` each
  step and zero-clamps below 0.01.
- **FLOOD (`hazards.py:145-161`)** uses `flow_vector` if set, else
  `wind_vector`. The hazard radius grows with `spread_rate + diffusion_rate +
  0.03 * flow_speed`. `inundation_depth_m` rises monotonically toward
  `max_depth_m` at `inundation_rise_rate_mps`, and `_step_inundation_field`
  writes per-cell depth to `self.inundation_field`. Depth-aware intensity is
  fused into `intensity_at` so deeper cells are more hazardous regardless of
  Euclidean distance to the seed.
- **EARTHQUAKE / AFTERSHOCK (`hazards.py:162-164`, `397-446`)** decay
  `shock_intensity` by `aftershock_decay_rate * dt` and consume the integer
  step schedule in `aftershock_schedule`. Each pulse calls
  `simulation.apply_terrain_damage(center, radius, damage)` to permanently
  damage walkable cells, then `simulation.trigger_re_evacuation_wave(center,
  re_evacuation_radius)` to nudge agents inside the wave radius to refresh
  their evacuation intent. Each pulse appends an event row to
  `simulation.aftershock_events` for telemetry.
- **SHOOTER (`hazards.py:70-75`, `177`)** uses a fixed effective radius
  `range_m` (default 8 m) rather than the spreading `radius`. Profile
  `speed_decay` is low but `rationality_decay` is high, so impact is mostly on
  cognition and route choice; agents in cone-of-fire pay accuracy-weighted
  exposure. `intensity_at` substitutes `range_m` for `radius` only when
  `kind.upper() == "SHOOTER"`.

These hazards expose the same `intensity_at(point)`, `to_dict()`, and
`from_dict()` surfaces as the stylized GAS/SMOKE/FIRE/CRUSH set, so study
exports, telemetry, viewer overlays, and assertion checks treat them
uniformly.

## High-Impact Gaps

- Static validation catches topology errors but not behavioral plausibility.
- Per-step route intent is now opt-in with `--per-step-intent`. It writes a
  sparse `intent_path_usage` table grouped by `(run_id, step, floor_id, x, y,
  intent)` and is loaded into the viewer payload as `intent_path_usage`.
  Expected file-size budget is at most one pre-grouping row per active agent per
  recorded step; grouped parquet output should stay under the corresponding
  `agent_steps` table size for the same run.
- The GeoJSON converter is a pragmatic OSM/GTFS-like bridge, not a
  standards-complete indoor data importer.
