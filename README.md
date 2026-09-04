# Chiyoda

Chiyoda is an Apache-2.0 research platform for deterministic, reproducible
3D pedestrian-flow experiments in transit interchanges. Its primary artifact
is a standalone experiment language and executable reference semantics—not a
real-time operations dashboard or a certified evacuation product.

The current `0.1.0-alpha.1` release establishes the language/runtime contract:

- a typed textual DSL with static unit, topology, reachability, capacity, and
  deterministic-replay checks;
- a Rust reference runtime for volume-bearing agents on connected 3D walkable
  surfaces, stairs, lifts, gates, exits, and typed information interventions;
- immutable JSON run bundles with source, canonical IR, events, traces,
  metrics, and SHA-256 integrity hashes;
- a deterministic, constraint-preserving scenario generator; and
- a native Linux trace replay application.

## Research boundary

Chiyoda is **not** suitable for regulatory approval, emergency dispatch,
facility certification, or life-safety decisions. The reference runtime has
not yet been calibrated against public trajectory data. It makes no claim of
predictive fidelity for any population, facility, or evacuation outcome.

An empirical benchmark round must supply redistributable calibration and
held-out datasets, documented transformations, source hashes, released seeds,
and an explicit evidence boundary. The repository now has a content-locked
candidate source and descriptive intake pipeline for 2D platform trajectories;
this is not runtime calibration or a published empirical round. The CLI rejects
a round manifest that does not meet its contract. See [evidence
boundaries](docs/evidence.md) and the [benchmark protocol](docs/benchmark.md).

## Quick start

Fedora Linux 43 with the pinned Rust toolchain is the supported environment.

```console
$ cargo run -p chiyoda -- generate --seed 73 -o example.chy
$ cargo run -p chiyoda -- format example.chy -o example.formatted.chy
$ cargo run -p chiyoda -- check example.chy
$ cargo run -p chiyoda -- compile example.chy -o out/example.ir.json
$ cargo run -p chiyoda -- run example.chy -o out/example
$ cargo run -p chiyoda -- replay out/example/run.json
$ cargo run -p chiyoda-replay -- out/example/run.json
```

The first replay command verifies a bundle hash and prints a summary. The
second opens the native replay viewer; it requires an available Linux display
server.

## Research data intake

Evidence acquisition, content locking, and descriptive source intake are
separate from simulator execution. The first source catalog is a CC BY 4.0
Eindhoven Centraal platform release. Raw data is intentionally never committed.

```console
$ PYTHONPATH=python/src python3 -m chiyoda_analysis.evidence_cli fetch \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json
$ cargo run -p chiyoda -- evidence lock \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json
$ cargo run -p chiyoda -- calibrate eindhoven-platform \
    benchmarks/evidence/eindhoven-centraal-platform-2024.json \
    -o out/eindhoven-platform-intake.json
```

The report is explicitly `descriptive_only`; it cannot justify predictive or
operational use. Details, source limits, and the required next review gate are
in [evidence boundaries](docs/evidence.md) and the [calibration
protocol](docs/calibration-protocol.md).

## Language at a glance

```chy
scenario "concourse-transfer"
seed 73
duration 120s
timestep 100ms

surface platform at (0m, 0m, 6m) size (40m, 16m)
surface concourse at (0m, 0m, 0m) size (40m, 16m)
exit street on concourse at (40m, 8m, 0m) width 3m
stair north_stair from platform at (24m, 8m, 6m) to concourse at (24m, 8m, 0m) width 2m
lift accessible_lift from platform at (5m, 8m, 6m) to concourse at (5m, 8m, 0m) cabin 2m 2m capacity 8 cycle 12s
gate fare_gate on concourse at (32m, 8m, 0m) width 2m capacity 18/s to street
agents passengers count 120 on platform at (8m, 8m, 6m) to street speed 1.2m/s radius 0.3m height 1.7m

message false_closure source peer on platform at (16m, 8m, 6m) claim connector north_stair closed truth false time 20s reach 10m trust 0.7
countermeasure correction corrects false_closure source staff on platform at (16m, 8m, 6m) time 35s reach 14m trust 0.9
```

The compiler accepts only explicit SI units. It rejects unknown identifiers,
invalid spatial references, unreachable exits, zero-capacity facilities,
non-reproducible scenario structure, and countermeasures that purport to
correct a truthful message. [The language reference](docs/language.md) and
[executable semantics](docs/semantics.md) define the normative behavior.

## Development

```console
$ make verify
```

`make verify` formats, lints, tests, and builds every workspace member. The
CI workflow performs the same checks with a read-only token. Branch protection
must be configured on the GitHub repository separately; repository files
cannot enforce it.

## Project structure

- `crates/chiyoda-core` — DSL parser, validator, canonical IR, evidence/source
  locks, bounded Parquet intake, deterministic generator, and reference runtime.
- `crates/chiyoda-cli` — compiler, runtime, generator, evidence, calibration,
  benchmark, and bundle-verification commands.
- `crates/chiyoda-replay` — native Linux trace viewer.
- `python` — dependency-light bundle verification and analysis helpers; it is
  intentionally outside the trusted simulation runtime.
- `benchmarks` — public fixtures and round-protocol materials; no empirical
  result is published until its evidence manifest validates. Its source
  screening decisions are documented in [data scouting](docs/data-scouting.md).
- `docs` — normative language, runtime, evidence, and release contracts.

## License

Apache-2.0. The project name references the Tokyo Metro Chiyoda Line; it does
not recreate, operationalize, or provide guidance about historic attacks.
