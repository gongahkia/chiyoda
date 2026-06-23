# Perf Baselines

`scripts/perf_regression_suite.py` runs a fixed smoke set and writes CSV timing
rows with `elapsed_s`, `rss_delta_mib`, evacuation count, and step count.

The PR workflow compares the pull request against the current `main` branch:

```sh
python scripts/perf_regression_suite.py --label main --output-file out/perf/base.csv
python scripts/perf_regression_suite.py --label pr --output-file out/perf/head.csv
python scripts/perf_regression_suite.py compare \
  --baseline out/perf/base.csv \
  --current out/perf/head.csv \
  --max-regression 0.10
```

The compare step writes `perf_delta.csv` and `perf_delta.md`, uploads them as
GitHub Actions artifacts, and appends the Markdown table to the job summary.

`elapsed_s` is the gating metric. A row fails when current elapsed time is more
than `10%` slower than the base branch for the same `(scenario, seed)` pair.
`[Speculation]` The `10%` threshold is an engineering guardrail, not a
statistical performance claim.
