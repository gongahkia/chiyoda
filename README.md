# Chiyoda

Chiyoda is an Apache-2.0 research platform for deterministic, reproducible
3D pedestrian-flow experiments in transit interchanges. Its primary artifact
is a standalone experiment language and executable reference semantics—not a
real-time operations dashboard or a certified evacuation product.

The current `0.42.0-alpha.1` release establishes the language/runtime contract:

- a typed textual DSL with static unit, topology, reachability, capacity, and
  deterministic-replay checks;
- a Rust reference runtime for radius-aware, height-annotated agents with
  authored connector-eligibility constraints and alternative final exits on
  connected 3D walkable surfaces with rectangular obstacles, stairs, ramps,
  escalators, lifts, gates, capacity-limited exits, scheduled availability and
  service-capacity states, explicitly authored portal lanes, and typed
  information interventions;
- immutable JSON run bundles with source, canonical IR, events, traces,
  per-intervention reach/acceptance metrics, queue-entry audit events,
  local-motion adjustment, ORCA-fallback, integration-boundary, and analytic
  same-surface-interval reference-disc-overlap audit telemetry, and SHA-256
  integrity hashes;
- a source-anchoring workflow that can prove selected scenario coordinates
  match a content-locked, explicitly projected OSM point without importing map
  geometry;
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

`portal-lanes` is an optional geometric placement declaration, not a capacity
or calibration feature. It partitions a connector, gate, or exit's already
authored width into named landing/target lanes. The runtime assigns an agent a
stable lane by identifier and, for a connector, leaves the agent in transit
until that exact lane is clear. It never derives throughput from width.

`queue-footprint` and multi-lane `queue-grid` are separate optional declarations for a lift, a non-lift
capacity-constrained connector, a gate, or a capacity-constrained exit. A line
footprint retains capacity-denial queueing; a grid preallocates deterministic FIFO
slots before on-surface motion and records entry when the agent reaches its slot.
Both validate slots against obstacles, FIFO transitions, and agent radii. A front
agent reserves either its lift's cabin place or the existing authored service token
before approaching its portal.
Within one explicit grid, an earlier FIFO ticket has deterministic local-motion
right-of-way over later tickets; a later ticket uses its predecessor's selected
same-step velocity rather than a stale velocity. Scheduled release is likewise
an eligibility time: a generated spawn disc is held outside the modeled surface
until it is dynamically clear, with an audit event. These are queue-formation
and reference-clearance rules, not safety or behavioral-fidelity claims.
This is still an uncalibrated reference queue model: it does not infer staffing,
queue discipline, demand, or observed facility behaviour.

`examples/experiments/queue-grid-stress.chy` is a repeatable 152-agent
formation stress case for the grid contract. Its declared geometry, demand, and
gate rate are uncalibrated structural assumptions. Use it to compare runtime
changes and inspect fallback/overlap telemetry; it is not a facility model,
capacity estimate, or safety test.
`queue-grid-stress-sensitivity.json` supplies a one-seed, four-condition
agent-count envelope for the same structural case. Run `sensitivity-plan`
before execution to inspect its exact workload and claim boundary.
`queue-grid-reference-clearance.chy` is a separate four-agent structural
fixture that passes the reference-clearance gate; it is a test fixture, not a
safe operating envelope.

For a strict internal reference-runtime acceptance boundary, run
`chiyoda verify-reference-clearance PATH/run.json`. It first reconstructs the
current bundle and then requires both reference-disc overlap audits to be zero.
The queue-grid stress case intentionally does not pass this check: it is a
dense failure-finding scenario. Passing the check does not certify a facility,
contact model, or physical safety.
The separate [queue-grid coordination architecture](docs/coordination.md)
documents the bounded continuous-time planning layer under construction; the
current local FIFO rule is not presented as a complete coordination solver.
Use `coordinate-queue-grid` only when you explicitly want that separate
planner: it writes an embedded-source artifact containing either exact-model
clear timed trajectories or a precise bounded no-plan/unresolved result.
`verify-queue-grid-coordination` reconstructs that result. Neither command
changes the default `run` runtime or turns an audit failure into a clearance
claim.

## Start without data

To start an uncalibrated structural exploration, no dataset, research protocol,
or external account is needed. This creates a deterministic generated scenario
and an editable manifest whose inputs are explicitly labelled as best guesses
or structural assumptions:

```console
$ cargo run -p chiyoda -- experiment init --name "concourse draft" --seed 73 -o out/concourse-draft
$ cargo run -p chiyoda -- experiment plan out/concourse-draft/experiment.json
```

Edit `out/concourse-draft/scenario.chy` and
`out/concourse-draft/experiment.json` until the disclosed inputs represent the
structural question you want to explore. The generated schema-`0.4` manifest
links numeric best guesses to exact authored targets and, with
`--with-sensitivity`, links the companion study contract. Planning makes both
covered and unexamined best guesses explicit without treating them as
calibrated estimates. Then create a deterministic,
self-verifying artifact:

```console
$ cargo run -p chiyoda -- experiment run out/concourse-draft/experiment.json -o out/concourse-run
$ cargo run -p chiyoda -- experiment verify out/concourse-run
```

`experiment init` does not run the runtime, download data, or create a
prediction. The generated topology and all population, service, and information
values are a reviewable starting point, not facility facts. Use
[`sensitivity`](docs/sensitivity.md) when more than one value is plausible. Add
`--with-sensitivity` to `experiment init` to create a companion
`sensitivity.json` that brackets the generated passenger count and speed, gate
service rate, misinformation trust, corrective-message trust, and
corrective-message time as explicit best guesses. The experiment plan records
that this companion explores only those linked inputs; it does not claim the
study has run or quantify uncertainty. The starter uses eight
deterministic replications per condition by default; set `--sensitivity-runs
COUNT` before creation when a shorter smoke study or a larger structural
exploration is appropriate:

```console
$ cargo run -p chiyoda -- experiment init --name "concourse draft" --seed 73 \
    --with-sensitivity --sensitivity-runs 20 -o out/concourse-draft
$ cargo run -p chiyoda -- sensitivity-plan out/concourse-draft/sensitivity.json
$ cargo run -p chiyoda -- sensitivity out/concourse-draft/sensitivity.json -o out/concourse-sensitivity
$ cargo run -p chiyoda -- verify-sensitivity out/concourse-sensitivity
```

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
$ cargo run -p chiyoda -- layout anchor-osm my-layout-catalog.json out/layout-observations.json out/layout-local-reference.json my-scenario-anchors.json -o out/layout-scenario-anchors.json
$ cargo run -p chiyoda -- layout verify-anchor-osm my-layout-catalog.json out/layout-observations.json out/layout-local-reference.json my-scenario-anchors.json out/layout-scenario-anchors.json
$ cargo run -p chiyoda -- replay out/example/run.json
$ cargo run -p chiyoda-replay -- out/example/run.json
$ cargo run -p chiyoda-replay -- --watch example.chy --paused
```

The first replay command reconstructs a compatible bundle before printing a
summary. The second opens the native replay viewer; it also reconstructs a
compatible bundle and requires an available Linux display server. It renders
the selected authored surface, obstacles, waypoints, exits, gates, connector
endpoints, portal lanes, and line/grid queue slots and paths behind the agents. Pass
`--surface <id>` to choose its initial floor
or press Tab to cycle floors. Its visual scope and trace-position boundary are
documented in the [native replay viewer guide](docs/replay.md).
`chiyoda-replay --watch` is the local visual-debug loop: after each saved valid
DSL revision it recompiles and reruns the scenario in memory, then replaces the
displayed trace. It neither writes a run bundle nor upgrades the result to a
verified artifact; use `chiyoda run` when a durable JSON bundle is required.
`sweep` is an
uncalibrated structural experiment: it generates and
runs a contiguous seed range, writes one independently hash-verifiable bundle
per seed, and records their summaries in `summary.json`, including final-exit
attribution, modeled queue-exposure counts, and current per-run discrete queue
telemetry attributed to individual constrained resources. Current bundles also
contain one auditable event for each agent's first entry to each modeled
resource queue and local-motion telemetry. The `run` and textual
`replay` summaries name the latter as affected agents, adjusted position
attempts, cumulative planned-to-resolved displacement, and the largest
adjustment. Current bundles also report every speed-bounded ORCA constraint
fallback and cross-check that count against its event trail; it is structural
runtime telemetry, not a density or collision measurement. They additionally
audit every integration boundary for overlapping on-surface reference discs,
reporting affected agents, overlapping pair-steps, and the largest overlap.
The endpoint audit does not sweep movement between boundaries. A separate
analytic audit computes the minimum horizontal separation of each eligible
pair's linearly interpolated same-surface movement paths over an integration
interval, so it can disclose an interior crossing that the endpoint audit
misses. It deliberately excludes surface-transition intervals and neither
audit reports observed physical contact or safety. Generated cases include a
scheduled primary-exit closure, so the output also exercises rerouting.
For a connector, gate, or exit without `queue-footprint`, an agent becomes ready
for service when its authored clearance disc contacts the point; local clearance
still applies throughout the approach. This avoids a numerical deadlock at a
zero-geometry service point without claiming to model a physical queue.
`verify-sweep` cross-checks that summary against every bundle and its canonical
source, and reruns each bundle compatible with the installed runtime to reject
self-hashed fabricated results. `analyze-sweep` performs that same verification before producing exact
cross-run counts, per-exit totals, intervention reach/acceptance totals, modeled
queue-exposure and discrete queue-telemetry totals, local-motion aggregates
(including separately coverage-labeled boundary and analytic interval
on-surface reference-disc audits), and
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
[open-layout source observations](docs/layout-sources.md). A later
`layout anchor-osm` artifact can prove a deliberately selected scenario point
equals one selected projected OSM point; it still cannot infer a usable layout
or any physical/operational property from the map.

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
