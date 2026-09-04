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
  and both author/product claim boundaries.

`experiment verify` rejects unexpected files, altered source-report snapshots,
a changed scenario/run pairing, a bundle hash failure, a trace-frequency mismatch,
or a report that does not exactly reconstruct. It verifies artifact integrity;
it does not validate a public source, prove the scenario represents a facility,
or elevate the run beyond its uncalibrated boundary.

Use `sensitivity` in addition when material inputs have multiple plausible
values. The experiment artifact is the single-scenario audit trail; sensitivity
is the explicit alternative-analysis workflow.
