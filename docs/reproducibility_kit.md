# Reproducibility Kit

## Environment Pin

Baseline local setup:

```sh
python3 -m venv .venv
.venv/bin/python -m ensurepip --upgrade
.venv/bin/python -m pip install -r requirements.txt pytest
```

Python version used for the current local verification: `3.14.6`.

Package input file: `requirements.txt`.

For fully pinned reproduction, install from the lock file instead:

```sh
.venv/bin/python -m pip install -r requirements-lock.txt
```

`requirements-lock.txt` was generated via `pip freeze` against the verified
`.venv` and pins exact versions of every transitive dependency.

## Verification Commands

```sh
.venv/bin/python -m pytest
.venv/bin/python -m chiyoda.cli benchmark submit --suite v1 -o out/benchmark_submission_smoke
python3 -m compileall -q chiyoda
```

Focused benchmark/docs gate:

```sh
.venv/bin/python -m pytest tests/test_benchmark.py
.venv/bin/python -m chiyoda.cli benchmark submit --suite v1 -o out/benchmark_submission_smoke
```

## Benchmark Seed Set

Benchmark v1 uses:

```text
42
137
```

Scenarios:

```text
transit_cbrn
transit_shooter
transit_mixed
```

## Expected Benchmark Outputs

Running the benchmark submit command writes:

```text
out/benchmark_submission_smoke/benchmark_runs.csv
out/benchmark_submission_smoke/leaderboard.json
out/benchmark_submission_smoke/reproducibility_manifest.json
docs/benchmark/index.html
```

Current local smoke manifest:

```json
{
  "suite": "v1",
  "config_hash": "b8445756e90417c1",
  "policy_hash": "44136fa355b3678a",
  "seed_set": [42, 137],
  "version": "3.0.0",
  "scenarios": ["transit_cbrn", "transit_shooter", "transit_mixed"]
}
```

Current local smoke leaderboard:

```json
{
  "suite": "v1",
  "policy_hash": "44136fa355b3678a",
  "mean_score": 67.82990139171999,
  "run_count": 6
}
```

## Hash Manifest

Current SHA-256 hashes for core benchmark artifacts:

```text
f39c6c2c3131eee6b13cc2af8a1f56812e77c7a5c8ab2027c47b4c5bf9068442  docs/benchmark/benchmark_spec_v1.json
21e9304bc09bf11a3015392c37c133318dee92e0e736b965dd7c4e90a2c19ea5  docs/benchmark/benchmark_spec_v1.schema.json
3d0bcf2f114a6bb45467cf59e112849bae94f8e4036346d46fc53fa6bd3bcd71  data/benchmark/v1/reference_trajectories.parquet
6b79684b522e4cd7411e348ab77e7a5466e3d6fb7cccb416e618173edb4a4724  scenarios/benchmark/transit_cbrn.yaml
94b930be39baa0b2931f6f0c6be2e3494964f83d7d813bdf0a50d479528143b1  scenarios/transit_shooter.yaml
98808bbd582b3795e43c6606d63e11972caa65abdfcb45f716457950aea0fb32  scenarios/benchmark/transit_mixed.yaml
```

Regenerate this block with:

```sh
shasum -a 256 \
  docs/benchmark/benchmark_spec_v1.json \
  docs/benchmark/benchmark_spec_v1.schema.json \
  data/benchmark/v1/reference_trajectories.parquet \
  scenarios/benchmark/transit_cbrn.yaml \
  scenarios/transit_shooter.yaml \
  scenarios/benchmark/transit_mixed.yaml
```

## Replay Checks

For LLM-enabled studies, inspect:

```text
tables/llm_calls.csv
tables/interventions.csv
tables/llm_decisions.csv
```

Replay validity requires stable cache keys, no unexpected `budget_exceeded` rows, and provider/model/cache status matching the intended study regime.
