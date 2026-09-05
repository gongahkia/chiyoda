# Chiyoda

Chiyoda is an Apache-2.0 research platform for deterministic, reproducible
3D pedestrian-flow experiments in transit interchanges. Its primary artifact
is a standalone experiment language and executable reference semantics—not a
real-time operations dashboard or a certified evacuation product.

The current `0.23.0-alpha.1` release establishes the language/runtime contract:

- a typed textual DSL with static unit, topology, reachability, capacity, and
  deterministic-replay checks;
- a Rust reference runtime for radius-aware, height-annotated agents with
  authored connector-eligibility constraints and alternative final exits on
  connected 3D walkable surfaces with rectangular obstacles, stairs, ramps,
  escalators, lifts, gates, capacity-limited exits, scheduled availability and
  service-capacity states, and typed information interventions;
- immutable JSON run bundles with source, canonical IR, events, traces,
  per-intervention reach/acceptance metrics, and SHA-256 integrity hashes;
- a deterministic, constraint-preserving scenario generator; and
- a native Linux trace replay application.

## Research boundary

Chiyoda is **not** suitable for regulatory approval, emergency dispatch,
facility certification, or life-safety decisions. The reference runtime has
not yet been calibrated against public trajectory data. It makes no claim of
predictive fidelity for any population, facility, or evacuation outcome.

The simulator is not gated on an empirical research programme: uncalibrated
models and structural experiments may be authored and executed now. The
evidence requirements apply only to an empirical benchmark or claims about
predictive fidelity. Such a round must supply redistributable calibration and
held-out datasets, documented transformations, source hashes, released seeds,
and an explicit evidence boundary. The repository now has a content-locked
candidate source and descriptive intake pipeline for 2D platform trajectories;
this is not runtime calibration or a published empirical round. The CLI rejects
only a benchmark round manifest that does not meet its contract. See [evidence
boundaries](docs/evidence.md) and the [benchmark protocol](docs/benchmark.md).

## Quick start

Fedora Linux 43 with the pinned Rust toolchain is the supported environment.

```console
$ cargo run -p chiyoda -- generate --seed 73 -o example.chy
$ cargo run -p chiyoda -- format example.chy -o example.formatted.chy
$ cargo run -p chiyoda -- check example.chy
$ cargo run -p chiyoda -- compile example.chy -o out/example.ir.json
$ cargo run -p chiyoda -- run example.chy -o out/example
$ cargo run -p chiyoda -- sweep --seed 73 --count 20 -o out/generated-sweep
$ cargo run -p chiyoda -- verify-sweep out/generated-sweep
$ cargo run -p chiyoda -- analyze-sweep out/generated-sweep -o out/generated-sweep/analysis.json
$ cargo run -p chiyoda -- replicate example.chy --seed 100 --count 20 -o out/authored-replicates
$ cargo run -p chiyoda -- compare-sweeps out/control out/intervention -o out/comparison.json
$ cargo run -p chiyoda -- sensitivity-plan examples/sensitivity/exit-capacity-and-trust.json -o out/sensitivity-plan.json
$ cargo run -p chiyoda -- sensitivity examples/sensitivity/exit-capacity-and-trust.json -o out/sensitivity
$ cargo run -p chiyoda -- verify-sensitivity out/sensitivity
$ cargo run -p chiyoda -- experiment plan examples/experiments/uncalibrated-interchange.json -o out/experiment-plan.json
$ cargo run -p chiyoda -- experiment run examples/experiments/uncalibrated-interchange.json -o out/experiment
$ cargo run -p chiyoda -- experiment verify out/experiment
$ cargo run -p chiyoda -- layout osm my-layout-catalog.json -o out/layout-observations.json
$ cargo run -p chiyoda -- layout verify-osm my-layout-catalog.json out/layout-observations.json
$ cargo run -p chiyoda -- layout project-osm my-layout-catalog.json out/layout-observations.json --origin-latitude 1.300000 --origin-longitude 103.800000 -o out/layout-local-reference.json
$ cargo run -p chiyoda -- layout verify-projection my-layout-catalog.json out/layout-observations.json out/layout-local-reference.json
$ cargo run -p chiyoda -- replay out/example/run.json
$ cargo run -p chiyoda-replay -- out/example/run.json
```

