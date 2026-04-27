# TODO

This file records the current research sequence for Chiyoda. Keep the baseline
study and paper stable before starting extension work.

## Active baseline work

1. Finish the current paper draft around the deterministic safety-control result.
   - Treat information interventions as safety-control actions, not only
     entropy-reduction mechanisms.
   - Keep the empirical claims tied to the available study artifacts:
     - Information-safety efficiency.
     - Hazard-convergence index.
     - Broad reach versus useful safety effect.
     - Cases where entropy or accuracy gains can worsen exposure.
   - Do a final narrative and reviewer-risk pass before submission.

2. Run core verification before treating the baseline package as paper-ready.
   - Run the Python test suite.
   - Run the paper smoke build.
   - Confirm reproduction commands and artifact paths in `paper/REPRODUCIBILITY.md`.
   - Re-run these checks after the next paper edits, even though the latest
     local checks passed.

## Active LLM paper decision

The LLM extension is now strong enough to support a bounded discussion, not a
replacement for the deterministic safety-control evidence. Remaining decision:

1. Decide whether LLM results belong in the main paper as an extension section
   or should be held for a follow-on paper after robustness is complete.
2. Run prompt-objective ablations before editing the paper. The key question is
   whether safety, anti-convergence, hazard-avoidance, and urgency prompts
   produce materially different ISE/HCI under fixed target selection, validator,
   cadence, radius, budget, provider, and model.
3. Run the budget-equivalence sweep only after prompt-objective ablations. This
   tests whether LLM guidance still helps when its intervention budget is no
   longer much smaller than deterministic baselines.

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

## Remaining confirmed gaps

1. Paper hardening and external-validity limits.
   - Continue tightening methods, statistical interpretation, related work,
     and reviewer-facing limitations.
   - The simulator still needs external validation against richer geometries,
     calibrated population behavior, hazard models, drills, VR traces, or
     incident records before any operational-readiness claim.

2. Full paper build and release-readiness check.
   - Smoke builds pass, but the ACM-style `make paper` build should be run
     before release if the local environment has `acmart.cls` and `latexmk`.
   - Confirm the final PDF table placement after adding the LLM medium,
     target-selection, and regime robustness tables.
