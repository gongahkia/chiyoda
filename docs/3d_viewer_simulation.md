# Browser-Side Simulation

Status: **implemented for a constrained single-floor preview.**

The viewer now exports `js/sim/browser_sim.js` and exposes a `Run browser sim`
toolbar control. This is an interactive preview, not the canonical simulator.
Reference, validation, and benchmark runs remain Python outputs.

## Motivation

The viewer used to be read-only: it replayed an exported run. After editing a
scenario in author mode, the user had to:

1. Export the edited YAML.
2. Re-run `chiyoda run` in a shell.
3. Reload the exported viewer.

This breaks the design-feedback loop for crowd-flow and intervention
authoring.

## Scope

The browser sim covers interactive iteration only:

- one runtime floor,
- at most 200 replay-seeded agents,
- 60 simulated seconds,
- no LLM calls,
- no multi-floor connectors,
- no hazard physics parity guarantee.

The stepper uses the exported runtime floor grid, exit cells, and first replay
frame. Agents follow a browser-computed grid distance field toward the nearest
exit and disappear when they reach an exit cell.

## Implementation Surface

Implemented:

| Subsystem | File(s) | Browser parity needed |
|:--|:--|:--|
| Browser stepper | `chiyoda/analysis/viewer_assets/js/sim/browser_sim.js` | Constrained preview only |
| Viewer data export | `chiyoda/analysis/viewer.py` | Adds `browser_sim` metadata and agent cells |
| Viewer controls | `chiyoda/analysis/viewer.py` | Run/reset browser replay |
| Regression tests | `tests/test_viewer_export.py` | Node module, speed threshold, 3-scenario egress parity |

Not implemented:

| Subsystem | Status |
|:--|:--|
| Social-force parity | Out of scope for this preview. |
| Belief updates and gossip | Out of scope. |
| Hazard field evolution | Out of scope. |
| Multi-floor connectors | T3.2/T3.3-adjacent future work. |
| Browser-generated benchmark submissions | Not supported. |

## Stays in Python

- LLM provider calls (`chiyoda/information/llm.py`): API surface, budget,
  cache replay, audit table.
- Study export (`chiyoda/studies/runner.py`, `chiyoda/studies/benchmark.py`):
  parquet/CSV writing, manifest hashing, leaderboard append.
- External validation (Wuppertal bottleneck, route-choice calibration).
- Reproducibility manifests, seed reporting, benchmark submission.

The browser is an interactive sandbox; canonical results come from Python.

## Verification

Automated checks:

- `tests/test_viewer_export.py::test_browser_sim_js_runs_exported_payload`
  runs a 60-second browser sim through Node and asserts at least 10 simulated
  steps/s.
- `tests/test_viewer_export.py::test_browser_sim_matches_cli_egress_for_three_small_scenarios`
  runs three generated single-floor Python scenarios, exports the viewer, runs
  the JS sim, and checks browser-vs-CLI egress count within 5%.

Local measurement on this development machine for the one-agent fixture:
`600` simulated steps for `60` simulated seconds, reporting about `1.6e6`
simulated steps/s. This is not a cross-machine benchmark.

## External Reference

JuPedSim is maintained by Forschungszentrum Juelich; official docs are at
<https://www.jupedsim.org/stable/>. I cannot verify a current public URL for
the specific web-based JuPedSim browser demo from accessible sources, so this
document cites only the verified official docs URL.