The first replay command reconstructs a compatible bundle before printing a
summary. The second opens the native replay viewer; it also reconstructs a
compatible bundle and requires an available Linux display server. It renders
the selected authored surface, obstacles, waypoints, exits, gates, and connector
endpoints behind the agents. Pass `--surface <id>` to choose its initial floor
or press Tab to cycle floors. Its visual scope and trace-position boundary are
documented in the [native replay viewer guide](docs/replay.md).
`sweep` is an
uncalibrated structural experiment: it generates and
runs a contiguous seed range, writes one independently hash-verifiable bundle
per seed, and records their summaries in `summary.json`, including final-exit
attribution, modeled queue-exposure counts, and current per-run discrete queue
telemetry attributed to individual constrained resources. Generated cases include a
declared alternative exit and a scheduled primary-exit closure, so the output
also exercises rerouting.
`verify-sweep` cross-checks that summary against every bundle and its canonical
source, and reruns each bundle compatible with the installed runtime to reject
self-hashed fabricated results. `analyze-sweep` performs that same verification before producing exact
cross-run counts, per-exit totals, intervention reach/acceptance totals, modeled
queue-exposure and discrete queue-telemetry totals, and
descriptive full-clearance and last-exit-time ranges. Its evacuation fraction is emitted as an exact numerator/denominator rather than a
misleadingly precise estimate, and its final-state totals explain agents still
in the system at the configured time limit. The output directory must be empty,
and the workflow does not require a benchmark manifest or research data.

`replicate` runs one authored, validated scenario over a contiguous seed range.
It stores the canonical template and its hash, then writes one canonical source
and independently hash-verifiable bundle per seed. `verify-sweep` proves every
replication differs from that template only in its seed, making interventions
and information-trust variation inspectable without pretending the seed range
is an empirical sample.

`compare-sweeps` verifies both authored replication directories before producing
a seed-aligned control/candidate artifact. It rejects generated sweeps and arms
with different seed ranges, bundle or runtime versions, duration, timestep, or
authored agent demand and journeys. The report records the shared execution
contract, two template hashes, every changed scenario section, each seed's
outcomes, exact aggregate count deltas, exit and terminal-state deltas,
intervention reach/acceptance deltas, queue-telemetry deltas, and clearance-time differences
only for seeds where both arms fully evacuated. A separately named last-exit-time
metric remains available when agents still remain in the system. It is a deterministic structural
comparison, not a control
group, causal estimate, uncertainty estimate, or predictive result. By default,
matching message or countermeasure identifiers retain their deterministic
acceptance stream; an explicit `sample` key can retain that stream across a
renamed intervention, and the comparison artifact discloses the alignment.

`sensitivity` makes best guesses inspectable rather than silently treating
them as facts. An authored JSON manifest names each mutable input, its discrete
alternatives, rationale, and basis, then produces a baseline replication sweep,
validated condition sweeps, seed-paired comparisons, and a bounded report. It
does not require research data and does not invent probability distributions or
confidence intervals for uncalibrated values. See the [sensitivity-study
workflow](docs/sensitivity.md).

`experiment plan` is the non-mutating review step for the corresponding
single-scenario artifact workflow. It checks the scenario, declared source
reports, and any optional OSM attestation before showing the exact authored
inventory. `experiment run` snapshots that contract and its deterministic
bundle; `experiment verify` reconstructs the artifact exactly. It is for
uncalibrated structural work, not an empirical gate. See [uncalibrated
experiment artifacts](docs/experiments.md).

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
$ PYTHONPATH=python/src python3 -m chiyoda_analysis.evidence_cli fetch \
    benchmarks/evidence/vru-trajectory-2022.json
