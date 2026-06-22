<p align="center">
  <img src="./asset/logo/chiyoda.png" width="150" alt="Chiyoda mascot"/>
</p>

<h1 align="center">Chiyoda</h1>

<h4 align="center">
    Entropy-guided information control for hazard-coupled pedestrian evacuation
</h4>

<div align="center">
    <a href="./docs/developer_environment.md">Developer Setup</a>
</div>
<br></br>

<p align="center">
    <a href="https://github.com/gongahkia/chiyoda">
        <img src="https://img.shields.io/badge/chiyoda-3.0.0-85c8c8?style=for-the-badge"
            alt="Chiyoda 3.0.0"/></a>
</p>

## Table of Contents

* [Why Chiyoda?](#why-chiyoda)
* [How can I use it?](#how-can-i-use-it)
* [Repository Guide](#repository-guide)
* [Development](#️development)
* [Citation](#citation)
* [Research Context](#research-context)

## Why Chiyoda?

Emergency warnings are usually treated as helpful information: tell people
more, and evacuation decisions should improve. Chiyoda studies the harder
control problem. A message can reduce uncertainty while also synchronizing
many agents toward the same exit, bottleneck, or hazard-adjacent route.

Chiyoda models evacuation as a coupled physical-information system. Agents
carry probabilistic exit and hazard beliefs, exchange local gossip, receive
beacon and responder messages, move through social-force crowd dynamics, and
route through the world they believe exists rather than the omniscient ground
truth. The toolkit evaluates communication policies by joint belief and safety
effects instead of raw evacuation count alone.

Core capabilities include:

| Layer | Capability |
|:------|:-----------|
| **Information** | Shannon-entropy beliefs, gossip propagation, beacon broadcast, responder relay, belief decay, hostile-channel provenance |
| **Agents** | BDI-style cognitive agents, familiarity priors, herding/exploration, physiological impairment |
| **Hazards** | GAS/SMOKE/FIRE/CRUSH/SHOOTER/WILDFIRE/EMBER/FLOOD/EARTHQUAKE/AFTERSHOCK hazards, spread fields, imported-field checks, visibility reduction |
| **Navigation** | Social-force dynamics, belief-weighted A* routing, bottleneck and counterflow pressure, floor-aware connector routing |
| **Interventions** | Static, global, responder, entropy-targeted, density-aware, exposure-aware, bottleneck-avoidance, bounded LLM, and responder-coordination messaging |
| **Analysis** | Belief entropy, belief accuracy, information-safety efficiency, harmful-convergence index, benchmark scoring, trajectory exports, static 3D viewer exports |

## How can I use it?

Clone the repository and create the local Python environment:

```console
$ git clone https://github.com/gongahkia/chiyoda
$ cd chiyoda
$ make venv
$ make verify PYTHON=.venv/bin/python
```

Run a single evacuation scenario:

```console
$ python -m chiyoda.cli validate-scenario scenarios/station_baseline.yaml
$ python -m chiyoda.cli run scenarios/station_baseline.yaml -o out/baseline
$ python -m chiyoda.cli run scenarios/station_sarin.yaml -o out/sarin
```

Run a study sweep:

```console
$ python -m chiyoda.cli sweep scenarios/study_information_control.yaml -o out/information_control
```

Inspect a completed run in the static 3D viewer:

```console
$ cd out/information_control/viewer
$ python3 -m http.server 8000
```

The viewer can also export edited strict `layout.floors` scenarios; see
[`docs/3d_viewer.md`](./docs/3d_viewer.md).

Validate edited scenarios before rerunning them:

```console
$ python -m chiyoda.cli validate-scenario ~/Downloads/chiyoda_edited_scenario.yaml
$ python -m chiyoda.cli run ~/Downloads/chiyoda_edited_scenario.yaml -o out/edited
```

Run scenario runtime assertions:

```console
$ python -m chiyoda.cli assert-scenario scenarios/validation_multifloor_connectors.yaml
$ python -m chiyoda.cli assert-scenario scenarios/validation_elevator_queue.yaml
```

Convert OSM/GTFS-like GeoJSON into strict scenario YAML:

```console
$ python -m chiyoda.cli convert-layout station.geojson scenarios/station_converted.yaml --name station_converted
```

Run toy calibration/regression and profiler scripts:

```console
$ python scripts/run_toy_calibrations.py -o out/toy_calibrations.json
$ python scripts/profile_large_scenario.py scenarios/station_sarin.yaml --max-steps 100 --population-total 250 -o out/profile_station_sarin.json
```

Compare study outputs or reference trajectories:

```console
$ python -m chiyoda.cli compare out/baseline out/sarin -o out/comparison
$ python -m chiyoda.cli compare-trajectory-reference out/information_control reference_trajectories.csv -o out/trajectory_reference.csv
```

Run Benchmark v1:

```console
$ python -m chiyoda.cli benchmark submit --suite v1 -o out/benchmark_submission
```

Benchmark v1 is a smoke-scale comparative suite, not an external validation
claim. It scores intervention policies across `transit_cbrn`,
`transit_shooter`, and `transit_mixed` with fixed seeds and a reproducibility
manifest.

For live LLM experiments, place `OPENAI_API_KEY=...` or
`ANTHROPIC_API_KEY=...` in `.env`. Use bounded, validated, replayable
generated-message workflows before treating any new live-provider run as
evidence.

## Repository Guide

| File / Folder | Purpose |
|:---|:---|
| [`chiyoda/core/`](./chiyoda/core) | Simulation engine and ITED runtime loop |
| [`chiyoda/agents/`](./chiyoda/agents) | Cognitive agents, commuters, and first responders |
| [`chiyoda/information/`](./chiyoda/information) | Belief fields, gossip, entropy metrics, and interventions |
| [`chiyoda/environment/`](./chiyoda/environment) | Layouts, obstacles, exits, and hazard fields |
| [`chiyoda/navigation/`](./chiyoda/navigation) | Social-force movement and belief-weighted pathfinding |
| [`chiyoda/analysis/`](./chiyoda/analysis) | Metrics, telemetry, reports, and figure exports |
| [`chiyoda/studies/`](./chiyoda/studies) | Study schemas, bundle persistence, and comparison workflows |
| [`scenarios/`](./scenarios) | YAML scenario and study definitions |
| [`docs/`](./docs) | Calibration, validation, geometry, and developer notes |
| [`tests/`](./tests) | Pytest suite |

Important documentation:

* [`docs/developer_environment.md`](./docs/developer_environment.md) documents
  local Python setup.
* [`docs/architecture_overview.md`](./docs/architecture_overview.md) describes
  the ITED runtime, information-warfare layer, and benchmark layer.
* [`docs/paper_outline_info_warfare.md`](./docs/paper_outline_info_warfare.md)
  outlines the current paper framing and verified benchmark smoke results.
* [`docs/reproducibility_kit.md`](./docs/reproducibility_kit.md) records the
  environment pin, seed set, expected outputs, and hash manifest.
* [`docs/external_validation.md`](./docs/external_validation.md) describes the
  Wuppertal bottleneck reference check.
* [`docs/benchmark/spec.md`](./docs/benchmark/spec.md) describes Benchmark v1.
* [`docs/station_geometry_workflow.md`](./docs/station_geometry_workflow.md)
  describes station geometry import and role inference.
* [`docs/3d_viewer.md`](./docs/3d_viewer.md) describes the static Three.js
  viewer export.
* [`docs/implementation_audit.md`](./docs/implementation_audit.md) states the
  current runtime semantics and known viewer/modeling gaps.
* [`docs/scenario_assertions.md`](./docs/scenario_assertions.md) describes
  runtime assertion checks, toy calibration scenarios, and profiling scripts.
* [`docs/standards_research.md`](./docs/standards_research.md) records the
  GTFS Pathways, OSM indoor, and pedestrian LOS references used to shape the
  multi-floor schema.
* [`docs/llm_agent_decisions.md`](./docs/llm_agent_decisions.md) describes
  bounded, replayable LLM agent-decision experiments.
* [`docs/llm_selective_controls.md`](./docs/llm_selective_controls.md) describes
  provider abstraction, budget guards, persona calibration, responder
  coordination, and `llm_calls` replay audit.

Hazard staging roadmap:

| Release | Scope |
|:--|:--|
| v1 | transit CBRN, active shooter, smoke, hostile information channels |
| v2 | Wildfire/WUI hazard, ember spread, long-range broadcast, vehicular-pedestrian egress coupling |
| v3 | Flood inundation, earthquake aftershock damage, re-evacuation waves |

## Development

Run the test suite:

```console
$ make verify PYTHON=.venv/bin/python
```

Check local dependencies:

```console
$ make doctor PYTHON=.venv/bin/python
```

The codebase intentionally keeps external pedestrian-analysis and high-fidelity
hazard tools at the boundary. Chiyoda exports trajectory and telemetry tables
for comparison with tools such as PedPy, JuPedSim, Vadere, and FDS-oriented
hazard-field workflows instead of claiming to replace them.

## Citation

Software citation metadata is provided in [`CITATION.cff`](./CITATION.cff).

## Research Context

The name `Chiyoda` references the
[Tokyo Metro Chiyoda Line](https://en.wikipedia.org/wiki/Tokyo_Metro_Chiyoda_Line).
The motivating domain is emergency communication under CBRN-like evacuation
pressure, but the framework is implemented as a more general information-control
simulation layer for spatial evacuation studies.

Chiyoda sits between pedestrian dynamics, hazard-coupled evacuation, and
information-aware decision models. Its contribution is not a new crowd-force
law; it is a replayable intervention surface and evaluation package for asking
when emergency communication improves safety and when it creates harmful
convergence.

<div align="center">
    <img src="./asset/logo/map.png" width="65%">
</div>
