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
$ python -m chiyoda.cli compare-trajectory-reference out/information_control reference_trajectories.csv -o out/trajectory_reference.csv
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

## Calibration and Reference Checks

Station geometry can be loaded from text grids, GeoJSON, or a small DXF
subset. The GeoJSON path accepts explicit Chiyoda roles and can also infer
walkable, blocked, and exit cells from common OSM/OpenStationMap indoor tags
and GTFS Pathways fields. See
[`docs/station_geometry_workflow.md`](./docs/station_geometry_workflow.md) for
the auditable workflow, manual fallbacks, and the synthetic
`edge_bottleneck_station` fixture.

Scenario YAML can calibrate behavior at two levels. The `behavior` block
accepts the full `BehaviorConfig` surface (`density_panic_weight`,
`neighbor_panic_weight`, `hazard_panic_weight`, `entropy_anxiety_weight`,
`freeze_probability`, `calm_recovery_rate`, and `helping_threshold`). Each
population cohort can set exact `base_speed`/`base_speed_mps`,
`base_rationality`, `credibility`, `gossip_radius`, and `base_vision_radius`
in addition to the existing familiarity, calmness, grouping, and release
fields. See
[`docs/population_calibration.md`](./docs/population_calibration.md) and
`scenarios/study_population_calibration_examples.yaml` for example-only
calibration variants and parameter provenance. Generated population priors are
available as an opt-in cache/replay preprocessing path; see
[`docs/generated_population_calibration.md`](./docs/generated_population_calibration.md).

The `compare-trajectory-reference` command performs a lightweight check
against a reference trajectory CSV or Parquet table with `agent_id`, `time_s`,
`x`, and `y` columns. It reports first-order deltas for duration, path length,
displacement, speed, and local density. For full trajectory science, Chiyoda
exports `agent_steps` tables that can be analyzed with dedicated tools such as
PedPy instead of reimplementing those methods here.
Export helpers can also write compact JuPedSim- and Vadere-compatible
trajectory tables for external comparison workflows; see
[`docs/trajectory_reference_workflow.md`](./docs/trajectory_reference_workflow.md).

Drill, VR, incident, and expert-coded event references can be loaded through
the standalone `chiyoda.references` schema. These records include explicit
source, license, timestamp, station, scenario-assumption, and missing-data
provenance and remain separate from simulation execution. See
[`docs/event_reference_ingestion.md`](./docs/event_reference_ingestion.md).

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
| `study_regime_robustness.yaml` | Full 3x3x5 robustness grid over hazard regime, familiarity regime, and representative communication policy |
| `study_llm_target_selection_ablation.yaml` | Hold generated-message settings fixed while varying LLM recipient targeting |
| `study_llm_regime_robustness.yaml` | Focused LLM OpenAI/replay extension over the 3x3 hazard/familiarity regime grid |
| `study_llm_prompt_objective_ablation.yaml` | Hold LLM guidance mechanics fixed while varying safety, hazard, anti-convergence, and urgency prompt objectives |
| `study_llm_budget_equivalence.yaml` | Compare sparse LLM guidance with static- and entropy-equivalent generated-message budgets |

The current paper artifact is documented in
[`paper/REPRODUCIBILITY.md`](./paper/REPRODUCIBILITY.md), including the exact
50-seed primary study command, the two 30-seed support-study commands, and the
artifact index used by the LaTeX build.

## Reference

The name `Chiyoda` references the [Chiyoda Line](https://en.wikipedia.org/wiki/Tokyo_Metro_Chiyoda_Line) of the [Tokyo Metro](https://www.tokyometro.jp/en/index.html), where sarin gas was released on 20 March 1995 as part of the [Tokyo subway sarin attacks](https://en.wikipedia.org/wiki/Tokyo_subway_sarin_attack).

## Research

* [*Social force model for pedestrian dynamics*](https://arxiv.org/abs/cond-mat/9805244) by Helbing & Molnár (1995)
* [*Simulation of pedestrian dynamics using a two-dimensional cellular automaton*](https://www.sciencedirect.com/science/article/pii/S0378437101001418) by Burstedde et al. (2001)
* [*A Cellular Automaton Model for Pedestrians' Movements Influenced by Gaseous Hazardous Material Spreading*](https://onlinelibrary.wiley.com/doi/10.1155/2020/3402198) by Makmul (2020)
* [*Using Cellular Automata to Model High Density Pedestrian Dynamics*](https://pmc.ncbi.nlm.nih.gov/articles/PMC7302238/) by Bazior, Pałka & Wąs (2020)
* [*Dynamic models of commuter behavior*](https://www.sciencedirect.com/science/article/abs/pii/0191260790900366) by Mahmassani (1990)
* [*Neural Cellular Automata and Deep Equilibrium Models*](http://arxiv.org/pdf/2501.03573.pdf) by Jia (2025)
