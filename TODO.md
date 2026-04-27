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

## Paper hardening checklist

Drill harder on these before treating the work as PhD-submission ready:

1. Full narrative pass from abstract to conclusion.
   - Make the thesis continuous: emergency communication is safety control,
     not just information spread.
   - Ensure the abstract, introduction, evaluation, limitations, and conclusion
     make the same bounded claim.

2. Methods rigor.
   - Specify the agent belief state, route choice logic, hazard exposure model,
     intervention policy mechanics, and telemetry pipeline precisely.
   - Define ISE and HCI as paper-level constructs, not only implementation
     outputs.
   - Explain why these coupled metrics are the right answer to the research
     question.

3. Statistical treatment.
   - Explain the use of seed-level aggregates, Mann--Whitney tests, descriptive
     effect sizes, and nonparametric interpretation.
   - Make clear why the paper does not overclaim operational superiority from
     stylized simulation.

4. Reviewer attack surface.
   - Address single station geometry, stylized hazard physics, uncalibrated
     population behavior, lack of real evacuation trace validation, and metric
     validity.
   - Explain why the static-beacon result is nontrivial: it wins by coupled
     information-safety efficiency, not by raw reach or raw evacuation count.

5. Related work tightening.
   - Position the work against pedestrian evacuation simulation, information
     diffusion in crowds, emergency communication systems, information-theoretic
     control, and safety-critical AI messaging.

6. Figure and table polish.
   - Keep one core result figure, one robustness figure, one claim matrix, and
     one compact policy comparison table central.
   - Avoid overwhelming the reader with raw metrics that do not support the
     thesis.

## Completed baseline work

- Completed the full 900-run regime robustness study:
  3 hazard regimes, 3 familiarity regimes, 5 policies, and 20 seeds per
  condition.
- Summarized the robustness grid with
  `scripts/summarize_regime_robustness.py`.
- Integrated the completed robustness result into the LaTeX paper, including
  the regime summary table and heatmap.

## Active LLM extension

The LLM extension should strengthen the core paper only if it is implemented as
a constrained safety-control policy. It must remain off by default and must not
change the deterministic baseline studies.

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
   - Do not require live API keys for tests or baseline reproduction.
   - Status: replay, template, and OpenAI Responses API providers implemented.

2. Add a simulator-state-to-message prompt layer.
   - Inputs should include hazard state, congestion, exits, policy budget, and
     local population context.
   - Outputs should be structured messages with target scope, routing intent,
     and confidence or abstention fields.
   - Keep state exposure controlled so language is the experimental factor, not
     hidden extra sensor access.
   - Status: structured request/message schema implemented; OpenAI prompt
     adapter implemented; provider-comparison prompts remain open.

3. Add safety validators before any generated message affects agents.
   - Reject invented exits, impossible routes, stale hazard claims, and
     instructions that over-concentrate agents into dangerous bottlenecks.
   - Include a safe fallback policy when validation fails.
   - Log rejected messages so the paper can report safety-filter behavior.
   - Status: exit, hazard, radius, credibility, abstention, conflicting-exit,
     congested-exit, vague-guidance, and confidence validation implemented.

4. Compare LLM-mediated policies against the current deterministic baselines.
   - Keep the same ISE and HCI metrics.
   - Compare against `static_beacon`, `global_broadcast`, `entropy_targeted`,
     and `bottleneck_avoidance`.
   - Separate language value from extra information access by controlling the
     simulator state exposed to each policy.
   - Status: optional deterministic pilot scenario added; full comparison has
     not been run or claimed.

5. Frame the paper extension carefully.
   - Strong framing: "LLM evacuation guidance must be evaluated as safety
     control."
   - Weak framing to avoid: "LLMs improve evacuation because messages sound
     more natural."
   - Decide after the first full paper draft whether this belongs in the same
     paper as a controlled extension or in a follow-on paper.
   - Status: method and implementation sections now describe the LLM extension
     as optional, bounded, cached, validated, and not yet a completed result.
