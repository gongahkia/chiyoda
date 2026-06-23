# Selective LLM Controls

Chiyoda treats LLM output as a bounded proposal layer. Generated content is cache-keyed, validated, and exported for replay checks.

## Providers

Supported provider values:

- `template`: deterministic local generator.
- `replay` / `local_replay`: cache-only local replay.
- `openai`: OpenAI Responses API.
- `anthropic`: Anthropic Messages API.

OpenAI uses `OPENAI_API_KEY` and `OPENAI_MODEL`. Anthropic uses `ANTHROPIC_API_KEY` and `ANTHROPIC_MODEL`.

## Budget Guard

Live cache misses are checked before generation. Cache hits do not consume budget.

Intervention keys:

- `llm_max_calls_per_run`
- `llm_max_estimated_tokens_per_run`
- `llm_max_estimated_usd_per_run`
- `llm_input_usd_per_mtok`
- `llm_output_usd_per_mtok`

Agent decision and population-calibration configs use the same keys without the `llm_` prefix. Cost estimates are active only when per-MTok prices are supplied.

If a guard blocks a call, the provider is recorded as `budget_guard`, cache status is `budget_exceeded`, and deterministic fallback behavior is used where the runtime requires an action.

## Persona Population Calibration

`generated_population_calibration.personas` or `persona_conditions` can request bounded generated cohorts:

```yaml
generated_population_calibration:
  enabled: true
  provider: template
  cache_path: out/cache/population
  allowed_targets: [cohort_mix, parameter_priors, scenario_metadata]
  personas:
    - regular wheelchair
    - visitor family
```

Generated cohorts preserve exact `persona_condition` labels, exact total count, bounded numeric fields, and replayable cache keys.

## Responder Coordination

`policy: llm_responder_coordination` ranks active responders by nearby entropy, density, and hazard load, then asks the configured generator for bounded broadcast guidance from those responder targets.

## LLM-MAS Attack Coverage

Chiyoda treats multi-agent LLM communication as an untrusted message surface.
Agent-in-the-Middle attacks intercept and manipulate inter-agent messages
without compromising each agent directly
([arXiv:2502.14847](https://arxiv.org/abs/2502.14847)). Multi-round stealthy
tampering attacks similarly target message content while trying to preserve
semantic similarity ([arXiv:2508.03125](https://arxiv.org/abs/2508.03125)).
The TrustAgent survey frames agent and multi-agent trustworthiness as spanning
internal modules and external interaction surfaces
([arXiv:2503.09648](https://arxiv.org/abs/2503.09648)).

The bounded validator rejects generated messages that contain intercepted-message
markers, instruction override markers, source spoofing, or coercive persuasion
markers such as unverifiable social-proof claims. These rejections apply to
normal intervention generations and to hostile-channel red-team generations
before a generated hostile claim can be used.

## Replay Audit

Study bundles include `tables/llm_calls.*` with one row per generated/replayed/blocked LLM call across:

- `population_calibration`
- `intervention`
- `agent_decision`

Rows include provider/model, cache key/status, validation status/reasons, fallback flag, prompt style, estimated tokens/USD, budget reason, and raw usage tokens when a provider response supplies them.
