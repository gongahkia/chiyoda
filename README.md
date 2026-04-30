<!-- project header -->
<h1 align="center">Chiyoda</h1>

<h4 align="center">
    Entropy-guided information control for hazard-coupled pedestrian evacuation
</h4>

<div align="center">
    <a href="https://doi.org/10.5281/zenodo.19905070">Published Paper</a> |
    <a href="./paper/main.pdf">PDF</a> |
    <a href="./paper/REPRODUCIBILITY.md">Reproducibility</a> |
    <a href="./docs/developer_environment.md">Developer Setup</a>
</div>
<br></br>

<p align="center">
    <a href="https://doi.org/10.5281/zenodo.19905070">
        <img src="https://zenodo.org/badge/DOI/10.5281/zenodo.19905070.svg"
            alt="Zenodo DOI"/></a>
    <a href="https://github.com/gongahkia/chiyoda/releases/tag/3.0.0">
        <img src="https://img.shields.io/badge/chiyoda-3.0.0-85c8c8?style=for-the-badge"
            alt="Chiyoda 3.0.0"/></a>
    <a href="./paper/main.pdf">
        <img src="https://img.shields.io/badge/paper-PDF-002b36?style=for-the-badge&color=85c8c8"
            alt="Paper PDF"/></a>
    <a href="https://creativecommons.org/licenses/by/4.0/">
        <img src="https://img.shields.io/badge/paper_license-CC_BY_4.0-85c8c8?style=for-the-badge"
            alt="Paper license: CC BY 4.0"/></a>
</p>

## Table of Contents

* [Published Paper](#published-paper)
* [Why Chiyoda?](#why-chiyoda)
* [How can I use it?](#how-can-i-use-it)
* [Repository Guide](#repository-guide)
* [Reproduce the Paper](#reproduce-the-paper)
* [Development](#️development)
* [Citation](#citation)
* [Research Context](#research-context)

## Published Paper

Chiyoda is published as a Zenodo preprint:

* **Title:** *Chiyoda: Entropy-Guided Information Control for Hazard-Coupled Pedestrian Evacuation*
* **Subtitle:** *A Framework for Benchmarking Emergency Communication Efficacy*
* **DOI:** [10.5281/zenodo.19905070](https://doi.org/10.5281/zenodo.19905070)
* **Version:** `v1`
* **License for the paper:** [Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/)

The repository contains the simulation code, scenario definitions, study
scripts, LaTeX source, generated figures, and the final paper PDF.

## Why Chiyoda?

Emergency warnings are usually treated as helpful information: tell people
more, and evacuation decisions should improve. Chiyoda studies the harder
control problem. A message can reduce uncertainty while also synchronizing
many agents toward the same exit, bottleneck, or hazard-adjacent route.

Chiyoda models evacuation as a coupled physical-information system. Agents
carry probabilistic exit and hazard beliefs, exchange local gossip, receive
beacon and responder messages, move through social-force crowd dynamics, and
route through the world they believe exists rather than the omniscient ground
truth. The paper evaluates communication policies by joint belief and safety
effects instead of raw evacuation count alone.

Core capabilities include:

| Layer | Capability |
|:------|:-----------|
| **Information** | Shannon-entropy beliefs, gossip propagation, beacon broadcast, responder relay, belief decay |
| **Agents** | BDI-style cognitive agents, familiarity priors, herding/exploration, physiological impairment |
| **Hazards** | Stylized GAS/SMOKE/FIRE/CRUSH hazards, spread fields, imported-field checks, visibility reduction |
| **Navigation** | Social-force dynamics, belief-weighted A* routing, bottleneck and counterflow pressure |
| **Interventions** | Static, global, responder, entropy-targeted, density-aware, exposure-aware, and bottleneck-avoidance messaging |
| **Analysis** | Belief entropy, belief accuracy, information-safety efficiency, harmful-convergence index, trajectory exports |

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
$ python -m chiyoda.cli run scenarios/station_baseline.yaml -o out/baseline
$ python -m chiyoda.cli run scenarios/station_sarin.yaml -o out/sarin
```

Run a study sweep:

```console
$ python -m chiyoda.cli sweep scenarios/study_information_control.yaml -o out/information_control
```

Compare study outputs or reference trajectories:

```console
$ python -m chiyoda.cli compare out/baseline out/sarin -o out/comparison
$ python -m chiyoda.cli compare-trajectory-reference out/information_control reference_trajectories.csv -o out/trajectory_reference.csv
```

For live LLM experiments, place `OPENAI_API_KEY=...` in `.env`. The paper
results use bounded, validated, replayable generated-message workflows; see
[`paper/REPRODUCIBILITY.md`](./paper/REPRODUCIBILITY.md) before treating any
new live-provider run as evidence.

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
| [`paper/`](./paper) | Paper source, figures, generated tables, and final PDF |
| [`tests/`](./tests) | Pytest suite |

Important documentation:

* [`paper/REPRODUCIBILITY.md`](./paper/REPRODUCIBILITY.md) records the exact
  study commands and artifact layout behind the paper.
* [`paper/README.md`](./paper/README.md) documents the LaTeX paper build.
* [`docs/developer_environment.md`](./docs/developer_environment.md) documents
  local Python and TeX setup.
* [`docs/external_validation.md`](./docs/external_validation.md) describes the
  Wuppertal bottleneck reference check.
* [`docs/station_geometry_workflow.md`](./docs/station_geometry_workflow.md)
  describes station geometry import and role inference.

## Reproduce the Paper

The final paper PDF is [`paper/main.pdf`](./paper/main.pdf). To rebuild it from
the current generated study artifacts:

```console
$ cd paper
$ make paper PYTHON=../.venv/bin/python
```

The primary 50-seed study command is:

```console
$ PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
    scenarios/study_information_control.yaml \
    -o out/information_control_50 \
    --seed-count 50
```

The full empirical package also includes intervention ablations, message-quality
stress tests, a 900-run hazard/familiarity robustness grid, and bounded LLM
extension studies. The complete command list is in
[`paper/REPRODUCIBILITY.md`](./paper/REPRODUCIBILITY.md).

## Development

Run the test suite:

```console
$ make verify PYTHON=.venv/bin/python
```

Check paper-build dependencies:

```console
$ make doctor PYTHON=.venv/bin/python
```

Run the lightweight paper smoke build:

```console
$ make paper-smoke PYTHON=.venv/bin/python
```

The codebase intentionally keeps external pedestrian-analysis and high-fidelity
hazard tools at the boundary. Chiyoda exports trajectory and telemetry tables
for comparison with tools such as PedPy, JuPedSim, Vadere, and FDS-oriented
hazard-field workflows instead of claiming to replace them.

## Citation

Please cite the Zenodo record if you use the paper, code, scenarios, or figures:

```bibtex
@misc{ong2026chiyoda,
  author    = {Ong, Gabriel Zhe Mian},
  title     = {Chiyoda: Entropy-Guided Information Control for Hazard-Coupled Pedestrian Evacuation},
  year      = {2026},
  publisher = {Zenodo},
  version   = {v1},
  doi       = {10.5281/zenodo.19905070},
  url       = {https://doi.org/10.5281/zenodo.19905070}
}
```

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