$ cargo run -p chiyoda -- evidence lock \
    benchmarks/evidence/vru-trajectory-2022.json
$ cargo run -p chiyoda -- reference vru-trajectory \
    benchmarks/evidence/vru-trajectory-2022.json \
    -o out/vru-trajectory-reference.json
$ PYTHONPATH=python/src python3 -m chiyoda_analysis.evidence_cli fetch \
    benchmarks/evidence/wuppertal-crowdqueue-2018.json
$ cargo run -p chiyoda -- evidence lock \
    benchmarks/evidence/wuppertal-crowdqueue-2018.json
$ cargo run -p chiyoda -- reference crowd-queue \
    benchmarks/evidence/wuppertal-crowdqueue-2018.json \
    -o out/wuppertal-crowdqueue-reference.json
```

The Eindhoven report is explicitly `descriptive_only`; it cannot justify
predictive or operational use. Details, source limits, and the required next
review gate are in [evidence boundaries](docs/evidence.md) and the [calibration
protocol](docs/calibration-protocol.md).

The VRU catalog is a content-locked `uncalibrated_reference`: it makes a
CC BY 4.0 urban-intersection trajectory archive available for documented,
out-of-domain structural assumptions without inventing a held-out split. It is
not accepted by the calibration adapter or benchmark workflow.

The Wuppertal crowd-queue catalog is another `uncalibrated_reference`. Its
source-specific report identifies observed crossings through a controlled 0.5 m
entry gate, including a per-run descriptive flow summary. The values are only
used by the source-linked gate-capacity sensitivity example; they do not make
the token-service runtime a calibrated queue model.

An acquired OpenStreetMap XML extract can also be content-locked as an ODbL
`uncalibrated_reference` and inspected with `layout osm`. It emits attributed
geographic tag observations—not a scenario or inferred station geometry. See
[open-layout source observations](docs/layout-sources.md).

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
escalator south_escalator from platform at (30m, 8m, 6m) to concourse at (30m, 8m, 0m) width 1m belt 0.6m/s capacity 1.2/s clearance 2.1m
lift accessible_lift from platform at (5m, 8m, 6m) to concourse at (5m, 8m, 0m) cabin 2m 2m capacity 8 cycle 12s clearance 2.1m
connector-state planned_north_closure connector north_stair closed time 50s
gate fare_gate on concourse at (32m, 8m, 0m) width 2m capacity 18/s to street
agents passengers count 120 on platform at (8m, 8m, 6m) to street speed 1.2m/s radius 0.3m height 1.7m via fare_hall exclude stair release 0s

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

`make verify` formats, lints, tests, validates every checked-in evidence
catalog, smoke-tests every checked-in uncalibrated experiment and sensitivity
plan, and builds every workspace member. It does not fetch raw data: source
locks remain an explicit local acquisition step. The CI workflow performs the
same checks with a read-only token. Branch protection must be configured on the
GitHub repository separately; repository files cannot enforce it.

## Project structure

- `crates/chiyoda-core` — DSL parser, validator, canonical IR, evidence/source
  locks, bounded Parquet intake, deterministic generator, and reference runtime.
- `crates/chiyoda-cli` — compiler, runtime, generator, evidence, calibration,
  benchmark, and bundle-verification commands.
- `crates/chiyoda-replay` — native Linux trace viewer.
- `python` — dependency-light bundle verification and analysis helpers; it is
  intentionally outside the trusted simulation runtime. It independently checks
  bundle hashes and current metric-attribution invariants, but it does not
  reconstruct or execute the runtime.
- `benchmarks` — public fixtures and round-protocol materials; no empirical
  result is published until its evidence manifest validates. Its source
  screening decisions are documented in [data scouting](docs/data-scouting.md).
- `docs` — normative language, runtime, evidence, and release contracts.

## License

Apache-2.0. The project name references the Tokyo Metro Chiyoda Line; it does
not recreate, operationalize, or provide guidance about historic attacks.
