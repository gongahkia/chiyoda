# Implementation Audit

This is a code-level audit of current Chiyoda behavior. It is not a claim that
the simulator is externally validated for station evacuation prediction.

## Current Runtime Shape

- Scenarios are loaded by `ScenarioManager` from YAML into one `Layout` grid.
- Layout cells use one-character raster tokens: wall `X`, walkable `.`, exit
  `E`, population spawn `@`, signage `S`, responder entry `R`.
- GeoJSON and CAD inputs are rasterized into that same single grid before
  simulation.
- Pathfinding uses a 4-neighbor NetworkX grid graph over non-wall cells.
- Agents route toward known exits with density and hazard penalties.
- Hazards are stylized fields or imported scalar fields; this is not CFD.
- Study exports persist telemetry tables, metadata, figures, and a static
  Three.js viewer.

## Viewer And Authoring

- The viewer replays exported trajectories and renders hazards, bottlenecks,
  messages, source-floor overlays, path-usage heat cells, and validation
  overlays.
- Author mode edits the raster `layout.text` grid, not the original GeoJSON or
  CAD source.
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
- explicit starts can reach an exit through 4-neighbor walkable cells,
- disconnected walkable cells are reported.

It does not prove calibration quality, evacuation realism, hazard realism, or
that the source station data is complete.

## Multi-Floor Truth Gap

Source GeoJSON levels can be rendered separately in the viewer. Simulation does
not yet preserve floor identity, vertical connectors, transfer penalties, or
per-floor route choice. After rasterization, all walkable cells are in one flat
grid unless the source data was manually encoded into the 2D raster topology.

Required work before calling this multi-floor simulation:

- carry level IDs through layout rasterization,
- represent stairs/escalators/elevators as typed inter-floor connectors,
- make pathfinding include floor and connector costs,
- export per-floor trajectories and validation paths,
- make viewer authoring preserve level-specific geometry instead of only
  `layout.text`.

## High-Impact Gaps

- Scenario authoring still exports a minimal runnable YAML, not a source-preserve
  scenario patch.
- Static validation catches topology errors but not behavioral plausibility.
- Path-usage debug is aggregate max usage per cell, not per-step route intent.
- Figure export can still emit NumPy histogram warnings on sparse runs.
- No browser-side simulation loop exists; edited YAML must be rerun through CLI.
