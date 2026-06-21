# Bounded LLM Agent Decisions

Chiyoda can run an experimental bounded decision layer for simulated agents.
This is separate from generated evacuation messages.

Enable it in a scenario:

```yaml
llm_decisions:
  enabled: true
  provider: "template"      # template, replay, or openai
  model: "template"
  cache_path: "out/llm_decision_cache/template"
  cache_mode: "cache_first" # cache_first or replay_only
  store_cache: true
  interval_steps: 20
  agent_budget_per_interval: 4
  objective: "bounded_agent_decision"
  prompt_style: "bounded"
  validator_profile: "standard"
```

The generated decision can only select bounded fields:

- `intent`: `EVACUATE`, `EXPLORE`, or `FOLLOW`
- `target_exit`: `null` or one exit already known to that agent
- `trust_delta`: bounded trust/rationality adjustment
- `avoid_congested`
- `rationale`
- `confidence`
- `abstain`

Validation rejects invented exits, unsupported intents, unsafe trust deltas,
low confidence, empty rationales, and congested targets when congestion
avoidance is requested. Rejected decisions are recorded but not applied.

OpenAI mode requires `cache_path` so live decisions can be replayed. Report
runs should use `provider: "replay"` against inspected cache artifacts.

Decision telemetry exports to `tables/llm_decisions.*`.
