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

## Viewer And Authoring

- The viewer replays exported trajectories and renders hazards, bottlenecks,
  messages, runtime floors, source-floor overlays, path-usage heat cells, and
  validation overlays.
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
- Browser authoring edits only the primary floor. Non-primary floors are
  preserved on export but not editable.

## High-Impact Gaps

- Scenario authoring still exports a minimal runnable YAML, not a source-
  preserve scenario patch.
- Static validation catches topology errors but not behavioral plausibility.
- Path-usage debug is aggregate max usage per cell, not per-step route intent.
- No browser-side simulation loop exists; edited YAML must be rerun through CLI.
