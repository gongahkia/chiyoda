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
- run a constrained browser-side simulation replay,
- reset back to the exported Python replay,
- project dispatcher message deltas before committing a marker,
- toggle hazards, bottlenecks, messages, source floors, validation, and path
  usage,
- inspect route strategy/cache metadata in the Routing panel,
- use `floor gap` to flatten or vertically separate source GeoJSON levels,
- toggle connector rendering.
- drag/pan/zoom the camera with OrbitControls; rotation is disabled so the
  viewer behaves like a navigable tilted plan.

Authoring:

- enable `author`,
- choose `paint`, `connector`, or `hostile`,
- choose `dispatch` to click a target cell for the dispatcher panel,
- choose the active `edit floor`,
- choose a paint token, connector type/capacity, or hostile objective/target/credibility,
- click or drag on the grid to paint cells,
- in connector mode, drag from the active floor cell to a target cell; the
  `to floor` selector sets the target floor,
- in hostile mode, click a cell to place a bounded hostile-channel actor,
- use the validation overlay to check exits, disconnected cells, reachable
  spawn/responder starts, and connector-crossing paths,
- export `chiyoda_edited_scenario.yaml`,
- inspect the top-level `origin.path`, `origin.sha256`, and `patch.ops`
  sidecar when source provenance matters,
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

The browser-side sim is a constrained local preview. It supports one runtime
floor, at most 200 replay-seeded agents, no LLM calls, and grid egress toward
exit cells. Reference and benchmark runs still come from Python exports.
Authoring exports a runnable raster `layout.floors` scenario; it does not edit
the original GeoJSON/CAD source or replace trajectory-analysis tools. It now
adds a top-level source-origin block and RFC 6902-style `patch.ops` using RFC
6901 JSON Pointer paths:

- RFC 6902 JSON Patch: <https://www.rfc-editor.org/rfc/rfc6902>
- RFC 6901 JSON Pointer: <https://www.rfc-editor.org/rfc/rfc6901>

Authoring can paint any runtime floor, create connector records, and add
hostile-channel actors to exported YAML.
