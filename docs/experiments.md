# Uncalibrated experiment artifacts

`chiyoda experiment run` creates one self-verifying artifact for an authored,
deterministic structural experiment. It is the normal path when a scenario is
informed by best guesses, a public layout observation, a descriptive report, or
other disclosed context but does not represent a calibrated or predictive model.

This is not a research-program gate. It is an audit trail: every serious run
records what was authored, why it was chosen, which source reports informed it,
and what the result must not be used to claim.

## Manifest

The baseline manifest uses schema `0.1` and is strict JSON. Schema `0.2` adds
optional source attestations for a content-locked OSM observation and local
projection; use it only when declaring `source_attestations`. Every path is
resolved relative to the manifest. Every experiment must state a non-empty
`claim_boundary` and at least one assumption.

```json
{
  "schema_version": "0.1",
  "name": "concourse gate structural exploration",
  "description": "inspect the deterministic response to one authored gate configuration",
  "scenario_source": "concourse.chy",
  "trace_every_steps": 10,
  "assumptions": [
    {
      "id": "street_gate_capacity",
      "subject": "fare_gate.capacity",
      "basis": "best_guess",
      "rationale": "there is no facility-specific measured service rate"
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

To attest it, use schema `0.2` and add this root-level field. Its `source_id`
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

## Plan, run, and verify

```console
$ cargo run -p chiyoda -- experiment plan experiment.json -o out/gate-experiment-plan.json
$ cargo run -p chiyoda -- experiment run experiment.json -o out/gate-experiment
$ cargo run -p chiyoda -- experiment verify out/gate-experiment
```

`experiment plan` is non-mutating except for its optional JSON output. It
parses and validates the scenario; verifies declared derived-report hashes; and
when schema `0.2` declares an OSM attestation, rechecks the local locked XML,
observation, and projection before reporting success. The plan lists every
assumption and source, the canonical scenario hash, the trace cadence, all
authored structure counts, the declared agent count, state/capacity changes,
and information interventions. Review it before treating a best guess as a
chosen input or invoking the runtime. It does not create an artifact or produce
an outcome.

The output directory must be empty. It contains:

- `manifest.json` — canonical snapshot of the declared contract;
- `scenario.chy` — exact authored source snapshot;
- `run.json` — independently bundle-hash-verifiable runtime artifact;
- `source-reports/SOURCE.json` — exact derived-report snapshots, when declared;
- `source-attestations/SOURCE/{catalog,observation}.json` — OSM catalog and
  source-observation snapshots, when a schema `0.2` OSM attestation is declared;
- `report.json` — hashes, the bundle/scenario linkage, source-snapshot paths,
  both author/product claim boundaries, and a directly reconstructed mirror of
  the run's exact runtime metrics. The current report schema is `0.2`; its
  metric mirror includes agent and evacuation counts, final-exit and terminal
  state attribution, intervention delivery counts, timing fields, and queue
  exposure plus discrete wait/peak telemetry. These are deterministic
  observations from this one configured run,
  not estimates, uncertainty measures, or real-world outcomes.

`experiment verify` rejects unexpected files, altered source-report snapshots
or source attestations, a changed scenario/run pairing, a bundle hash failure,
a trace-frequency mismatch, or a report (including its mirrored metrics) that
does not exactly reconstruct.
It reruns the deterministic reference runtime from the scenario snapshot and
rejects even a self-hashed run bundle that differs from that reconstruction.
Existing `0.1` reports without the metric mirror remain verifiable for audit.
It does not validate a public source, prove the scenario represents a facility,
or elevate the run beyond its uncalibrated boundary.

Use `sensitivity` in addition when material inputs have multiple plausible
values. The experiment artifact is the single-scenario audit trail; sensitivity
is the explicit alternative-analysis workflow.
