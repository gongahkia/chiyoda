# Generated Population Calibration

Generated population calibration is an opt-in scenario preprocessing step. It
exists to test cacheable external suggestions for population assumptions while
keeping measured or hand-authored scenario values authoritative.

The feature is intentionally conservative:

- Generated output can influence only targets listed in `allowed_targets`.
- The only supported application policy is `overwrite_policy: "missing_only"`.
- Existing cohort fields are never replaced.
- Every accepted generated field receives generated provenance.
- Every run attaches audit metadata with provider, model, cache key, cache
  status, validation status, applied targets, and skipped fields.

## Scenario Block

Add this block to a scenario or a study variant override:

```yaml
generated_population_calibration:
  enabled: true
  provider: "template"      # template, replay, or openai
  model: "template"
  cache_path: "out/population_calibration_cache/template"
  cache_mode: "cache_first" # cache_first or replay_only
  store_cache: true
  allowed_targets: ["parameter_priors", "scenario_metadata"]
  objective: "fill_missing_population_priors_without_overwriting_manual_values"
  prompt_style: "conservative"
  overwrite_policy: "missing_only"
```

Allowed targets are:

| Target | What generated output may influence |
| --- | --- |
| `cohort_mix` | Create `population.cohorts` only when the scenario has no authored cohorts. |
| `parameter_priors` | Fill missing cohort fields such as `base_speed`, `base_rationality`, `credibility`, `gossip_radius`, and `base_vision_radius`. |
| `scenario_metadata` | Add generated-calibration metadata under `metadata.generated_population`. |

Generated output may not alter hazards, responders, layout, interventions,
simulation timing, or any existing cohort value. If a live provider proposes a
disallowed target, unsupported parameter, out-of-range value, invented cohort
prior, or low-confidence result, validation rejects it and no generated values
are applied.

## Providers and Replay

`template` is deterministic and safe for tests. It writes cache records when
`cache_path` and `store_cache` are set.

`replay` requires `cache_path` and sets replay-only semantics. If the cache
does not contain the request key, it abstains and validation rejects the
proposal.

`openai` is optional live generation through the same Responses API pattern
used by generated evacuation messages. Live use should populate cache records
only in explicit pilot runs; paper-facing runs should use `provider: "replay"`
against inspected cache artifacts.

The request cache key includes the scenario name, objective, prompt style,
allowed targets, population total, existing cohort summaries, hazard count,
responder count, and metadata keys. It does not include provider name, so a
template or live cache-population run can be replayed by a replay-only variant
with the same scenario context.

## Example Study

`scenarios/study_generated_population_calibration.yaml` contains three
variants:

- `template_missing_priors`: fills missing priors for existing authored
  cohorts, without replacing their existing fields.
- `replay_missing_priors`: reuses the same request and cache path in replay
  mode.
- `template_missing_cohort_mix`: demonstrates generated cohort creation only
  after the variant explicitly clears authored cohorts and allows `cohort_mix`.

Smoke run:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py scenarios/study_generated_population_calibration.yaml -o out/generated_population_calibration --no-figures
```

Use this as plumbing evidence only. Generated population priors are still
future-work inputs until matched against trajectory, drill, VR, incident, or
expert-coded references.

## Cache Audit

Generated population calibration cache records now include the accepted or
rejected validation status, validation reasons, proposed targets, applied
targets, skipped overwrite attempts, provider/model, and token usage when the
raw provider response includes it. Run:

```sh
python3 scripts/audit_llm_cache_usage.py \
  --population-cache-root out/population_calibration_cache \
  -o out/llm_synthesis
```

The script writes:

- `generated_population_calibration_cache_records.csv`
- `generated_population_calibration_cache_usage.csv`
- `generated_population_calibration_cache_usage_totals.csv`
