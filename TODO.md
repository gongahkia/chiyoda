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
   - Status: the robustness summary is integrated, but the paper still needs a
     final narrative and reviewer-risk pass before submission.

2. Run core verification before treating the baseline package as paper-ready.
   - Run the Python test suite.
   - Run the paper smoke build.
   - Confirm reproduction commands and artifact paths in `paper/REPRODUCIBILITY.md`.
   - Status: `PYTHONPATH=. pytest -q` passes and the paper smoke build succeeds
     against `out/information_control_50`.

## Active LLM study work

The first pass of the LLM extension is now complete enough to support a
bounded extension discussion, not a replacement for the core deterministic
evidence:

1. Completed: tiny OpenAI cache pilot and replay verification.
   - Command:

     ```bash
     PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py scenarios/study_llm_openai_pilot.yaml -o out/llm_openai_pilot
     ```

   - Then summarize:

     ```bash
     PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py out/llm_openai_pilot
     ```
   - Result: 8 live OpenAI events, 8 accepted messages, 0 fallbacks, 0
     congested recommendations, and replay reproduced all 8 cached messages.

2. Completed: inspected the tiny pilot artifacts.
   - Check cache hit/miss counts.
   - Check accepted/rejected message rates.
   - Check rejection reasons.
   - Confirm replay-only variants use cached records rather than live calls.

3. Completed: medium LLM study after the tiny pilot passed.
   - Command:

     ```bash
     PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py scenarios/study_llm_medium.yaml -o out/llm_medium
     ```

   - This design contains deterministic baselines, prompt ablations,
     validator-profile ablations, and replay verification.
   - Result: 80 runs completed across 8 variants and 10 seeds. OpenAI
     safety-strict and replay safety-strict matched on aggregate outcomes,
     with ISE 0.0491 and HCI 7.84.

4. Completed first paper integration.
   - Current interpretation: validated LLM guidance improves ISE under a much
     smaller intervention budget, but it does not yet reduce harmful
     convergence relative to conservative deterministic baselines.
   - This should be reported as safety-control evidence, not language-quality
     evidence.

Next LLM work:

1. Add a robustness extension for LLM guidance across hazard severity and
   population familiarity, probably smaller than the 900-run deterministic grid
   unless the paper needs a full factorial extension.
   - Status: completed the focused 90-run OpenAI/replay extension in
     `out/llm_regime_robustness` and integrated the high-level result into the
     limitations section. Replay exactly matches live aggregate outcomes; HCI
     still rises sharply under high hazard severity.
2. Add target-selection ablations so LLM text quality is separated from who
   receives messages.
   - Status: completed the 90-run template target-selection ablation in
     `out/llm_target_selection_ablation` and integrated the high-level result
     into the limitations section. Target selection materially changes ISE:
     bottleneck, entropy, density, and static targeting are much stronger than
     global or exposure targeting under the same generated-message budget.
3. Add a table-generation script for `paper/sections/limitations.tex` so the
   medium LLM table can be regenerated directly from
   `out/llm_medium/tables/llm_policy_comparison.csv`.
4. Decide whether LLM results belong in the main paper as an extension section
   or should be held for a follow-on paper after robustness is complete.
5. Run a fresh opt-in live OpenAI validation pass with the intended
   organization/project API key if the LLM extension is going to be claimed
   beyond cached artifacts. The deterministic baseline package must continue
   to pass without any live API key.
   - Status: completed for `scenarios/study_llm_regime_robustness.yaml`.
6. Reconcile LLM documentation so `README.md`, `paper/REPRODUCIBILITY.md`,
   `paper/sections/limitations.tex`, and this TODO agree on which LLM results
   are completed, which are cached/replayable, and which are future work.
   - Status: updated after the regime robustness and target-selection runs.

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
     adapter and prompt-style ablations implemented.

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
   - Status: tiny OpenAI/replay and medium LLM studies have been executed and
     summarized. The remaining gap is not first-pass comparison, but robustness
     across regimes and target-selection ablations that separate message
     generation from recipient choice.

5. Frame the paper extension carefully.
   - Strong framing: "LLM evacuation guidance must be evaluated as safety
     control."
   - Weak framing to avoid: "LLMs improve evacuation because messages sound
     more natural."
   - Decide whether the current bounded extension belongs in the main paper or
     should be held for a follow-on paper after robustness is complete.
   - Status: method, implementation, and limitations sections describe the LLM
     extension as optional, bounded, cached, validated, and empirically
     preliminary.

## Remaining confirmed gaps

1. LLM robustness extension across hazard severity and population familiarity.
   - Needed because the medium LLM result is currently a single scenario
     extension, while the deterministic baseline has a 900-run regime grid.
   - The next study should test whether LLM guidance retains high ISE and
     whether it can reduce HCI under low/mixed/high familiarity and
     low/medium/high hazard regimes.

2. LLM target-selection ablations.
   - Needed because the medium LLM result is confounded by a much smaller
     intervention budget and recipient count than the deterministic baselines.
   - Hold provider, prompt style, validator profile, message radius, cadence,
     and budget fixed while varying who receives messages: entropy, density,
     exposure, bottleneck, static-beacon, and global target selection.

3. Regenerable LLM table integration.
   - `paper/sections/limitations.tex` currently contains a hand-maintained
     medium LLM table plus hand-maintained regime and target-selection tables.
   - Add a table-generation script or extend `paper/scripts/gen_stats.py` so
     the tables can be regenerated from:
     - `out/llm_medium/tables/llm_policy_comparison.csv`
     - `out/llm_regime_robustness/tables/llm_policy_comparison.csv`
     - `out/llm_target_selection_ablation/tables/llm_policy_comparison.csv`

4. Fresh live OpenAI verification.
   - Existing cached artifacts show prior OpenAI pilots and replay coverage.
   - Before strengthening any LLM claim, run the intended live OpenAI
     smoke/medium or robustness subset with the actual organization/project
     API key, then verify cache hits, misses, validation reasons, fallback
     counts, replay identity, and cost/log metadata.

5. Paper hardening and external-validity limits.
   - Continue tightening methods, statistical interpretation, related work,
     and reviewer-facing limitations.
   - The simulator still needs external validation against richer geometries,
     calibrated population behavior, hazard models, drills, VR traces, or
     incident records before any operational-readiness claim.

6. Full paper build and release-readiness check.
   - Smoke builds pass, but the ACM-style `make paper` build should be run
     before release if the local environment has `acmart.cls` and `latexmk`.
   - Confirm the final PDF table placement after adding the LLM medium,
     target-selection, and regime robustness tables.

7. Direct deterministic-versus-LLM regime comparison.
   - Add a compact summary that joins
     `out/regime_robustness_900/tables/regime_summary.csv` with
     `out/llm_regime_robustness/tables/llm_policy_comparison.csv`.
   - Use it to state exactly where LLM guidance beats or trails static beacon,
     global broadcast, entropy targeting, and bottleneck avoidance by ISE and
     HCI.
