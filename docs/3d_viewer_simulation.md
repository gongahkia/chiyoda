# Browser-Side Simulation: Design Spike

Status: **DESIGN ONLY — no implementation has been started.**

This document evaluates whether to port Chiyoda's runtime loop to the
browser-side 3D viewer so users can iterate on edited scenarios without
falling back to the Python CLI. The decision (`stop` vs `go`) lives at the
bottom; do not start any JS implementation until that line is flipped to
`go` in a separate PR.

## Motivation

Today the viewer is read-only: it replays an exported run. After editing a
scenario in author mode the user must:

1. Export the edited YAML.
2. Re-run `chiyoda run` in a shell.
3. Reload the exported viewer.

This breaks the design-feedback loop for crowd-flow and intervention
authoring.

## Scope

The browser sim would cover *interactive iteration*, not benchmark or paper
runs. Reference and reproducibility runs stay in Python.

## JS Port Surface

Estimated minimum surface to reach a useful interactive loop:

| Subsystem | File(s) | Browser parity needed |
|:--|:--|:--|
| Cell graph and 4-neighbor walkability | `chiyoda/environment/layout.py` | Yes |
| Pathfinding (belief-weighted A*) | `chiyoda/navigation/pathfinding.py` | Yes (single-floor first) |
| Spatial index | `chiyoda/navigation/spatial_index.py` | Yes |
| Social-force step | `chiyoda/navigation/social_force.py` | Yes (smaller crowds, ~1-2k agents) |
| Belief updates and gossip | `chiyoda/information/field.py`, `propagation.py` | Yes (vectorize in JS) |
| Bottleneck zone detection | `chiyoda/analysis/telemetry.py` | Optional (display only) |
| Hazards: GAS/SMOKE/FIRE/CRUSH/SHOOTER | `chiyoda/environment/hazards.py` | Yes (stylized only) |
| Multi-floor connectors | `chiyoda/navigation/connectors.py` | Optional, phase 2 |
| Wildfire ember field, flood inundation, aftershocks | `chiyoda/environment/hazards.py` | Out of scope for v1 |

## Stays in Python

- LLM provider calls (`chiyoda/information/llm.py`): API surface, budget,
  cache replay, audit table.
- Study export (`chiyoda/studies/runner.py`, `chiyoda/studies/benchmark.py`):
  parquet/CSV writing, manifest hashing, leaderboard append.
- External validation (Wuppertal bottleneck, route-choice calibration).
- Reproducibility manifests, seed reporting, benchmark submission.

The browser is an interactive sandbox; canonical results come from Python.

## Open Questions

1. Determinism: do we promise bit-identical browser-vs-Python output for a
   given seed? Probably not — JS floating point and RNG would diverge.
   Better answer: the browser loop reports its own seed and is not used for
   submissions.
2. Crowd budget: target 1-2k agents at 30 fps on a mid-range laptop, or aim
   higher with a WebGPU compute pass? Recommend starting CPU-only.
3. State transfer: edited scenario can already be exported as strict
   `layout.floors`. Reuse this as the JS input — no new schema.
4. LLM in browser: out of scope. The browser loop runs without
   intervention policies that require Python.

## Recommended Decision Process

1. Land this design doc.
2. Open a tracking issue with a 2-week timeboxed prototype targeting:
   - load a strict scenario YAML in JS,
   - run social-force + path-finding for 500 agents,
   - render hazards and beliefs in the existing 3D viewer.
3. Re-evaluate at the end of the timebox. If the prototype shows useful
   interactive frame rates and acceptable behavioral drift from Python, flip
   the decision below to `go`.

## Decision

`stop` — do not start implementation. Reopen this section after a
timeboxed prototype shows that the cost of duplicating six subsystems is
justified.
