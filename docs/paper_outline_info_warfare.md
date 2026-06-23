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
3. Hostile channels make this problem adversarial: false protective actions, threat amplification, authority confusion, and social-proof poisoning can change route belief without changing ground truth.
4. Chiyoda contributes a small, replayable benchmark layer for evaluating communication under hazard pressure and misinformation.

## Related Work Anchors

- Route-choice priors: Scientific Data 2025 route-choice and web-interaction dataset, DOI `10.1038/s41597-025-04440-y`.
- Social homophily priors: Humanities and Social Sciences Communications 2026 Marshall Fire study, DOI `10.1057/s41599-026-07237-5`.
- Movement/fire-drill calibration: NIST TN 1839 stair movement data.
- Hazard-field boundary: NIST FDS/CFAST are external high-fidelity references; Chiyoda imports or approximates fields rather than replacing CFD.

| System / work | Information layer | Hostile channel | Hazard model | Validation | Benchmark |
|:--|:--|:--|:--|:--|:--|
| Chiyoda / ITED (this paper) | Per-agent exit/hazard beliefs, gossip, signage, responder relay, adaptive interventions, bounded LLM proposals. | Explicit hostile-channel objectives: false protective action, threat amplification, authority confusion, social-proof poisoning. | Stylized smoke/gas/shooter/wildfire/flood/quake fields; imports external geometry/fields instead of replacing CFD. | Scenario assertions, replayable `llm_calls`, hash-chain audit, deterministic table tests, benchmark manifests. | Benchmark v1/v2/v3 with travel time, exposure, equity, and induced harmful convergence. |
| [JuPedSim](https://www.jupedsim.org/) | Route planning and wayfinding for pedestrian motion; no native misinformation/belief-provenance layer in the cited docs. | Not a first-class adversarial communication model. | Microscopic pedestrian dynamics in 2D walkable areas with built-in movement/routing models. | Established simulator with trajectory/flow analysis workflows. | Supports scenario studies, but no hostile-information benchmark comparable to ITED. |
| [Vadere](https://www.vadere.org/) | Includes a psychological layer and pedestrian-dynamics tooling. | No first-class hostile information channel. | Microscopic pedestrian/crowd dynamics with optimal-steps, gradient-navigation, and social-force models. | Open-source, tested framework with visualization and data-analysis tools. | Scenario-study framework, not an information-warfare benchmark. |
| [Ding et al. 2023/2024 bombing MAS](https://www.frontiersin.org/journals/physics/articles/10.3389/fphy.2023.1200927/full) | Agent behavior and psychological stress under attack, not message provenance or LLM-mediated information flow. | Models terrorists and attack dynamics, not misinformation channels. | AnyLogic multi-agent/social-force model for suicide-bombing/public-place attacks. | Simulation experiments report evacuation efficiency and casualties under strategy changes. | Case-specific bombing-attack study, not a reusable benchmark suite. |
| [PsychoAgent / arXiv:2505.16455](https://arxiv.org/abs/2505.16455) | Psychology-driven LLM role-playing agents for panic-emotion prediction on social media. | No physical hostile-channel simulator; focuses on panic sentiment/risk perception. | Disaster-event context without coupled pedestrian/hazard physics. | COPE panic dataset and emotion-prediction performance gains. | Social-media panic benchmark, not evacuation-policy benchmark. |
| [LLM policy-practice study / arXiv:2509.21868](https://arxiv.org/abs/2509.21868) | LLM agents model crowd movement and communication in emergency-preparedness design cycles. | Not centered on adversarial misinformation or source spoofing. | Large campus-gathering emergency scenarios with movement/communication. | Stakeholder-engaged design study; emphasizes verifiable scenarios and policy usefulness. | Policy-practice case study with 13,000 agents, not an open hostile-channel benchmark. |

## Method

- Runtime: strict 3D `layout.floors`, floor-aware connectors, social-force movement, belief-weighted A* routing.
- Beliefs: per-agent exit and hazard belief vectors with entropy, freshness, credibility, and source provenance.
- Information flow: observation, gossip, signage/beacons, responder relay, adaptive interventions, hostile channels.
- LLM layer: bounded generated messages and agent decisions with cache keys, validation, provider abstraction, budget guard, and `llm_calls` study export.
- Adversary: hostile-channel objectives `false-protective-action`, `threat-amplification`, `authority-confusion`, and `social-proof-poisoning`.

## Benchmark Introduction

Benchmark suites v1/v2/v3 share seeds `42, 137` and composite_v1 scoring.

| Suite | Scenario | Stressor |
|:--|:--|:--|
| v1 | `transit_cbrn` | compact gas-release evacuation |
| v1 | `transit_shooter` | multi-floor active-shooter evacuation |
| v1 | `transit_mixed` | smoke plus hostile misinformation |
| v2 | `wildfire_wui` | wind-driven wildland-urban interface fire with ember spotting |
| v2 | `transit_shooter` | multi-floor active-shooter evacuation |
| v3 | `flood_urban` | urban inundation with rising depth field |
| v3 | `quake_aftershock` | earthquake plus aftershock-driven re-evacuation waves |

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

| Threat | Risk | Mitigation / status |
|:--|:--|:--|
| Calibration provenance | Route-choice, social-force, population, and homophily priors may mix measured, literature-derived, and generated values. | Codebase mitigation: calibration docs and artifacts record source files, checksums, fitted parameters, and `parameter_provenance`; report-facing station cases require `metadata.station_provenance` or `metadata.provenance_file`. |
| Hazard fidelity | Stylized smoke/gas/shooter/wildfire/flood/quake fields may not match CFD, fire, hydrology, or security-event ground truth. | Codebase mitigation: imported hazard fields are supported and NIST FDS/CFAST are treated as external references. Unmitigated: benchmark hazard fields are not validated against high-fidelity event reconstructions. |
| LLM nondeterminism | Live provider outputs, model revisions, and token accounting can change across runs. | Codebase mitigation: template/replay providers, cache keys, `llm_calls` exports, budget guard rows, validator/judge reasons, provider/model cost reports, and SHA-256 hash-chain audit for replay tables. |
| Hostile-channel construct validity | Objectives such as authority confusion and social-proof poisoning are abstract operationalizations of misinformation. | Codebase mitigation: hostile events, recipients, credibility, objective, persona targeting, and belief effects are exported for audit. Unmitigated: objective weights and credibility decay are not calibrated to real adversarial messaging campaigns. |
| Benchmark scale | Smoke baseline scenarios are intentionally small. | Codebase mitigation: Benchmark v1/v2/v3 manifests record seeds, hashes, policy knobs, and scoring; results are framed as comparative internal behavior, not station-scale prediction. |
| Causal interpretation | Matched-seed deltas can be mistaken for external causal effects. | Codebase mitigation: `causal_delta.json` includes matched seeds, bootstrap intervals, leave-one-seed-out sensitivity, and assumptions documented in `docs/causal_layer_assumptions.md`. |
| Operational use | LLM outputs could be read as emergency advice. | Codebase mitigation: generated text is treated as a bounded proposal, validated before use, replay-audited, and documented as non-operational advice. |

## Paper Figures

- Architecture diagram: physical layer, belief layer, intervention layer, hostile channel, benchmark export.
- Information-safety frontier: entropy reduction vs exposure/HCI.
- Hostile-channel timeline: injected claims, recipients, credibility decay.
- Benchmark leaderboard table with manifest hash.
- Replay audit excerpt: `llm_calls` rows for cache hit, miss, and budget block.
