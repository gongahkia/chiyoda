# Generated benchmark protocol

Chiyoda evaluates runtime generalization with a deterministic constraint-based
generator rather than a permanently fixed set of scenarios. The generator is
not a source of empirical truth: it produces only structurally valid candidate
experiments.

## Round lifecycle

1. Publish a generator version, constraints, fixture seeds, evidence manifest,
   scoring code, and calibration split.
2. Commit the SHA-256 hash of the held-out evaluation seed list before results
   are accepted.
3. Run the reference and submitted runtimes against the sealed seed list.
4. Publish artifacts, scores, seed list, and verification instructions at the
   end of the round.
5. Start the next round with a new committed seed list.

This permits genuine held-out evaluation while making every completed round
independently reproducible. Hosted-only permanent secret evaluation is out of
scope.

## Current status

`0.24.0-alpha.1` includes the generator, fixture-seed protocol, and manifest
validator. It now also includes a content-locked candidate source and a
descriptive 2D platform-trajectory intake path. It does not publish an
empirical round: the source is neither a calibrated runtime nor independent
facility/primitives validation. See the [evidence boundary](evidence.md).

## Uncalibrated structural sweeps

`chiyoda sweep --seed FIRST --count COUNT -o DIRECTORY` generates and runs a
contiguous seed range without consulting any evidence catalog or benchmark
manifest. It writes canonical source and a hash-verifiable run bundle under
`seed-SEED/` for every run, plus `summary.json` listing each bundle hash,
basic outcome metrics, evacuations attributed to each final exit, and current
queue-exposure counts plus discrete queue telemetry for each modeled wait
state. Current telemetry also maps each modeled wait to the individual
capacity-constrained resource that denied it, retaining zero-valued entries for
unreached constrained resources. Current bundles record a first-entry audit
event for each agent/resource queue pair; sweep verification cross-checks those
events against both the per-resource and aggregate exposure telemetry.
`chiyoda verify-sweep DIRECTORY` cross-checks that summary against every
bundle hash, metric, and canonical source. For bundles with the installed
runtime and bundle versions, it also reruns the scenario and rejects a
self-hashed bundle that differs from deterministic reconstruction. Older
runtime contracts remain hash-, source-, and metric-checked but cannot be
reconstructed by an incompatible installed runtime. The supplied output directory must
be empty so an existing experiment artifact cannot be silently overwritten.
Each generated case declares a primary exit, an alternative final exit, and a
scheduled closure of the primary, so a sweep exercises the deterministic
rerouting semantics as well as its outcome attribution.

`chiyoda replicate SOURCE --seed FIRST --count COUNT -o DIRECTORY` uses the
same bundle, verification, and analysis format for a user-authored scenario.
It records canonical `template.chy` and its scenario hash, and every replica is
required to match that template except for the declared seed. This is the
appropriate uncalibrated workflow for comparing an authored intervention or
information design over deterministic trust samples; it does not make those
samples empirical observations.

`chiyoda analyze-sweep DIRECTORY [-o REPORT.json]` first performs every
`verify-sweep` check, then emits an explicitly descriptive aggregate: exact
agent and evacuation counts, exact overall evacuation fraction numerator and
denominator, per-exit totals, legacy unattributed evacuations, and a minimum,
mean, and maximum clearance time over only fully evacuated runs, a separately
named last-exit-time range over runs that recorded an evacuation, aggregate
per-intervention reach/acceptance counts, aggregate queue exposure, cumulative
queue-wait agent-seconds, and the largest per-run step-boundary queue peak for
each mechanism. Each current constituent run also retains its resource-level
breakdown. Analysis separately aggregates each authored lift, connector, gate,
and exit across the runs that expose that attribution, with an observed versus
legacy-unobserved run count. A resource's maximum peak is its largest
single-run step-boundary peak; neither that number nor the resource peak set
reconstructs a mechanism's simultaneous aggregate peak.
Queue exposure is the number of agents that ever entered each modeled lift,
connector, gate, or exit wait state. The telemetry reflects only the authored
discrete runtime; it is neither an observed queue length, delay, throughput,
nor physical-flow measurement. Older summaries without a field are
distinguishable at deserialization; verified analysis hydrates exposure from a
constituent bundle when available, but never invents absent detailed telemetry.
It does not
estimate uncertainty or turn generator seeds into a population sample. Final
non-evacuated states are also summed, with legacy agents lacking state
attribution reported separately rather than silently assigned a cause.

This command supports structural exploration and regression investigation. It
does not produce a benchmark score, calibration result, or predictive claim.

## Uncalibrated sensitivity studies

`chiyoda sensitivity MANIFEST -o DIRECTORY` is a separate structural workflow
for discrete, declared alternatives to authored numeric inputs. It runs a
baseline and every valid condition over the same contiguous seed range, writes
hash-verifiable replication sweeps, and preserves the seed-paired comparisons.
`chiyoda sensitivity-plan MANIFEST [-o PLAN.json]` resolves and validates this
condition set without executing it, including the exact condition count and
canonical template hashes.
`chiyoda verify-sensitivity DIRECTORY` verifies the whole resulting study,
including every sweep, manifest-derived condition, saved comparison, and report.
The manifest requires a rationale and basis for each factor but no dataset or
probability distribution. `one_at_a_time` examines individual input changes;
`full_factorial` examines declared interactions within an explicit condition
limit. The result is a sensitivity artifact, not parameter uncertainty,
calibration, a benchmark score, or a predictive claim. See [sensitivity
studies](sensitivity.md).

## Authored intervention comparisons

`chiyoda compare-sweeps BASELINE CANDIDATE [-o REPORT.json]` is the
uncalibrated comparison workflow. Both directories must first pass the full
replication verifier and therefore carry an authored `template.chy`, a template
hash, and one bundle per declared seed. The command accepts only equal
contiguous seed ranges and rejects arms whose bundle or runtime version,
duration, timestep, or authored agent groups (including their journeys) differ.
It does not require geometry,
capacity, route-state, or information declarations to be identical: the report
lists every changed top-level scenario section and retains the two canonical
template hashes and shared execution contract so the actual intervention remains
inspectable.

The report has one row per common seed, with arm-specific bundle hashes,
evacuation counts, final-exit attribution, remaining-state attribution, current
queue telemetry (including current per-resource attribution and aggregate
per-resource deltas when both arms expose it), and a
clearance-time delta only if both runs completed; separately named last-exit
times remain available for partially completed runs. It additionally reports exact
aggregate candidate-minus-baseline count deltas and separates pairs with only
one completed arm from comparable clearance-time pairs, including intervention
reach/acceptance and queue-telemetry deltas. No confidence interval,
significance label, causal conclusion, or predictive interpretation is
emitted. A shared seed labels deterministic scenario variation; it is not an
empirical sample. Information acceptance samples also incorporate the message
or countermeasure identifier by default, so changing an identifier changes that
stream. An explicit, globally unique `sample` key can instead align a stream
across the two arms; the comparison artifact reports shared and arm-specific
keys without treating them as empirical observations.
