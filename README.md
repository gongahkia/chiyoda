<p align="center">
  <img src="./asset/logo/chiyoda.png" width="20%" alt="Chiyoda mascot"/>
</p>

<h1 align="center">Chiyoda</h1>

<h4 align="center">
    Entropy-guided information control for hazard-coupled pedestrian evacuation
</h4>

<p align="center">
    <a href="https://github.com/gongahkia/chiyoda">
        <img src="https://img.shields.io/badge/chiyoda-3.0.0-85c8c8?style=for-the-badge"
            alt="Chiyoda 3.0.0"/></a>
</p>

## Table of Contents

* [Purpose](#purpose)
* [Capabilities](#capabilities)
* [Usage](#usage)
* [Benchmarks](#benchmarks)
* [Paper](#paper)
* [Citation](#citation)
* [Research](#research)
* [Reference](#reference)

## Purpose

The general view is that emergency warnings are inherently helpful, with the implicit assumption being that giving people more information allows for a smoother evacuation.

I don't necessarily think that's true, and a large body of [existing research](#research) appears to agree.

With that in mind, I created `Chiyoda` to model evacuation as a coupled physical-information system. 

For the nerds, `Chiyoda`'s agents carry probabilistic exit and hazard beliefs, exchange local gossip, receive beacon-to-responder messages and move through social-force crowd dynamics. In doing so, this toolkit evaluates communication policies by the metrics of joint belief and safety effects instead of evacuation count alone.

## Capabilities

| Layer | Capability |
|:------|:-----------|
| **Information** | Shannon-entropy beliefs, gossip propagation, beacon broadcast, responder relay, belief decay, hostile-channel provenance |
| **Agents** | BDI-style cognitive agents, familiarity priors, herding/exploration, physiological impairment |
| **Hazards** | Gas, Smoke, Fire, Crush, Shooter, Wildfire, Ember, Flood, Earthquake, Aftershock hazards, Spread fields, Imported-field checks, Visibility reduction |
| **Navigation** | Social-force dynamics, belief-weighted A* routing, bottleneck and counterflow pressure, floor-aware connector routing |
| **Interventions** | Static, global, responder, entropy-targeted, density-aware, exposure-aware, bottleneck-avoidance, bounded LLM, and responder-coordination messaging |
| **Analysis** | Belief entropy, belief accuracy, information-safety efficiency, harmful-convergence index, benchmark scoring, trajectory exports, static 3D viewer exports |

## Usage

The below instructions are for locally installing and running `Chiyoda`.

1. First run the below commands to clone the repo and set up the dev environment.

```console
$ git clone https://github.com/gongahkia/chiyoda && cd chiyoda
$ make venv
$ make verify PYTHON=.venv/bin/python
```

2. Next execute the below to run an evacuation scenario.

```console
$ python -m chiyoda.cli validate-scenario scenarios/station_baseline.yaml
$ python -m chiyoda.cli run scenarios/station_baseline.yaml -o out/baseline
$ python -m chiyoda.cli run scenarios/station_sarin.yaml -o out/sarin
```

3. Optionally run a study sweep with this command.

```console
$ python -m chiyoda.cli sweep scenarios/study_information_control.yaml -o out/information_control
```

4. Finally, observe a completed run with the static 3D viewer.

```console
$ cd out/information_control/viewer
$ python3 -m http.server 8000
```

## Benchmarks

Run `Chiyoda`'s Benchmark with the below command.

```console
$ python -m chiyoda.cli benchmark submit --suite v1 -o out/benchmark_submission
```

## Paper

You can find my paper *Chiyoda: Entropy-Guided Information Control for Hazard-Coupled Pedestrian Evacuation* published on Zenodo [here](https://zenodo.org/records/19905070).

## Citation

Software citation metadata is provided in [`CITATION.cff`](./CITATION.cff).

## Research

* [*Simulation of Urban Density Scenario according to the Cadastral Map using K-Means Unsupervised Classification*](https://www.researchgate.net/publication/381057650_Simulation_of_Urban_Density_Scenario_according_to_the_Cadastral_Map_using_K-Means_unsupervised_classification) by M. A. El-Kenawy et al. (2023)
* [*Parametric Modeling for Form-Based Planning in Dense Urban Environments*](https://www.mdpi.com/2071-1050/11/20/5678) by S. A. Abdul-Rahman et al. (2019)
* [*Knowledge-Based Modeling of Buildings in Dense Urban Areas by Fusing LiDAR and Aerial Images*](https://www.mdpi.com/2072-4292/5/11/5944) by J. Jung et al. (2013)
* [*Simulating Urban Growth through Case-Based Reasoning*](https://www.tandfonline.com/doi/full/10.1080/22797254.2022.2056518) by Y. Liu et al. (2022)
* [*Generative Methods for Urban Design and Rapid Solution Space Exploration*](https://arxiv.org/abs/2212.06783) by Y. Sun and T. Dogan (2022)
* [*UrbanSim: Open Source Urban Simulation System*](https://urbansim.com/) by P. Waddell (2002)
* [*A Study of the “Kowloon Walled City”*](https://hub.hku.hk/bitstream/10722/259448/1/Content.pdf) by T. F. Ng (2018)
* [*CAE Simulates Complex Dense Urban Environments with Cesium*](https://cesium.com/blog/2022/02/15/cae-simulates-a-complex-dense-urban-environment/) by CAE (2022)
* [*Simulation of Urban Density Scenario according to the Cadastral Map using K-Means Unsupervised Classification*](https://www.researchgate.net/publication/381057650_Simulation_of_Urban_Density_Scenario_according_to_the_Cadastral_Map_using_K-Means_unsupervised_classification) by M. A. El-Kenawy et al. (2023)
* [*Parametric Modeling for Form-Based Planning in Dense Urban Environments*](https://www.mdpi.com/2071-1050/11/20/5678) by S. A. Abdul-Rahman et al. (2019)

## Reference

The name `Chiyoda` is in reference to the [Tokyo Metro Chiyoda Line](https://en.wikipedia.org/wiki/Tokyo_Metro_Chiyoda_Line) and the [1995 Tokyo Subway Sarin Attack](https://en.wikipedia.org/wiki/Tokyo_subway_sarin_attack) enacted by the [Aum Shinrikyo](https://en.wikipedia.org/wiki/Aum_Shinrikyo) Cult.

<div align="center">
    <img src="./asset/logo/map.png" width="65%">
</div>