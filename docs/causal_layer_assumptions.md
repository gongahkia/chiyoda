# Causal comparison assumptions

`chiyoda.cli causal-compare` estimates matched-pair average treatment effects
from two exported study bundles. Matching is by `seed`, so baseline and treated
bundles must use the same seed set.

## Estimand

The implemented estimator is `ate`:

```text
ATE = mean(metric_treated(seed) - metric_baseline(seed))
```

Bootstrap confidence intervals resample matched seed-level differences.

## Required assumptions

- SUTVA: one run's treatment does not change another run's outcome.
- No interference within matched pair beyond the simulated treatment itself.
- Exchangeability across seeds after matching.
- Same scenario family and compatible metric definitions across bundles.
- Deterministic seed assignment for each run. `StudyConfig.treatment_assignments`
  can record explicit seed-to-condition labels in the exported manifest.

## Robustness output

The CLI reports leave-one-seed-out sensitivity bounds and max absolute ATE
shift. It also reports an E-value-style mean-ratio robustness score for positive
metrics. For continuous simulation metrics, this is a descriptive robustness
proxy, not a causal proof.
