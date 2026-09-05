# Native replay viewer

`chiyoda-replay` is a local Linux viewer for a hash-valid `run.json` artifact.
It visualizes the reference runtime's recorded state; it is not a simulation
engine, a facility survey, a 3D renderer, or an operational display.

```console
$ cargo run -p chiyoda-replay -- out/experiment/run.json --paused
$ cargo run -p chiyoda-replay -- out/experiment/run.json --surface concourse
$ cargo run -p chiyoda-replay -- out/experiment/run.json --speed 10
```

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
- Escape closes the viewer.

The window title identifies the selected surface and current trace time. The
camera extent comes from that authored surface, rather than from the agents
visible in one trace frame, so static geometry remains stable while replaying.
By default it advances according to the recorded simulation-time gaps; use
`--speed N` to show `N` simulation seconds per wall-clock second. This avoids
making a trace recorded at a different sampling frequency appear to have a
different duration.

## Rendering contract

For the selected surface the viewer draws its rectangular walkable boundary,
rectangular obstacles, waypoints, exits, gates, and the endpoint of every
authored stair, ramp, escalator, or lift on that surface. Agents are drawn only
when their recorded `surface` matches the selected surface. Moving agents are
blue, departure/route/waypoint waits grey, connector/lift/gate/exit waits and
in-transit agents amber, and evacuated agents green. Static marker colours
distinguish waypoints, exits, gates, and connector class.

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
