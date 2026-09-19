[![](https://img.shields.io/badge/chiyoda_1.0.0-passing-light_green)](https://github.com/gongahkia/chiyoda/releases/tag/1.0.0) 

# `Chiyoda` 🚇

[Deterministic](#features) 2D/3D [pedestrian-flow](https://www.researchgate.net/figure/Examples-of-pedestrian-flow-data_tbl1_366989142) simulator built atop a typed [DSL](https://en.wikipedia.org/wiki/Domain-specific_language) for [scenario source specification](#the-chiyoda-dsl).

## Features

* **Text-first**: Author geometry, demand, routes, capacities, and state changes in a small DSL with explicit SI units
* **Deterministic**: Run the same valid scenario with the same runtime and get the same result
* **Inspectable**: Format, validate, compile, run, and replay from the command line
* **Reproducible**: Runs include their source and a hash-verifiable `run.json` bundle

## Usage

> [!NOTE]  
> `Chiyoda` uses [Rust 1.98.0](https://releases.rs/docs/1.98.0/) through `rust-toolchain.toml`.

The below instructions are for running `Chiyoda` locally.

1. First clone `Chiyoda` to your machine.

```console
$ git clone https://github.com/gongahkia/chiyoda.git && cd chiyoda
```

2. Next execute the below to build `Chiyoda`'s scenario CLI and its native replay viewer *(currently only supported on Linux Display Servers)*.

```console
$ cargo build --release --workspace --locked
$ export PATH="$PWD/target/release:$PATH"
```

3. Finally, run any of below commands to use `Chiyoda`'s functionality.

```console
$ chiyoda generate --seed 73 -o example.chy
$ chiyoda format example.chy -o example.formatted.chy
$ chiyoda check example.formatted.chy
$ chiyoda compile example.formatted.chy -o out/example.ir.json
$ chiyoda run example.formatted.chy -o out/example
$ chiyoda replay out/example/run.json
```

## The `Chiyoda` DSL

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

The compiler rejects unknown identifiers, invalid spatial references,
unreachable exits, invalid capacities, and nondeterministic structure. See the
[language reference](docs/language.md) for the complete grammar and static
checks.

## Replay

![Animated replay of the grand interchange showcase](assets/demo/grand-interchange-showcase.gif)

The [showcase source](examples/demos/grand-interchange-showcase.chy) includes
multi-surface movement, obstacles, capacities, state changes, and alternative
exits. Run it locally, then open its replay:

```console
$ chiyoda run examples/demos/grand-interchange-showcase.chy \
    -o out/grand-interchange-showcase --trace-every 20
$ chiyoda-replay out/grand-interchange-showcase/run.json --surface concourse
```

For an edit-and-rerun loop without writing a bundle, use
`chiyoda-replay --watch scenario.chy`. The [replay guide](docs/replay.md)
covers controls, snapshots, GIF export, and sprite atlases. The showcase's
[GIF provenance sidecar](assets/demo/grand-interchange-showcase.gif.json)
records its rendering inputs.

## Documentation

- [Language reference](docs/language.md) — grammar, validation, canonical IR,
  and geometry boundary.
- [Executable semantics](docs/semantics.md) — runtime state, step order, and
  reproducibility contract.
- [Native replay viewer](docs/replay.md) — live debugging, controls, and export.

## Development

```console
$ make verify
```

This formats, lints, tests, smoke-tests the shipped showcase, and builds the
workspace. GitHub Actions runs the same target for pull requests and pushes to
`main`.

## Scope

Chiyoda is for inspecting consequences of explicitly authored inputs. A valid
scenario or reproducible trace is not a prediction of a real facility or
population, and it is not appropriate for life-safety, emergency, or regulatory
decisions.
