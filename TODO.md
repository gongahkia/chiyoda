# TODO

This file records the current research sequence for Chiyoda. Keep the baseline
study and paper stable before starting extension work.

## Active baseline work

1. Finish the current paper draft around the deterministic safety-control result.
   - Treat information interventions as safety-control actions, not only
     entropy-reduction mechanisms.
   - Keep the empirical claims tied to the completed studies:
     - Information-safety efficiency.
     - Hazard-convergence index.
     - Broad reach versus useful safety effect.
     - Cases where entropy or accuracy gains can worsen exposure.
   - Update the LaTeX sections after the robustness summary is available.

2. Run core verification before treating the baseline package as paper-ready.
   - Run the Python test suite.
   - Run the paper smoke build.
   - Confirm reproduction commands and artifact paths in `paper/REPRODUCIBILITY.md`.

## Completed baseline work

- Completed the full 900-run regime robustness study:
  3 hazard regimes, 3 familiarity regimes, 5 policies, and 20 seeds per
  condition.
- Summarized the robustness grid with
  `scripts/summarize_regime_robustness.py`.
- Integrated the completed robustness result into the LaTeX paper, including
  the regime summary table and heatmap.

## Deferred LLM extension

Do not start this track until the baseline robustness run is complete and the
current paper has a full first-pass draft.

Research value proposition:

- Study LLM-generated evacuation messaging as a safety-control actor.
- Evaluate LLM messages by downstream safety outcomes, not by fluency,
  plausibility, or information accuracy alone.
- Test whether richer adaptive language improves evacuation safety or merely
  increases harmful convergence and herd behavior.

Possible implementation path:

1. Add a provider-neutral LLM message-generation interface.
   - Support cached API calls for providers such as OpenAI or Anthropic.
   - Require deterministic replay from cache for paper runs.
   - Store prompts, model metadata, response text, validation status, and cache
     keys as artifacts.

2. Add a simulator-state-to-message prompt layer.
   - Inputs should include hazard state, congestion, exits, policy budget, and
     local population context.
   - Outputs should be structured messages with target scope, routing intent,
     and confidence or abstention fields.

3. Add safety validators before any generated message affects agents.
   - Reject invented exits, impossible routes, stale hazard claims, and
     instructions that over-concentrate agents into dangerous bottlenecks.
   - Include a safe fallback policy when validation fails.

4. Compare LLM-mediated policies against the current deterministic baselines.
   - Keep the same ISE and HCI metrics.
   - Compare against `static_beacon`, `global_broadcast`, `entropy_targeted`,
     and `bottleneck_avoidance`.
   - Separate language value from extra information access by controlling the
     simulator state exposed to each policy.

5. Frame the paper extension carefully.
   - Strong framing: "LLM evacuation guidance must be evaluated as safety
     control."
   - Weak framing to avoid: "LLMs improve evacuation because messages sound
     more natural."
   - Decide after the first full paper draft whether this belongs in the same
     paper as a controlled extension or in a follow-on paper.
