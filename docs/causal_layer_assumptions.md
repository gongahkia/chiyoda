# Causal comparison assumptions

`chiyoda.cli causal-compare` estimates matched-pair average treatment effects
from two exported study bundles. `chiyoda run --counterfactual` now creates a
treated bundle, a matched no-intervention bundle, and `causal_delta.json` in one
command. Matching is by `seed`, so baseline and treated bundles must use the
same seed set.

## Estimand

The implemented estimator is `ate`:

```text
ATE = mean(metric_treated(seed) - metric_baseline(seed))
```

Bootstrap confidence intervals resample matched seed-level differences.

## Required assumptions

- [Inference] SUTVA approximation: one run's treatment does not change another
  run's outcome.
- [Inference] No interference within matched pair beyond the simulated
  treatment itself.
- [Inference] Exchangeability across seeds after matching.
- [Inference] Same scenario family and compatible metric definitions across
  bundles.
- [Inference] Deterministic seed assignment for each run.
  `StudyConfig.treatment_assignments` can record explicit seed-to-condition
  labels in the exported manifest.

## Robustness output

The CLI reports leave-one-seed-out sensitivity bounds and max absolute ATE
shift. It also reports an E-value-style mean-ratio robustness score for positive
metrics. For continuous simulation metrics, this is a descriptive robustness
proxy, not a causal proof.

## Threats to validity

| Threat | Causal risk | Mitigation / status |
|:--|:--|:--|
| Calibration provenance | Matched-pair deltas inherit bias from route-choice, population, social-force, and homophily priors. | Codebase mitigation: committed calibration artifacts record source metadata and fitted parameters; generated calibration writes audit metadata and per-field `parameter_provenance`; report-facing station scenarios require station provenance metadata. |
| Hazard fidelity | Treatment effects over stylized hazards may not transport to CFD-grade smoke, fire, flood, or security-event fields. | Codebase mitigation: imported hazard fields and external-validation workflows are documented separately. Unmitigated: the causal layer does not adjust ATEs for high-fidelity hazard-model error. |
| LLM nondeterminism | Live model calls may produce different interventions under the same seed if replay is not used. | Codebase mitigation: LLM cache/replay modes, provider/model fields, token/cost report, validation reasons, and `llm_calls` hash-chain audit make replay state visible in exported bundles. |
| Hostile-channel construct validity | Estimated harm may depend on abstract attacker objectives and credibility decay rather than empirically calibrated misinformation behavior. | Codebase mitigation: hostile-channel objectives, recipients, persona targeting, and event telemetry are exported. Unmitigated: no causal adjustment is applied for misspecified hostile-channel behavior. |
| Seed exchangeability | [Inference] Matched seeds approximate exchangeable counterfactual pairs, but finite seed sets can leave residual imbalance. | Codebase mitigation: `run --counterfactual` uses matched seed sets; `causal-compare` reports bootstrap intervals and leave-one-seed-out sensitivity. |
| Metric compatibility | ATEs are invalid if baseline and treated bundles use incompatible scenario families or metric definitions. | Codebase mitigation: exported metadata records scenario names, variants, run IDs, and metric columns; this remains a user-facing precondition rather than an automatic proof. |
