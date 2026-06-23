# RiMEA validation results

Source: [RiMEA Guideline 4.1.1, 2025-09-11](https://rimea.de/wp-content/uploads/2025/09/rimea-4.1.1-d-e-1.pdf). The German text is authoritative; the English text is a translation aid. RiMEA states that it does not specify fixed concrete pass/fail boundaries for the validation tests.

Scope: these scenarios are `scaled_ci_proxy` executable checks mapped to RiMEA Annex 1 tests 1-10. They are not a certification run against the full-scale geometries in the directive.

Command used:

```bash
python - <<'PY'
from chiyoda.analysis.external_validation import run_rimea_validation_scenarios, summarize_rimea_validation_runs
runs = run_rimea_validation_scenarios(seeds=(42, 43, 44, 45, 46))
print(summarize_rimea_validation_runs(runs).to_string(index=False))
PY
```

`CI95` is the two-sided 95% confidence interval half-width over seeds 42-46.

| case | expected check | observed over 5 seeds | result |
| --- | --- | --- | --- |
| 1 | single agent clears a corridor at fixed configured speed | evacuated 1/1; max evacuation 26.80s +/- 0.00; mean travel 26.80s +/- 0.00 | pass |
| 2 | single agent clears via upstairs connector without floor jump | evacuated 1/1; max evacuation 10.90s +/- 0.00; mean travel 10.90s +/- 0.00 | pass |
| 3 | single agent clears via downstairs connector without floor jump | evacuated 1/1; max evacuation 10.90s +/- 0.00; mean travel 10.90s +/- 0.00 | pass |
| 4 | corridor density sample clears and preserves configured free speed | evacuated 20/20; max evacuation 29.86s +/- 0.33; mean travel 24.82s +/- 0.27 | pass |
| 5 | premovement release steps span 100-1000 before evacuation | evacuated 10/10; max evacuation 105.90s +/- 0.00; mean travel 61.68s +/- 0.00 | pass |
| 6 | 20 agents turn left around a corner without wall/floor violations | evacuated 20/20; max evacuation 28.24s +/- 0.23; mean travel 21.39s +/- 0.14 | pass |
| 7 | 50-agent adult speed distribution matches configured cohorts | evacuated 50/50; max evacuation 25.08s +/- 1.31; mean travel 11.45s +/- 0.64 | pass |
| 8 | multi-floor parameter variation clears via configured stairs | evacuated 12/12; max evacuation 25.34s +/- 1.09; mean travel 18.26s +/- 0.06 | pass |
| 9 | public-room crowd uses all four exits | evacuated 40/40; max evacuation 5.70s +/- 0.00; mean travel 5.70s +/- 0.00 | pass |
| 10 | assigned cohorts use their corresponding exits | evacuated 8/8; max evacuation 4.84s +/- 0.16; mean travel 3.18s +/- 0.14 | pass |

Runtime assertions live in each `scenarios/validation_rimea_*.yaml` file and are evaluated by:

```bash
python -m chiyoda.cli assert-scenario scenarios/validation_rimea_01.yaml
```

CI runs cases 1, 4, 6, and 7 on pull requests via `.github/workflows/rimea-validation.yml`.
