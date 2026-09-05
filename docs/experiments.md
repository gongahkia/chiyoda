# Uncalibrated experiment artifacts

`chiyoda experiment run` creates one self-verifying artifact for an authored,
deterministic structural experiment. It is the normal path when a scenario is
informed by best guesses, a public layout observation, a descriptive report, or
other disclosed context but does not represent a calibrated or predictive model.

This is not a research-program gate. It is an audit trail: every serious run
records what was authored, why it was chosen, which source reports informed it,
and what the result must not be used to claim.

## Start a no-data draft

`experiment init` creates an editable, no-data draft from a deterministic
generated scenario. It writes only `scenario.chy` and `experiment.json` to an
empty directory; it does not execute the runtime, acquire a source, or create
a prediction.

```console
$ cargo run -p chiyoda -- experiment init \
    --name "concourse draft" --seed 73 -o out/concourse-draft
$ cargo run -p chiyoda -- experiment plan out/concourse-draft/experiment.json
```

The generated manifest has no sources, preserves the requested trace cadence,
and explicitly labels generated topology, passenger demand/motion, and service
and information conditions as structural assumptions or best guesses. Review
and edit both files before a run. When ready, use the ordinary `experiment run`
and `experiment verify` commands below. This is the normal starting path for an
uncalibrated structural question; an OSM source, evidence catalog, calibration
protocol, and benchmark manifest are all optional and unnecessary here.

Pass `--with-sensitivity` to also create `sensitivity.json`. It uses the
generated scenario as its baseline and creates one-at-a-time alternatives for
the generated passenger count and walking-speed, gate-service-rate,
misinformation-trust, corrective-message-trust, and corrective-message-time
inputs. Each has the `best_guess` basis, no sources, and no probability
distribution. It is a reviewable starting bracket, not an estimate of real
people or operations. It uses eight deterministic replications per condition by default. Set
`--sensitivity-runs COUNT` before creation to choose the workload deliberately.
The generated schema-`0.4` experiment also links that companion manifest, so
its plan identifies which typed best guesses are covered by those alternatives
and which remain unexamined. This records a declared study contract only; run
and verify `sensitivity` separately before describing any structural outcomes.

```console
$ cargo run -p chiyoda -- experiment init \
    --name "concourse draft" --seed 73 --with-sensitivity --sensitivity-runs 20 \
    -o out/concourse-draft
$ cargo run -p chiyoda -- sensitivity-plan out/concourse-draft/sensitivity.json
$ cargo run -p chiyoda -- sensitivity out/concourse-draft/sensitivity.json -o out/concourse-sensitivity
$ cargo run -p chiyoda -- verify-sensitivity out/concourse-sensitivity
```

## Manifest

The baseline manifest uses schema `0.1` and is strict JSON. Schema `0.2` adds
optional OSM source attestations: a content-locked observation/local projection
and, when wanted, selected source-point-to-scenario-point anchors. Schema
`0.3` additionally lets an assumption name its exact authored numeric inputs
using the sensitivity target vocabulary. Schema `0.4` adds optional linked
sensitivity-study contracts. Use `0.4` when declaring `sensitivity_studies`.
Every path is resolved relative to the manifest. Every experiment must state a
non-empty `claim_boundary` and at least one assumption.

```json
{
  "schema_version": "0.4",
  "name": "concourse gate structural exploration",
  "description": "inspect the deterministic response to one authored gate configuration",
  "scenario_source": "concourse.chy",
  "trace_every_steps": 10,
  "assumptions": [
    {
      "id": "street_gate_capacity",
      "subject": "fare_gate.capacity",
      "basis": "best_guess",
      "rationale": "there is no facility-specific measured service rate",
      "targets": [
        {"target": "gate_service_rate_per_s", "subject": "fare_gate"}
      ]
    }
  ],
  "sensitivity_studies": [
    {
      "id": "gate_capacity_bracket",
      "manifest_path": "gate-capacity-sensitivity.json"
    }
  ],
  "sources": [],
  "claim_boundary": "This is an uncalibrated structural experiment, not a prediction, operational recommendation, or safety assessment."
}
```

