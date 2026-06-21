# Static 3D Viewer

Chiyoda exports a lightweight Three.js viewer beside study bundles. It is for
research inspection: scenario geometry, trajectory playback, bottlenecks,
hazards, interventions, and LLM decision events.

Run any scenario or study:

```sh
.venv/bin/python -m chiyoda.cli run scenarios/example.yaml -o out/example
```

Open:

```sh
cd out/example/viewer
python3 -m http.server 8000
```

Then visit `http://localhost:8000`. Opening `index.html` directly may fail in
some browsers because the viewer fetches `viewer_data.json`.

For an existing bundle:

```sh
.venv/bin/python -m chiyoda.cli export-viewer out/example
```

The viewer reads `viewer/viewer_data.json`, generated from:

- `metadata.json`
- `tables/agent_steps.*`
- `tables/hazards.*`
- `tables/bottlenecks.*`
- `tables/interventions.*`
- `tables/llm_decisions.*`

Controls:

- drag to rotate,
- right-drag or shift-drag to pan,
- scroll or pinch to zoom,
- reset camera from the toolbar,
- toggle hazards, bottlenecks, messages, source floors, validation, and path
  usage,
- use `floor gap` to flatten or vertically separate source GeoJSON levels,
- toggle connector rendering.
- drag/pan/zoom the camera with OrbitControls; rotation is disabled so the
  viewer behaves like a navigable tilted plan.

Authoring:

- enable `author`,
- choose the active `edit floor`,
- choose a paint token,
- click or drag on the grid to paint cells,
- use the validation overlay to check exits, disconnected cells, reachable
  spawn/responder starts, and connector-crossing paths,
- export `chiyoda_edited_scenario.yaml`,
- validate the exported scenario,
- run the exported scenario with `python -m chiyoda.cli run <file> -o <out>`.

```sh
.venv/bin/python -m chiyoda.cli validate-scenario ~/Downloads/chiyoda_edited_scenario.yaml
.venv/bin/python -m chiyoda.cli run ~/Downloads/chiyoda_edited_scenario.yaml -o out/edited
```

Paint tokens map to the simulator's raster layout tokens:

- `floor` -> `.`
- `wall` -> `X`
- `exit` -> `E`
- `spawn` -> `@`
- `signage` -> `S`
- `responder` -> `R`

When the study metadata points to a GeoJSON scenario, the viewer reads source
feature levels such as OSM `level` or GTFS-like `level_id` and renders them as
separate source-floor overlays. Runtime `layout.floors` are also rendered at
their stored `z` values, and exported telemetry includes per-agent floor IDs,
per-floor path usage, hazards with `z`, and per-floor cell grids.

The current viewer is static and local. Authoring exports a runnable raster
`layout.floors` scenario; it does not edit the original GeoJSON/CAD source, run
simulations in the browser, or replace trajectory-analysis tools. Authoring can
paint any runtime floor and preserves existing connectors in exported YAML.
Creating new connectors in the browser is not implemented.
