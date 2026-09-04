# Uncalibrated sensitivity studies

`chiyoda sensitivity MANIFEST -o DIRECTORY` turns a stated set of discrete,
authored input alternatives into independently verifiable, seed-paired
replication sweeps and comparison artifacts. It is available without an
evidence catalog, calibration protocol, or benchmark manifest.

The command is for sensitivity analysis: vary a declared input and inspect the
structural consequences in this reference runtime. It does not perform
uncertainty propagation because the manifest has no implied parameter
probability distribution. This distinction follows published modeling guidance:
[AHRQ's modeling guidance](https://effectivehealthcare.ahrq.gov/products/decision-models-guidance/methods)
separates sensitivity analysis from uncertainty propagation, and ASAM's
[OpenSCENARIO concepts](https://www.asam.net/fileadmin/Standards/OpenSCENARIO/ASAM_OpenSCENARIO_2-0_Concept_Paper.html)
similarly distinguish declared parameter variation from later stochastic
parameter models.

## Run an example

```console
$ cargo run -p chiyoda -- sensitivity-plan \
    examples/sensitivity/exit-capacity-and-trust.json \
    -o out/concourse-sensitivity-plan.json
$ cargo run -p chiyoda -- sensitivity \
    examples/sensitivity/exit-capacity-and-trust.json \
    -o out/concourse-sensitivity
$ cargo run -p chiyoda -- verify-sensitivity out/concourse-sensitivity
```

`sensitivity-plan` is non-mutating except for its optional JSON output file. It
resolves the baseline, validates every concrete condition, reports the exact
condition count, factor assignments, and canonical template hashes, but does
not execute the runtime or create a study directory. Review this plan before
raising `max_conditions` or launching an expensive factorial study.

The output directory must be empty. It contains:

- `manifest.json` — canonical snapshot of the input study contract;
- `baseline/` — an authored, verified replication sweep of the source scenario;
- `conditions/case-NNNN/` — one authored sweep for each concrete alternative;
- `comparisons/case-NNNN.json` — the existing seed-paired structural comparison
  from baseline to that condition; and
- `reference-reports/FACTOR/REFERENCE.json` — exact snapshots of any declared
  derived reference reports; and
- `report.json` — factor rationale, declared basis, baseline values, exact
  condition values, source/template hashes, outcome deltas, and interpretation
  boundaries. For a one-at-a-time design it also indexes each factor's ordered
  alternatives and their structural responses; factorial designs retain the
  condition-level results instead of assigning an unsupported individual-factor
  attribution.

Every condition uses the same contiguous runtime seeds as the baseline. This
holds deterministic variation constant where the runtime supports it; it does
not make seeds a sampled population or turn a delta into a causal estimate.

`verify-sensitivity` re-plans the persisted manifest against the baseline
template, verifies every constituent sweep, reconstructs every comparison, and
requires the saved comparison files and study report to match that
reconstruction exactly. It detects inconsistent or accidentally altered study
artifacts. It does not add a signature, establish an external chain of custody,
or change the study's uncalibrated interpretation boundary.

## Manifest

The manifest is strict JSON with schema version `0.1`.

```json
{
  "schema_version": "0.1",
  "name": "short study name",
  "description": "what structural question this explores",
  "baseline_source": "baseline.chy",
  "first_seed": 100,
  "count": 20,
  "design": "one_at_a_time",
  "max_conditions": 256,
  "factors": [
    {
      "id": "street_capacity",
      "target": "exit_capacity_per_s",
      "subject": "street",
      "values": [1.0, 2.0, 3.0],
      "basis": "best_guess",
      "rationale": "why these alternatives are worth exploring"
    }
  ],
  "claim_boundary": "what this study must not be used to claim"
}
```

`baseline_source` is resolved relative to the manifest. Every factor needs a
safe identifier, one existing named subject, at least two finite and distinct
values, an explicit basis, and a non-empty rationale. The baseline scenario is
parsed and validated before any output directory is created; every generated
condition is then validated again. An invalid alternative fails the study rather
than being silently skipped.

`basis` is one of `best_guess`, `documented_estimate`,
`structural_assumption`, or `measured_input`. The final label records input
provenance only; it does not calibrate the runtime or authorize empirical
claims.

A `documented_estimate` or `measured_input` factor must retain at least one
reference. Every reference has a stable identifier, citation, HTTPS source URL,
applicability statement, limitation statement, and optionally the SHA-256 of
the exact source file. A reference can additionally declare a local JSON
`derived_report` and its SHA-256. Planning checks that report; execution copies
its exact bytes into `reference-reports/FACTOR/REFERENCE.json`; study verification
checks the saved snapshot without depending on the original path. The reference
documents selection of alternatives; it does not make their values draws from a
distribution or establish source-to-scenario transferability.

`examples/sensitivity/urban-reference-speed.json` demonstrates this with the
source-locked VRU report. Its source is explicitly urban and out of domain for a
transit concourse, so its values are broad sensitivity brackets rather than
defaults. First reproduce that report with `chiyoda reference vru-trajectory`
as described in [evidence boundaries](evidence.md).

`examples/sensitivity/crowd-queue-gate-capacity.json` applies the same contract
to the locked Wuppertal crowd-queue report. It uses that adapter's per-run
P05/P50/P95 passage-flow order statistics through one fixed 0.5 m controlled
entry gate, snapshots the derived report, and varies only an authored exit
capacity. The source remains out of domain for a station or evacuation; its
values are structural alternatives, not a capacity law or queue calibration.

`examples/sensitivity/arrival-cadence.json` varies a declared
`agent_release_interval_s`, `agent_release_batch_size`, and
`gate_capacity_state_per_s` for the uncalibrated interchange example. They are
deterministic demand and service-limit schedule sensitivities, not an
arrival-rate fit, demand model, or observed operating profile. Batch-size
variation requires an authored release interval; an omitted batch size has
baseline value one.

## Designs and limits

`one_at_a_time` is the default. It creates one condition for each factor/value
that differs from the baseline, leaving every other factor at its baseline
value. Use it to isolate the reference runtime's response to each selected
input.

`full_factorial` evaluates every Cartesian combination of factor values, apart
from the duplicated all-baseline combination. Use it when interaction behavior
is relevant. `max_conditions` is a declared execution guard, defaulting to
256; increase it deliberately only after accounting for the number of seeds and
the scenario runtime. It is not a research-data gate.

The command supports these numeric targets:

| Target | Subject | Unit |
| --- | --- | --- |
| `agent_count` | agent group | agents |
| `agent_speed_mps`, `agent_radius_m`, `agent_height_m`, `agent_release_at_s`, `agent_release_interval_s`, `agent_release_batch_size` | agent group | target suffix |
| `exit_capacity_per_s` | exit with an authored capacity | `/s` |
| `connector_capacity_per_s` | non-lift connector with an authored capacity | `/s` |
| `exit_capacity_state_per_s`, `connector_capacity_state_per_s`, `gate_capacity_state_per_s` | named capacity-state declaration | `/s` |
| `escalator_belt_speed_mps` | escalator | `m/s` |
| `gate_service_rate_per_s` | gate | `/s` |
| `message_trust`, `message_reach_m` | message | target suffix |
| `countermeasure_trust`, `countermeasure_reach_m` | countermeasure | target suffix |

Unbounded exits and connectors intentionally cannot be treated as a numeric
baseline capacity. Add an explicit capacity to the authored scenario first if a
finite-capacity sensitivity question is intended. Values use their target's
fixed unit: no implicit unit conversion occurs in the manifest.

The ordinary `compare-sweeps` command continues to require identical authored
agent declarations. A sensitivity condition may deliberately vary one of the
agent targets above. Its comparison then records `agent_declarations_matched:
false` and preserves both `baseline_total_agents` and
`candidate_total_agents` for every seed. Those are raw structural outcomes, not
normalized rates or evidence that a changed demand represents the same
population. Agent-speed and agent-count factors therefore execute as declared
without weakening the comparability contract for ordinary intervention
comparisons.

## Interpretation

The report carries both the author-supplied claim boundary and a fixed product
boundary. It never assigns a likelihood, distribution, confidence interval, or
rank to alternatives. Its one-at-a-time response index is an ordered lookup of
exact structural deltas, not an elasticity, derivative, or estimate of input
importance. A sensitivity result exposes how declared assumptions affect this
deterministic model; it does not establish which alternative is more plausible
or predict a real crowd, facility, message response, or safety outcome.