`basis` uses the same labels as a sensitivity study: `best_guess`,
`documented_estimate`, `structural_assumption`, or `measured_input`. The latter
two evidence-oriented labels require `source_ids` referring to records in the
manifest's `sources` array. Each source needs a citation, HTTPS URL,
applicability and limitation statement, and may retain an exact source SHA-256.

Beginning with schema `0.3`, `targets` is optional because some assumptions describe a
structural choice rather than a mutable numeric field. Each target has a
`target` (for example `agent_speed_mps` or `gate_service_rate_per_s`) and the
authored resource `subject`. A target is resolved against the scenario when
planning, running, and verifying; a nonexistent subject or unsupported field
fails instead of leaving a prose reference that cannot be checked. One
target/subject pair may appear under only one assumption, so competing
justifications cannot silently describe the same input. The plan and current
artifact report retain the exact baseline value and unit. This is input
traceability—not a parameter distribution, uncertainty interval, calibration,
or claim of model validity.

### Linking declared sensitivity coverage

Schema `0.4` may include `sensitivity_studies`. Each entry has an identifier
and a path to a strict `sensitivity` manifest. At planning and artifact
creation, Chiyoda parses that manifest, validates all of its conditions, and
requires its baseline scenario to have the same canonical scenario hash as the
experiment. Every linked factor must name one of the experiment's typed
`target`/`subject` pairs. This prevents a bracket from silently varying a
different or undisclosed input.

