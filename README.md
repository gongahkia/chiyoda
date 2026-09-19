# Chiyoda

Chiyoda is a deterministic 3D pedestrian-flow simulator for authored transit
interchange scenarios. It provides a typed textual DSL, static validation, a
Rust reference runtime, reproducible JSON run bundles, a deterministic example
generator, and a native Linux trace viewer.

The simulator is useful for inspecting the consequences of explicitly authored
geometry, demand, routes, capacities, and state changes. A valid scenario or
reproducible trace is not a prediction of a facility or population, and is not
appropriate for life-safety, emergency, or regulatory decisions.

## Quick start

Fedora Linux 43 with the pinned Rust toolchain is the supported environment.

```console
$ cargo run -p chiyoda -- generate --seed 73 -o example.chy
$ cargo run -p chiyoda -- format example.chy -o example.formatted.chy
$ cargo run -p chiyoda -- check example.formatted.chy
$ cargo run -p chiyoda -- compile example.formatted.chy -o out/example.ir.json
$ cargo run -p chiyoda -- run example.formatted.chy -o out/example
$ cargo run -p chiyoda -- replay out/example/run.json
$ cargo run -p chiyoda-replay -- out/example/run.json
```

`run` writes the source and a hash-verifiable `run.json` bundle. `replay`
reconstructs that bundle with the installed runtime before printing a summary.
`chiyoda-replay` opens the native viewer and requires an available Linux display
server. Pass `--surface ID` to choose its initial surface, or use `--watch
SOURCE` for a local edit-and-rerun loop that does not write a bundle.

## Language at a glance

```chy
scenario "concourse-transfer"
seed 73
duration 120s
timestep 100ms

surface platform at (0m, 0m, 6m) size (40m, 16m)
surface concourse at (0m, 0m, 0m) size (40m, 16m)
obstacle retail_kiosk on concourse at (18m, 6m, 0m) size (4m, 4m)
waypoint fare_hall on concourse at (28m, 8m, 0m) dwell 5s
exit street on concourse at (40m, 8m, 0m) width 3m capacity 3/s
stair north_stair from platform at (24m, 8m, 6m) to concourse at (24m, 8m, 0m) width 2m capacity 1.5/s clearance 2.1m
gate fare_gate on concourse at (32m, 8m, 0m) width 2m capacity 18/s to street
agents passengers count 120 on platform at (8m, 8m, 6m) to street speed 1.2m/s radius 0.3m height 1.7m via fare_hall release 0s
```

The compiler accepts only explicit SI units and rejects unknown identifiers,
invalid spatial references, unreachable exits, invalid capacity declarations,
and nondeterministic scenario structure. The [language reference](docs/language.md)
and [executable semantics](docs/semantics.md) define the current contract.

## Replay showcase

![Animated replay of the grand interchange showcase](assets/demo/grand-interchange-showcase.gif)

The [showcase source](examples/demos/grand-interchange-showcase.chy) exercises
multi-surface movement, obstacles, capacities, state changes, alternative exits,
and information events. Its [GIF provenance sidecar](assets/demo/grand-interchange-showcase.gif.json)
records the exact rendering inputs.

Regenerate the animation with:

```console
$ cargo run -p chiyoda -- run examples/demos/grand-interchange-showcase.chy \
    -o out/grand-interchange-showcase --trace-every 20
$ cargo run -p chiyoda-replay -- out/grand-interchange-showcase/run.json \
    --surface concourse --sprite-atlas assets/replay/undercity-atlas.json \
    --export-gif out/grand-interchange-showcase/grand-interchange-showcase.gif \
    --gif-speed 10
```

## Development

```console
$ make verify
```

This formats, lints, tests, smoke-tests the shipped scenario, and builds every
workspace member. The GitHub Actions workflow runs the same target for pull
requests and pushes to `main`.

## Project structure

- `crates/chiyoda-core` — DSL parser, validator, canonical IR, deterministic
  runtime, run bundles, and generator.
- `crates/chiyoda-cli` — scenario authoring, execution, and bundle inspection.
- `crates/chiyoda-replay` — native Linux trace viewer.
- `examples` and `assets` — a replayable showcase and its rendering assets.
- `docs` — language, runtime, and replay contracts.
