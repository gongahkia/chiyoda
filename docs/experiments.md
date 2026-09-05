# Uncalibrated experiment artifacts

`chiyoda experiment run` creates one self-verifying artifact for an authored,
deterministic structural experiment. It is the normal path when a scenario is
informed by best guesses, a public layout observation, a descriptive report, or
other disclosed context but does not represent a calibrated or predictive model.

This is not a research-program gate. It is an audit trail: every serious run
records what was authored, why it was chosen, which source reports informed it,
and what the result must not be used to claim.

## Manifest

The manifest uses schema `0.1` and is strict JSON. `scenario_source` is
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

### Retaining a local OSM reference

An OSM local-coordinate reference can be retained in exactly the same way. Run
both layout verifiers before computing the report hash, then declare the report
as a source for the assumptions it informed:

```console
$ cargo run -p chiyoda -- layout verify-osm layout-catalog.json observations.json
$ cargo run -p chiyoda -- layout verify-projection layout-catalog.json \
    observations.json local-reference.json
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

`experiment run` snapshots and rechecks the local-reference byte hash, but it
does not fetch or revalidate external OSM XML. The two layout commands above
are therefore required before attaching a report; the experiment remains an
uncalibrated structural artifact even when the report verified successfully.

## Run and verify

```console
$ cargo run -p chiyoda -- experiment run experiment.json -o out/gate-experiment
$ cargo run -p chiyoda -- experiment verify out/gate-experiment
```

The output directory must be empty. It contains:

- `manifest.json` — canonical snapshot of the declared contract;
- `scenario.chy` — exact authored source snapshot;
- `run.json` — independently bundle-hash-verifiable runtime artifact;
- `source-reports/SOURCE.json` — exact derived-report snapshots, when declared;
- `report.json` — hashes, the bundle/scenario linkage, source-snapshot paths,
  both author/product claim boundaries, and a directly reconstructed mirror of
  the run's exact runtime metrics. The current report schema is `0.2`; its
  metric mirror includes agent and evacuation counts, final-exit and terminal
  state attribution, intervention delivery counts, timing fields, and queue
  counts. These are deterministic observations from this one configured run,
  not estimates, uncertainty measures, or real-world outcomes.

`experiment verify` rejects unexpected files, altered source-report snapshots,
a changed scenario/run pairing, a bundle hash failure, a trace-frequency mismatch,
or a report (including its mirrored metrics) that does not exactly reconstruct.
It reruns the deterministic reference runtime from the scenario snapshot and
rejects even a self-hashed run bundle that differs from that reconstruction.
Existing `0.1` reports without the metric mirror remain verifiable for audit.
It does not validate a public source, prove the scenario represents a facility,
or elevate the run beyond its uncalibrated boundary.

Use `sensitivity` in addition when material inputs have multiple plausible
values. The experiment artifact is the single-scenario audit trail; sensitivity
is the explicit alternative-analysis workflow.