The plan and schema-`0.4` artifact report retain the sensitivity manifest hash,
design, condition count, factor alternatives, and a target-by-target crosswalk.
An empty `sensitivity_factors` list explicitly means that the input is
disclosed but no linked study varies it. Coverage does not mean that a
simulation was run, that alternatives are likely, or that output uncertainty
has been quantified. This distinction follows guidance that treats sensitivity
analysis, verification/validation, and uncertainty quantification as distinct
activities: [NIST IR 8298](https://www.nist.gov/publications/summary-industrial-verification-validation-and-uncertainty-quantification-procedures)
and [ISPOR's modeling guidance](https://pubmed.ncbi.nlm.nih.gov/12535234/).

For example, the checked-in
`examples/experiments/uncalibrated-interchange.json` links
`uncalibrated-interchange-sensitivity.json`. That companion varies release
cadence and the scheduled gate reduction; passenger count, speed, body envelope,
and baseline gate service remain explicitly listed as not covered by that one
study. It remains an uncalibrated structural exercise.

A source can also declare the exact local JSON report which informed the choice:

```json
{
  "id": "gate_reference",
  "citation": "Adrian et al. (2018)",
  "url": "https://ped.fz-juelich.de/da/crowdqueue",
  "applicability": "documents a broad, out-of-domain gate-flow bracket",
  "limitation": "does not calibrate a station or Chiyoda's service-token model",
  "source_sha256": "EXACT-64-CHARACTER-RAW-SOURCE-SHA256",
  "derived_report": {
    "path": "gate-reference.json",
    "sha256": "EXACT-64-CHARACTER-REPORT-SHA256"
  }
}
```

The command checks the report is JSON, verifies its declared byte hash, and
copies its exact bytes. A URL or source checksum documents selection context; it
does not transfer validity from a source to this scenario.

### Retaining and attesting a local OSM reference

An OSM local-coordinate reference can be retained in exactly the same way.
Schema `0.2` can also make its creation-time provenance check part of the
experiment contract. Generate the observation and projection first:

```console
$ cargo run -p chiyoda -- layout verify-osm layout-catalog.json observations.json \
    --data-root data/raw
$ cargo run -p chiyoda -- layout verify-projection layout-catalog.json \
    observations.json local-reference.json --data-root data/raw
$ sha256sum local-reference.json
```

```json
{
  "id": "station_layout_reference",
  "citation": "OpenStreetMap contributors (2026), reviewed station-area extract",
  "url": "https://EXACT-OSM-EXTRACT-URL",
  "applicability": "provides an attributed, explicitly anchored east/north reference for manual scenario authoring",
  "limitation": "does not establish surveyed facility geometry, elevations, connectivity, capacity, accessibility, or runtime validity",
  "source_sha256": "EXACT-64-CHARACTER-OSM-XML-SHA256",
  "derived_report": {
    "path": "local-reference.json",
    "sha256": "EXACT-64-CHARACTER-LOCAL-REFERENCE-SHA256"
  }
}
```

To attest it, use schema `0.2` or later and add this root-level field. Its `source_id`
must name the source declaring `local-reference.json` above.

```json
{
  "schema_version": "0.2",
  "source_attestations": [
    {
      "kind": "osm_local_projection",
      "source_id": "station_layout_reference",
      "catalog_path": "layout-catalog.json",
      "data_root": "data/raw",
      "observation_report_path": "observations.json"
    }
  ]
}
```

At `experiment run`, Chiyoda checks the catalog and locked XML, reconstructs
the observation report, then reconstructs the derived local projection. It
copies the catalog and observation report into the artifact alongside the
already content-locked local-reference report. Creation therefore fails if the
local raw XML, its catalog lock, observation, or projection has changed.

Later `experiment verify` validates the captured catalog and its exact link to
the observation’s dataset identity, source URL, locked raw-file hash and size,
license, attribution, and catalog hash; it then reconstructs the
observation-to-projection link. It deliberately does not fetch a URL or
revalidate raw XML: the raw file is not copied into an artifact and an offline
verifier must not claim to have revalidated an external source. This proves the
provenance chain captured at creation, not facility geometry, model calibration,
or any operational result.

### Retaining selected OSM scenario-point anchors

When a manually authored scenario point exactly uses a projected OSM point,
create and verify the narrow anchor report first. See
[layout sources](layout-sources.md#anchor-selected-scenario-points) for
the `layout anchor-osm` and `layout verify-anchor-osm` commands. The anchor
report is a separate derived report and needs its own source declaration; its
raw-source SHA-256 must match the report's OSM source hash.

The anchor attestation depends on an `osm_local_projection` attestation. Its
`projection_source_id` names that attestation's `source_id`; its `source_id`
names the source that declares the anchor report. The anchor manifest's
declared scenario file must have exactly the same bytes as the experiment's
scenario source at planning and artifact creation.

```json
{
  "sources": [
    {
      "id": "station_layout_reference",
      "citation": "OpenStreetMap contributors (2026), reviewed station-area extract",
      "url": "https://EXACT-OSM-EXTRACT-URL",
      "applicability": "provides an explicitly anchored east/north reference",
      "limitation": "does not establish surveyed facility geometry or runtime validity",
      "source_sha256": "EXACT-64-CHARACTER-OSM-XML-SHA256",
      "derived_report": {
        "path": "local-reference.json",
        "sha256": "EXACT-64-CHARACTER-LOCAL-REFERENCE-SHA256"
      }
    },
    {
      "id": "main_entrance_anchor",
      "citation": "OpenStreetMap contributors (2026), reviewed station-area extract",
      "url": "https://EXACT-OSM-EXTRACT-URL",
      "applicability": "retains one selected source point for manual scenario authoring",
      "limitation": "does not import geometry, establish access, or calibrate the runtime",
      "source_sha256": "EXACT-64-CHARACTER-OSM-XML-SHA256",
      "derived_report": {
        "path": "main-entrance-anchor.json",
        "sha256": "EXACT-64-CHARACTER-ANCHOR-REPORT-SHA256"
      }
    }
  ],
  "source_attestations": [
    {
      "kind": "osm_local_projection",
      "source_id": "station_layout_reference",
      "catalog_path": "layout-catalog.json",
      "data_root": "data/raw",
      "observation_report_path": "observations.json"
    },
    {
      "kind": "osm_scenario_anchor",
      "source_id": "main_entrance_anchor",
      "projection_source_id": "station_layout_reference",
      "anchor_manifest_path": "main-entrance-anchors.json"
    }
  ]
}
```

At creation Chiyoda validates the catalog-to-projection chain and then
reconstructs the anchor report from the exact anchor-manifest bytes, experiment
scenario bytes, and local-projection report bytes. It snapshots the anchor
manifest next to the report snapshots. Offline verification repeats that final
reconstruction using only the artifact. This proves only the selected point
link; it does not turn surrounding authored surfaces, paths, widths,
connectivity, elevation, capacity, or behavior into map-derived facts.

## Plan, run, and verify

```console
$ cargo run -p chiyoda -- experiment plan experiment.json -o out/gate-experiment-plan.json
$ cargo run -p chiyoda -- experiment run experiment.json -o out/gate-experiment
$ cargo run -p chiyoda -- experiment verify out/gate-experiment
```

`experiment plan` is non-mutating except for its optional JSON output. It
parses and validates the scenario; verifies declared derived-report hashes; and
when schema `0.2` or later declares an OSM attestation, rechecks the local locked XML,
observation, and projection before reporting success, and reconstructs any
declared scenario anchors. The plan lists every assumption and source, the
canonical scenario hash, the trace cadence, exact integration-step and stored
trace-frame counts, all authored structure counts, the declared agent count,
state/capacity changes, and information interventions.
Review it before treating a best guess as a chosen input or invoking the
runtime. It does not create an artifact or produce an outcome.

The output directory must be empty. It contains:

- `manifest.json` — canonical snapshot of the declared contract;
- `scenario.chy` — exact authored source snapshot;
- `run.json` — independently bundle-hash-verifiable runtime artifact;
- `source-reports/SOURCE.json` — exact derived-report snapshots, when declared;
- `source-attestations/SOURCE/{catalog,observation}.json` — OSM catalog and
  source-observation snapshots for an `osm_local_projection` attestation;
- `source-attestations/SOURCE/anchor-manifest.json` — the exact manifest for an
  `osm_scenario_anchor` attestation; its report remains content-locked in
  `source-reports/SOURCE.json`;
- `sensitivity-studies/ID/{manifest.json,baseline.chy}` — exact linked
  sensitivity-contract and baseline-source snapshots, when schema `0.4`
  declares that study;
- `report.json` — hashes, the bundle/scenario linkage, source-snapshot paths,
  both author/product claim boundaries, and a directly reconstructed mirror of
  the run's exact runtime metrics. The schema-`0.4` report additionally retains
  each typed assumption target's resolved baseline value and unit, plus the
  linked-study coverage crosswalk. Its metric mirror includes agent and
  evacuation counts, final-exit and terminal state attribution, intervention
  delivery counts, timing fields, and queue exposure plus discrete wait/peak
  telemetry. These are deterministic observations from this one configured run,
  not estimates, uncertainty measures, or real-world outcomes.

`experiment verify` rejects unexpected files, altered source-report snapshots,
source attestations, or sensitivity-study contracts, a changed scenario/run pairing, a broken
projection-to-anchor link, a bundle hash failure, a trace-frequency mismatch,
or a report (including its mirrored metrics) that does not exactly reconstruct.
It reruns the deterministic reference runtime from the scenario snapshot and
rejects even a self-hashed run bundle that differs from that reconstruction.
Existing `0.1` reports without the metric mirror, `0.2` reports without typed
assumption targets, and `0.3` reports without linked sensitivity coverage
remain verifiable for audit.
It does not validate a public source, prove the scenario represents a facility,
or elevate the run beyond its uncalibrated boundary.

Use `sensitivity` in addition when material inputs have multiple plausible
values. The experiment artifact is the single-scenario audit trail; sensitivity
is the explicit alternative-analysis workflow.
