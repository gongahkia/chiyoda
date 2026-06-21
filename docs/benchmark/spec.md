# Benchmark v1

Benchmark v1 contains three canonical scenarios:

- `transit_cbrn`: compact gas-release evacuation.
- `transit_shooter`: multi-floor active-shooter evacuation.
- `transit_mixed`: smoke plus hostile misinformation.

The composite score is `composite_v1`:

```text
100 * (0.35 * egress + 0.30 * exposure + 0.20 * equity + 0.15 * hci)
```

Where:

- `egress = 1 / (1 + mean_travel_time_s)`.
- `exposure = 1 / (1 + p95_hazard_exposure)`.
- `equity = 1 / (1 + equity_time_gap_s)`.
- `hci = 1 / (1 + harmful_convergence_index_induced)`.

Submission policies may only override `interventions`, `information`,
`behavior`, and `hostile_channels`.

Run:

```sh
.venv/bin/python -m chiyoda.cli benchmark submit --suite v1 --policy policy.yaml -o out/benchmark_submission
```

Outputs:

- `benchmark_runs.csv`
- `leaderboard.json`
- `reproducibility_manifest.json`

Reference trajectories live at
`data/benchmark/v1/reference_trajectories.parquet`.
