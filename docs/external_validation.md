# External Bottleneck Validation

Chiyoda now includes a public pedestrian bottleneck reference used by
JuPedSim/PedPy examples:

- Reference: Wuppertal 2018 bottleneck experiment, `040_c_56_h-.txt`.
- Archive: Pedestrian Dynamics Data Archive, DOI `10.34735/ped.da`.
- Documentation source: JuPedSim getting-started bottleneck comparison.
- Local copy: `data/external/wuppertal_bottleneck_2018/040_c_56_h-.txt`.
- Metadata: `data/external/wuppertal_bottleneck_2018/metadata.json`.

This supports a narrow validation contribution: Chiyoda can ingest a public
PeTrack trajectory file, compute bottleneck crossing times at the same
measurement line used in the JuPedSim/PedPy example, and compare those flow
statistics to a Chiyoda bottleneck proxy run.

## Reproduce

Run the Chiyoda bottleneck proxy:

```sh
.venv/bin/python -m chiyoda.cli sweep \
  scenarios/study_validation_wuppertal_bottleneck.yaml \
  -o out/validation_wuppertal_bottleneck
```

Compare against the external reference:

```sh
.venv/bin/python scripts/validate_wuppertal_bottleneck.py \
  out/validation_wuppertal_bottleneck \
  -o out/validation_wuppertal_bottleneck_external
```

The script writes:

- `bottleneck_flow_summary.csv`
- `bottleneck_flow_comparison.csv`
- `reference_metadata.json`

## Interpretation

The comparison is intentionally diagnostic. It validates the ingestion and
measurement pipeline and gives an external bottleneck-flow target. It does not
validate station evacuation predictions, CBRN hazard physics, or generated
message policies.

The current Chiyoda proxy is expected to differ from the laboratory reference
because it uses a grid-scale bottleneck, simplified social-force dynamics, and
no calibrated experiment-specific parameters. If the proxy underestimates
flow, that is a useful calibration gap rather than a failure to hide.
