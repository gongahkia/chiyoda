# Benchmark v1

Benchmark v1 contains six canonical scenarios:

- `transit_cbrn`: compact gas-release evacuation.
- `transit_shooter`: multi-floor active-shooter evacuation.
- `transit_mixed`: smoke plus hostile misinformation.
- `large_station_multifloor`: three-level station transfer under smoke pressure.
- `open_air_event_funnel`: open-air event plaza egress through a constrained funnel.
- `mixed_indoor_outdoor_arena`: indoor foyer to outdoor concourse egress with hostile misinformation.

The composite score is `composite_v1`:

```text
100 * (0.35 * egress + 0.30 * exposure + 0.20 * equity + 0.15 * hci)
```

Where:

- `egress = 1 / (1 + mean_travel_time_s)`.
- `exposure = 1 / (1 + p95_hazard_exposure)`.
- `equity = 1 / (1 + equity_time_gap_s)`.
- `hci = 1 / (1 + harmful_convergence_index_induced)`.

## Equity Subgroups

Study bundles include an `equity_subgroups` table with run-level breakdowns for
the equity component. Each row reports subgroup evacuation rate, travel-time
gap versus the run mean, exposure, final impairment, and familiarity.

Subgroup tags are internal model tags:

- `impaired`: final physiological impairment is at least `0.1`.
- `not_impaired`: final physiological impairment is below `0.1`.
- `elderly`: `age_band` is `senior`, `elderly`, `older_adult`, `older-adult`,
  `older adult`, or `65+`.
- `non_elderly`: `age_band` is present and not in the elderly set.
- `unknown_age`: no `age_band` was exported.
- `low_familiarity`: initial familiarity prior is below `0.33`.
- `medium_familiarity`: initial familiarity prior is at least `0.33` and below
  `0.67`.
- `high_familiarity`: initial familiarity prior is at least `0.67`.
- `unknown_familiarity`: no familiarity prior was exported.

## Statistical Reporting

Leaderboard JSON reports the mean composite score plus a seed-bootstrap 95% CI:

- `mean_score`
- `score_ci_low`
- `score_ci_high`
- `seeds_used`
- `seed_count`
- `bootstrap_n`
- `tier`
- `scenario_breakdown`

`scenario_breakdown` repeats `mean_score`, `score_ci_low`, `score_ci_high`,
`seeds_used`, `seed_count`, `bootstrap_n`, and `run_count` for each scenario.

The bootstrap resamples seed-level scores with replacement, using `bootstrap_n =
1000`. Scenario-level CIs resample that scenario's seed scores. Overall CIs
resample each seed's mean score across scenarios.

## Tiers

Official submissions require at least 20 distinct seeds. Runs with fewer than
20 seeds are accepted as `smoke` tier only. The bundled benchmark specs still
use seeds `[42, 137]` so local CI remains cheap; those two-seed outputs are
therefore `smoke`, not official leaderboard results.

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
