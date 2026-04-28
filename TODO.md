# TODO

This file records the remaining work for the current Chiyoda paper scope. The
project is now in paper-hardening mode, not experiment-expansion mode.

## Current Status

Scoped data collection for the current paper is complete.

Completed empirical artifacts:

1. Deterministic safety-control evidence.
   - `out/information_control_50`
   - `out/intervention_ablation_30`
   - `out/message_quality_30`
   - `out/regime_robustness_900`

2. Bounded LLM guidance evidence.
   - `out/llm_medium`
   - `out/llm_target_selection_ablation`
   - `out/llm_regime_robustness`
   - `out/llm_prompt_objective_ablation`
   - `out/llm_budget_equivalence`
   - `out/llm_synthesis`

3. Reproducible paper support.
   - Generated `paper/stats.tex`
   - Generated `paper/llm_tables.tex`
   - Smoke-built `paper/main_smoke.pdf`
   - Self-review notes in `paper/SELF_REVIEW.md`

No additional code changes, live API runs, or new simulation sweeps are planned
for the current paper unless final reading finds a claim that is not supported
by the existing artifacts.

## Remaining Paper Work

1. Richer station geometries with attempted modelling of real-world stations/possible edgecase stations with drastic results

2. Various calibrated population behavior (powered by heuristic changes)

3. Various calibrated population behavior (powered by LLMs and api call to relevant APIs) 

4. Validated hazard physics,

5. Applied simulation of evacuation drills

6. Pedestrian trajectory data where relevant for the scope of the paper

7. Full narrative read from abstract to conclusion.
   - Keep the thesis continuous: emergency communication is safety control, not
     just information spread.
   - Ensure the abstract, introduction, claims, evaluation, limitations, and
     conclusion make the same bounded claim.
   - Make LLM guidance a bounded extension result, not the central paper claim.

8. Claim-evidence alignment pass.
   - Check every major claim in the abstract and introduction against the
     deterministic and LLM artifacts listed above.
   - Weaken or remove any claim that is not directly supported.
   - Preserve the limitation that Chiyoda is a stylized simulator, not an
     operational evacuation planner.

9. Methods and metric clarity.
   - Confirm the agent belief state, route-choice logic, hazard exposure model,
     intervention policy mechanics, and telemetry pipeline are precise enough
     for a reviewer to reproduce.
   - Define ISE and HCI clearly as paper-level constructs.
   - Explain why coupled belief/safety metrics answer the research question
     better than entropy, reach, or evacuation count alone.

10. Statistical interpretation.
   - Confirm the paper explains seed-level aggregates, Mann-Whitney tests,
     descriptive effect sizes, and nonparametric interpretation.
   - Avoid claiming operational superiority from stylized simulation.
   - Emphasize tradeoff patterns and bounded evidence.

11. Related work tightening.
   - Position the contribution against pedestrian evacuation simulation,
     information diffusion in crowds, emergency communication systems,
     information-theoretic control, and safety-critical AI messaging.
   - Make clear why the paper is about communication as control, not only about
     adding messages to an evacuation simulator.

12. Figure, table, and PDF polish.
   - Visually inspect the final PDF for table placement, overfull boxes, figure
     order, and caption clarity.
   - Keep the central result figures and compact policy tables prominent.
   - Avoid overwhelming the reader with raw metrics that do not support the
     thesis.

13. Final release checks.
   - Run the Python test suite.
   - Run the paper smoke build.
   - Run the full ACM-style `make paper` build if the local environment has
     `acmart.cls` and `latexmk`.
   - Confirm reproduction commands and artifact paths in
     `paper/REPRODUCIBILITY.md`.

## Not In Current Scope

External validation remains future work, not a blocker for the current paper as long as it does not purport operational readiness for actual application in a real-world setting, or attempt to cover VR traces or handle incident record generation.