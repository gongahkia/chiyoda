# Population Calibration Examples

Chiyoda exposes behavior and cohort parameters so sensitivity studies can make
population assumptions explicit. The examples in
`scenarios/study_population_calibration_examples.yaml` are heuristic stress
variants only. They are not calibrated to a real trajectory, evacuation drill,
VR study, incident record, or expert-coded station observation.

## What the Example Study Covers

The example study keeps the base station and hazard scenario fixed and varies
population and behavior assumptions:

- `documented_baseline`: the current commuter/visitor split with every exposed
  cohort and behavior knob stated explicitly.
- `regular_heavy_calm`: more regular commuters, higher familiarity and
  credibility, and lower anxiety transition weights.
- `visitor_heavy_uncertain`: more grouped visitors, lower familiarity and
  credibility, and stronger entropy-driven anxiety.
- `limited_visibility_stress`: lower vision and speed with stronger hazard,
  density, and neighbor-driven panic transitions.

Run it as a smoke or sensitivity bundle with:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py scenarios/study_population_calibration_examples.yaml -o out/population_calibration_examples --no-figures
```

## Parameter Provenance

These values are intentionally local and auditable:

| Parameter | Meaning | Example provenance |
| --- | --- | --- |
| `base_speed` / `base_speed_mps` | Desired walking speed before crowd, hazard, and state effects. | Local scenario assumption around Chiyoda's existing default of `1.34`; slower visitor and visibility-stress variants are heuristic slowdowns. |
| `base_rationality` | Initial rational route-choice tendency before impairment and behavioral state updates. | Local cohort tier: regular/calm cohorts higher, unfamiliar/stressed cohorts lower. |
| `familiarity` | Initial probability of knowing station exits and layout cues. | Local cohort tier: regular commuters higher, visitors lower. |
| `credibility` | Weight carried by information shared through gossip or responder-like interactions. | Local trust/noise assumption: familiar regulars higher, uncertain visitors lower. |
| `gossip_radius` | Local communication radius in layout units. | Local crowd-contact assumption: grouped or stressed variants use shorter radii. |
| `base_vision_radius` | Baseline observation radius before visibility and impairment effects. | Local visibility assumption: ordinary variants stay near the scenario observation radius, stress variants reduce it. |
| `density_panic_weight` | Contribution of nearby crowding to panic probability. | Heuristic stress dial; not fitted. |
| `neighbor_panic_weight` | Contribution of nearby panicked agents to panic probability. | Heuristic contagion dial; not fitted. |
| `hazard_panic_weight` | Contribution of current hazard load to panic probability. | Heuristic hazard-response dial; not fitted. |
| `entropy_anxiety_weight` | Contribution of belief uncertainty to anxiety and following behavior. | Central ITED sensitivity dial; not fitted. |
| `freeze_probability` | Per-step chance that a panicked agent freezes. | Heuristic stress dial; not fitted. |
| `calm_recovery_rate` | Per-step de-escalation probability. | Heuristic recovery dial; not fitted. |
| `helping_threshold` | Impairment threshold at which nearby agents may help. | Heuristic social-response dial; not fitted. |

## Guardrails

Use these examples to exercise the model, not to claim a measured population:

- Keep labels such as "regular", "visitor", or "low visibility" tied to
  scenario assumptions, not demographics.
- Do not reuse these values as evidence of real station behavior without a
  matched external reference.
- When a real reference is added, record source, license, timestamp, station,
  scenario assumptions, missing data, and the exact mapping from reference
  observations to Chiyoda parameters.
- Do not let generated or API-proposed calibration silently overwrite
  hand-audited or measured scenario parameters.
