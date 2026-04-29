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

Run the calibration sweep:

```sh
.venv/bin/python scripts/sweep_wuppertal_bottleneck_calibration.py \
  -o out/validation_wuppertal_bottleneck_calibration
```

The sweep varies bottleneck/exit width, free walking speed, density slowdown,
and social-force repulsion settings. It writes:

- `calibration_sweep_results.csv`
- `best_bottleneck_flow_summary.csv`
- `best_bottleneck_flow_comparison.csv`
- `best_candidate_parameters.json`
- per-candidate `bottleneck_flow_summary.csv` and
  `bottleneck_flow_comparison.csv` files under `candidates/`

Current best candidate:

| Parameter | Value |
|:---|:---|
| Candidate | `w7__v1p60__dens0p60__baseline_sfm` |
| Bottleneck/exit width | 7 grid cells |
| Base speed | 1.60 m/s |
| Density slowdown scale | 0.60 |
| Social-force agent repulsion | baseline, `A_AGENT=2.1`, `B_AGENT=0.3` |

Current best comparison:

| Metric | Chiyoda | Wuppertal reference | Delta |
|:---|---:|---:|---:|
| Crossing count | 49 | 75 | -34.7% |
| Mean flow | 0.675 ped/s | 1.163 ped/s | -42.0% |
| Mean time headway | 1.513 s | 0.871 s | +73.6% |

## Interpretation

The comparison is intentionally diagnostic. It validates the ingestion and
measurement pipeline and gives an external bottleneck-flow target. It does not
validate station evacuation predictions, CBRN hazard physics, or generated
message policies.

The current Chiyoda proxy is expected to differ from the laboratory reference
because it uses a grid-scale bottleneck, simplified social-force dynamics, and
no calibrated experiment-specific parameters. If the proxy underestimates
flow, that is a useful calibration gap rather than a failure to hide.

The calibration sweep reduces the original single-exit proxy gap but does not
meet the conservative match thresholds used by the script. The paper should
therefore describe this artifact as a diagnostic bottleneck-flow gap, not as
calibrated pedestrian behavior.
