# Native replay viewer

`chiyoda-replay` is a local Linux viewer for either a hash-valid `run.json`
artifact or an in-memory live DSL debug run. It visualizes the reference
runtime's recorded state; it is not a simulation engine, a facility survey, a
3D renderer, or an operational display.

```console
$ cargo run -p chiyoda-replay -- out/experiment/run.json --paused
$ cargo run -p chiyoda-replay -- out/experiment/run.json --surface concourse
$ cargo run -p chiyoda-replay -- out/experiment/run.json --speed 10
$ cargo run -p chiyoda-replay -- out/experiment/run.json --surface concourse --export-gif out/experiment/replay.gif --gif-speed 4
$ cargo run -p chiyoda-replay -- out/experiment/run.json --sprite-atlas assets/replay/undercity-atlas.json
```

## Live DSL debugging

For the editing loop, pass a source file through `--watch` instead of a run
bundle:

```console
$ cargo run -p chiyoda-replay -- --watch examples/experiments/uncalibrated-interchange.chy
$ cargo run -p chiyoda-replay -- --watch draft.chy --paused --surface platform --trace-every 1
```

The viewer polls saved source text, waits 150 ms for writes to settle, then
compiles, validates, and executes the complete deterministic scenario on a
background worker. A newer save supersedes a stale worker result. A successful
revision atomically replaces the displayed trace at simulation time zero; it
does not mutate a running simulation. The default watch trace cadence is one
integration step, which affects display smoothness only.

An invalid edit leaves the most recent valid scene and trace on screen. The
terminal prints the complete compile, validation, or runtime diagnostic; the
window and an in-canvas diagnostic panel identify the failed revision. Before
the first valid source revision, the viewer displays an empty debug canvas with
the current status and any read error.

Watch runs are intentionally in-memory, unpersisted debug results. They are
not hash-verified bundles and cannot be cited as a replayable experiment
artifact. Run `chiyoda run SOURCE -o DIRECTORY` separately when a durable,
hash-verified JSON bundle is needed.

For a bundle made by the installed runtime, the viewer checks the bundle hash
and reconstructs the complete deterministic run before opening a window. It
rejects an incompatible legacy runtime by default; `--allow-legacy-hash-only`
permits display after hash verification alone and emits a warning. The CLI also
rejects an unknown initial surface and an empty trace. It requires an available
Linux display server.

## Controls

- Space toggles playback.
- Left and right arrows step the trace.
- Tab cycles surfaces in their authored declaration order.
- V toggles between the selected-surface 2D view and an all-surface isometric overview.
- P writes the current rendered frame as a PNG under `--snapshot-dir` (default
  `out/chiyoda-replay`). No image is written until P is pressed; existing
  snapshots are never overwritten.
- Escape closes the viewer.

The window title identifies the selected surface and current trace time. The
camera extent comes from that authored surface, rather than from the agents
visible in one trace frame, so static geometry remains stable while replaying.
By default it advances according to the recorded simulation-time gaps; use
`--speed N` to show `N` simulation seconds per wall-clock second. This avoids
making a trace recorded at a different sampling frequency appear to have a
different duration.

## Animated GIF export

PNG snapshots remain the smallest and clearest artifact for a single reviewed
state. Use an animated GIF only when the sequence itself matters—for example,
to review movement timing, queue-state transitions, or rerouting. `--export-gif
PATH` verifies and reconstructs the input bundle before rendering every stored
trace frame of the selected surface, writes `PATH`, then exits without opening a
window. It never accepts `--watch`, does not re-run or compile source, and does
not modify the canonical bundle.

`--gif-speed N` means `N` simulation seconds per playback second. GIF timing is
limited to centiseconds; the companion `PATH.json` records the exact
per-frame rounded delays, the terminal-delay policy, playback speed,
`trace_every_steps`, trace-frame count, rendered surface, scenario hash, bundle
hash, runtime and bundle versions, and a clear derived-artifact boundary. The
exporter refuses to overwrite either the GIF or its sidecar. The verified JSON
bundle remains the canonical replay and timing artifact.

## Optional sprite atlas

The default renderer is intentionally geometric: a direct 1200×800 software
framebuffer with flat rectangles, lines, and small square agent markers. It has
no texture dependency. Supplying `--sprite-atlas PATH.json` is an opt-in visual
treatment only; omitting the flag restores that default renderer without
changing the scenario, trace, bundle verification, or simulation.

The included original limited-palette atlas is
`assets/replay/undercity-atlas.json`. It gives walkable surfaces, walls,
obstacles, connectors, queue positions, markers, and agent states distinct
top-down pixel forms. It uses static sprites: an agent's recorded position and
state change, but the renderer does not invent animation frames or behavior.

An atlas manifest names a relative RGB/RGBA PNG, declares its uniform tile
dimensions, and maps each required visual role to a grid cell. This makes a
sheet replaceable without code changes. The loader rejects an unsupported
schema, path traversal, oversized images, non-divisible dimensions, unsupported
PNG colour forms, and cells outside the sheet. PNG snapshots use the selected
atlas. GIF exports record SHA-256 hashes of both the manifest and image in their
sidecar, because those files affect the derived pixels but never the canonical
JSON bundle.

## Rendering contract

For the selected surface the viewer draws its rectangular walkable boundary,
rectangular obstacles, line-footprint or serpentine-grid queue paths and slot centres, waypoints, exits,
gates, portal-lane centres, and the endpoint of every authored stair, ramp,
escalator, or lift on that surface. Agents are drawn only
when their recorded `surface` matches the selected surface. Moving agents are
blue, departure/route/waypoint waits grey, connector/lift/gate/exit waits and
in-transit agents amber, and evacuated agents green. Static marker colours
distinguish waypoints, exits, gates, connector class, portal-lane centres, and
queue geometry. The queue path and slots are authored placement
geometry, not a measured queue, a claimed standing obstruction, or a density
visualization.

With an optional atlas those same authored and recorded elements are drawn with
the manifest's tiles. The atlas does not add geometry, hide invalid source,
interpret identities, or turn the viewer into a 3D or empirical visualization.

Connector endpoints are visual cues, not surveyed shapes or a claim that the
space between floors is visible from above. During a connector traversal the
runtime records the agent on its originating surface until arrival; a
floor-specific view therefore shows the recorded horizontal interpolation on
that origin surface. This is trace provenance, not a physical 3D position.

The viewer renders only values explicitly authored in the scenario and recorded
in the bundle. It does not infer floor plans, building boundaries, indoor
connectivity, elevations, widths, density, hazards, visibility, accessibility,
capacity, behavior, or any empirical outcome from an OSM observation or a run.
Use the [layout-source workflow](layout-sources.md) for the separate,
source-observation-only authoring reference boundary.

The isometric overview is a fixed projection of authored surface elevations,
static geometry, connectors, and recorded agent positions. It is a debugging
aid, not a general 3D camera, a physical view of a station, or a new rendering
engine. The selected-surface 2D view remains the detailed rendering of queues,
portal lanes, obstacles, and resource markers.
