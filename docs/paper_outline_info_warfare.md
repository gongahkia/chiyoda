# Paper Outline: Info-Warfare In Info-Aware Evacuation

## Working Title

Information warfare in evacuation dynamics: benchmarked safety effects of hostile and corrective messaging.

## Core Claim

Emergency communication is a control input, not a uniformly positive signal. In Chiyoda, a message can reduce belief entropy while increasing harmful convergence, queue pressure, or hazard exposure. The paper should frame this as an information-safety tradeoff under hostile-channel pressure.

## Abstract Outline

- Problem: evacuation simulators usually model hazards and motion more explicitly than adversarial or unreliable information.
- Method: introduce ITED, where agents hold probabilistic exit/hazard beliefs, exchange gossip, observe hazards, receive interventions, and may be targeted by hostile channels.
- Benchmark: evaluate policies on Benchmark v1 across `transit_cbrn`, `transit_shooter`, and `transit_mixed` using travel time, exposure, equity, and induced harmful convergence.
- Result: current smoke baseline `44136fa355b3678a` scores `67.82990139171999` over six runs. Mixed misinformation scenarios expose belief persistence and non-evacuation failure modes even when physical exposure is low.
- Contribution: replayable LLM/control hooks, cache audit tables, hostile-channel provenance, benchmark artifacts, and scenario-level reproducibility manifests.

## Introduction

1. Evacuation guidance is often evaluated by egress time or compliance.
2. In coupled physical-information systems, guidance can synchronize agents into bottlenecks or unsafe routes.
3. Hostile channels make this problem adversarial: decoy exits, responder spoofing, panic induction, and gossip poisoning can change route belief without changing ground truth.
4. Chiyoda contributes a small, replayable benchmark layer for evaluating communication under hazard pressure and misinformation.

## Related Work Anchors

- Route-choice priors: Scientific Data 2025 route-choice and web-interaction dataset, DOI `10.1038/s41597-025-04440-y`.
- Social homophily priors: Humanities and Social Sciences Communications 2026 Marshall Fire study, DOI `10.1057/s41599-026-07237-5`.
- Movement/fire-drill calibration: NIST TN 1839 stair movement data.
- Hazard-field boundary: NIST FDS/CFAST are external high-fidelity references; Chiyoda imports or approximates fields rather than replacing CFD.

## Method

- Runtime: strict 3D `layout.floors`, floor-aware connectors, social-force movement, belief-weighted A* routing.
- Beliefs: per-agent exit and hazard belief vectors with entropy, freshness, credibility, and source provenance.
- Information flow: observation, gossip, signage/beacons, responder relay, adaptive interventions, hostile channels.
- LLM layer: bounded generated messages and agent decisions with cache keys, validation, provider abstraction, budget guard, and `llm_calls` study export.
- Adversary: hostile-channel objectives `decoy-exit`, `panic-induce`, `responder-spoof`, and `gossip-poison`.

## Benchmark Introduction

Benchmark v1 uses three scenarios and two seeds:

| Scenario | Stressor |
|:--|:--|
| `transit_cbrn` | compact gas-release evacuation |
| `transit_shooter` | multi-floor active-shooter evacuation |
| `transit_mixed` | smoke plus hostile misinformation |

Composite score:

```text
100 * (0.35 * egress + 0.30 * exposure + 0.20 * equity + 0.15 * hci)
```

Only `interventions`, `information`, `behavior`, and `hostile_channels` are policy knobs.

## Current Key Results

Verified local smoke run: `out/benchmark_submission_smoke`.

| Item | Value |
|:--|:--|
| policy hash | `44136fa355b3678a` |
| config hash | `b8445756e90417c1` |
| seeds | `42`, `137` |
| run count | `6` |
| mean score | `67.82990139171999` |

Observed scenario scores:

| Scenario | Seed 42 | Seed 137 |
|:--|--:|--:|
| `transit_cbrn` | `62.549829849295115` | `61.03448275862071` |
| `transit_shooter` | `49.74693505632869` | `33.64816068607539` |
| `transit_mixed` | `99.99999999999999` | `99.99999999999999` |

Interpretation for draft: these are smoke-scale benchmark outputs, not external validation claims. They mainly verify scoring, telemetry, hostile-channel event capture, and reproducibility paths.

## Experiments

- Baseline policy: no intervention.
- Information-control policies: static, global, responder relay, entropy-targeted, density-aware, exposure-aware, bottleneck avoidance.
- Hostile-channel sweeps: vary attacker objective and budget.
- LLMSelective sweeps: template, local replay, OpenAI, Anthropic, budget-guarded miss, responder coordination.
- Calibration sensitivity: route-choice priors and homophily/mobility priors.

## Threats To Validity

- Stylized hazards are not CFD.
- Smoke baseline scenarios are intentionally small.
- LLM outputs are proposals and require validation/replay; they are not operational advice.
- Current benchmark measures internal consistency and comparative policy behavior, not predictive station-scale evacuation accuracy.

## Paper Figures

- Architecture diagram: physical layer, belief layer, intervention layer, hostile channel, benchmark export.
- Information-safety frontier: entropy reduction vs exposure/HCI.
- Hostile-channel timeline: injected claims, recipients, credibility decay.
- Benchmark leaderboard table with manifest hash.
- Replay audit excerpt: `llm_calls` rows for cache hit, miss, and budget block.
