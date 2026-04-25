[![](https://img.shields.io/badge/chiyoda_3.0.0-ITED-blue)](https://github.com/gongahkia/chiyoda/releases/tag/3.0.0)

# `Chiyoda` — ITED Framework

**Information-Theoretic Evacuation Dynamics**: a computational framework for studying how heterogeneous information propagation, coupled hazard dynamics, and bounded-rational decision-making interact to shape evacuation outcomes.

Domain-agnostic at its core — models entities navigating spatial environments under propagating stimuli with asymmetric information access. Primary application: CBRN-threat evacuation in transit environments.

## Key Features

| Layer | Capability |
|:------|:-----------|
| **Information** | Shannon entropy beliefs, SIR gossip propagation, beacon broadcast, belief decay |
| **Agents** | BDI cognitive architecture, physiological impairment model, exploration/herding modes |
| **Hazards** | Multi-kind (GAS/SMOKE/FIRE/CRUSH), advection-diffusion spread, visibility reduction |
| **Navigation** | Helbing-Molnar social force model, belief-weighted A* pathfinding, counter-flow friction |
| **Responders** | Counter-flow first responders with PPE, high-credibility information injection |
| **Interventions** | Static, global, responder, entropy-targeted, density-aware, exposure-aware, and bottleneck-avoidance broadcasts |
| **Analysis** | Fundamental diagram extraction, entropy metrics, incapacitation tracking |
| **Studies** | YAML-driven multi-seed, multi-variant sweeps with Parquet/CSV/figure export |

## Usage

```console
$ git clone https://github.com/gongahkia/chiyoda && cd chiyoda
$ make config
$ python -m chiyoda.cli run scenarios/station_baseline.yaml -o out/baseline
$ python -m chiyoda.cli run scenarios/station_sarin.yaml -o out/sarin
$ python -m chiyoda.cli sweep scenarios/study_ited_full.yaml -o out/ited_study
$ python -m chiyoda.cli sweep scenarios/study_information_control.yaml -o out/information_control
$ python -m chiyoda.cli compare out/baseline out/sarin -o out/comparison
```

## Structure

| File / Folder | Purpose |
|:---|:---|
| [`chiyoda/core/`](./chiyoda/core) | Simulation engine with ITED integration |
| [`chiyoda/agents/`](./chiyoda/agents) | CognitiveAgent (BDI), Commuter, FirstResponder |
| [`chiyoda/information/`](./chiyoda/information) | InformationField, GossipModel, entropy metrics |
| [`chiyoda/environment/`](./chiyoda/environment) | Layout, multi-hazard physics, obstacles |
| [`chiyoda/navigation/`](./chiyoda/navigation) | Social force model, belief-weighted pathfinding |
| [`chiyoda/analysis/`](./chiyoda/analysis) | Metrics, telemetry, report generation |
| [`chiyoda/studies/`](./chiyoda/studies) | Study schemas, bundle models, comparison workflows |
| [`chiyoda/scenarios/`](./chiyoda/scenarios) | Scenario loading and management |
| [`scenarios/`](./scenarios) | YAML scenario definitions and layouts |
| [`tests/`](./tests) | Test suite |

## ITED Study Variants

The included `study_ited_full.yaml` defines 9 experimental variants:

| Variant | Description |
|:--------|:------------|
| `baseline_no_hazard` | No hazard — establishes fundamental diagram baseline |
| `perfect_info` | All agents have perfect exit/hazard knowledge |
| `no_info` | Agents start with zero knowledge |
| `asymmetric_info` | Mixed familiarity with gossip propagation |
| `responder_early` | First responders arrive at t=6s |
| `responder_late` | First responders arrive at t=20s |
| `no_responder` | No responders — tests organic info spread |
| `high_decay` | Fast belief decay (5× baseline) |
| `no_beacons` | Disabled PA/signage system |

## Entropy-Guided Information Control

Chiyoda now treats emergency communication as a first-class intervention
instead of a fixed background assumption. Scenario files can opt into an
`interventions` block that schedules static signage/PA broadcasts, global
announcements, responder relays, or adaptive policies that target high-entropy
agents, dense clusters, high-exposure agents, or active bottlenecks.

The core research question is whether reducing uncertainty always improves
evacuation outcomes, or whether poorly timed and poorly targeted information
can create harmful convergence, exit imbalance, bottleneck queues, or hazard
exposure. The exported study bundles include an `interventions` table plus
summary metrics such as `information_safety_efficiency`,
`harmful_convergence_index`, `intervention_entropy_reduction`, and
`intervention_accuracy_gain`.

Included study definitions:

| Study | Purpose |
|:------|:--------|
| `study_information_control.yaml` | Compare no intervention, static/global broadcasts, responder relay, and adaptive targeting policies |
| `study_intervention_ablation.yaml` | Ablate intervention timing, budget, and adaptive target choice |
| `study_message_quality.yaml` | Stress-test message credibility, delay, and frequency |

## Reference

The name `Chiyoda` references the [Chiyoda Line](https://en.wikipedia.org/wiki/Tokyo_Metro_Chiyoda_Line) of the [Tokyo Metro](https://www.tokyometro.jp/en/index.html), where sarin gas was released on 20 March 1995 as part of the [Tokyo subway sarin attacks](https://en.wikipedia.org/wiki/Tokyo_subway_sarin_attack).

## Research

* [*Social force model for pedestrian dynamics*](https://arxiv.org/abs/cond-mat/9805244) by Helbing & Molnár (1995)
* [*Simulation of pedestrian dynamics using a two-dimensional cellular automaton*](https://www.sciencedirect.com/science/article/pii/S0378437101001418) by Burstedde et al. (2001)
* [*A Cellular Automaton Model for Pedestrians' Movements Influenced by Gaseous Hazardous Material Spreading*](https://onlinelibrary.wiley.com/doi/10.1155/2020/3402198) by Makmul (2020)
* [*Using Cellular Automata to Model High Density Pedestrian Dynamics*](https://pmc.ncbi.nlm.nih.gov/articles/PMC7302238/) by Bazior, Pałka & Wąs (2020)
* [*Dynamic models of commuter behavior*](https://www.sciencedirect.com/science/article/abs/pii/0191260790900366) by Mahmassani (1990)
* [*Neural Cellular Automata and Deep Equilibrium Models*](http://arxiv.org/pdf/2501.03573.pdf) by Jia (2025)
