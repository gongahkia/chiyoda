# Paper Self-Review

This note applies the `research-paper-writing` skill's reviewer-facing
checklist to the current Chiyoda draft. It is not part of the manuscript.

## Mini Outline

1. Task: evaluate emergency communication as safety control in hazard-coupled
   evacuation.
2. Challenge: better information can improve beliefs while synchronizing crowds
   into bottlenecks or hazard exposure.
3. Method: Chiyoda couples physical evacuation, belief state, information
   propagation, intervention policies, and ISE/HCI telemetry.
4. Evidence: deterministic studies show static local messaging is the strongest
   information-safety baseline; robustness and ablation studies define where
   the claim holds.
5. Extension: validated sparse LLM guidance is efficient, but equal-budget LLM
   guidance weakens that advantage and does not solve harmful convergence.

## Claim-Evidence Map

- Claim: Emergency communication is a safety-control action.
  Evidence: 50-seed primary study, intervention ablation, message-quality
  study, ISE/HCI metrics.
  Status: supported.

- Claim: Static local messaging is the strongest conservative baseline by ISE.
  Evidence: 50-seed primary study and 900-run hazard/familiarity robustness
  grid.
  Status: supported.

- Claim: Broad reach and entropy reduction are not sufficient safety
  objectives.
  Evidence: global broadcast and entropy-targeted policies improve some
  information outcomes while retaining weaker ISE or high HCI.
  Status: supported.

- Claim: Generated guidance should be evaluated as bounded safety control, not
  as better-sounding text.
  Evidence: LLM medium, target-selection, regime robustness, prompt-objective,
  budget-equivalence, replay, and cache-audit artifacts.
  Status: supported as bounded extension.

- Claim: Chiyoda is operationally predictive for real stations.
  Evidence: none yet.
  Status: explicitly not claimed.

## Five-Dimension Reviewer Check

1. Contribution: The paper contributes a safety-control framing, coupled
   metrics, reproducible studies, and a bounded generated-guidance extension.
   Remaining risk mitigated: the introduction and implementation now foreground
   the information-control thesis and explain why the simulator is organized
   around controllable communication actions rather than generic crowd motion.

2. Writing clarity: Core terms are stable: ISE, HCI, information-safety
   efficiency, harmful convergence, generated guidance. Remaining risk:
   LLM tables are now in `evaluation.tex`, but table placement should still be
   checked in the final ACM PDF.

3. Experimental strength: The deterministic evidence is broad for a stylized
   simulator, and the LLM extension includes target, regime, prompt, budget,
   replay, and cache audits. Remaining risk: LLM HCI improvements are not
   strong; the draft frames this as a limitation rather than a win.

4. Evaluation completeness: The current package covers baselines, timing,
   budget, credibility, target selection, hazard/familiarity robustness, and
   generated-guidance ablations. Remaining risk: no real evacuation trace,
   drill, VR, or incident calibration.

5. Method design soundness: The simulator makes assumptions inspectable and
   preserves deterministic replay for live LLM calls. Remaining risk: hazard
   physics and behavior models are stylized; limitations must remain explicit.
