[![](https://img.shields.io/badge/chiyoda_1.0.0-passing-light_green)](https://github.com/gongahkia/chiyoda/releases/tag/1.0.0) 
[![](https://img.shields.io/badge/chiyoda_2.0.0-passing-green)](https://github.com/gongahkia/chiyoda/releases/tag/2.0.0) 

# `Chiyoda`

[Simulating](#gifs) [commuter](https://dictionary.cambridge.org/dictionary/english/commuter) dynamics within a [static system](https://www.quora.com/What-is-the-difference-between-a-static-and-dynamic-system).

Rendered with [Matplotlib](https://matplotlib.org/stable/)'s [animation API](https://matplotlib.org/stable/api/animation_api.html) and [Plotly dashboards](https://plotly.com/python/).

## Usage

```console
$ git clone https://github.com/gongahkia/chiyoda && cd chiyoda
$ make config
$ python -m chiyoda.cli run scenarios/example.yaml --headless -o out.html
$ python -m chiyoda.cli run scenarios/example.yaml -o dashboard.html
$ python -m chiyoda.cli generate layouts/new_layout.txt --width 40 --height 30
```

## Screenshots

![](./asset/reference/v2/1.png)
![](./asset/reference/v2/2.png)

## GIFs

| Normal movement | Static simulation |
| :---: | :---: |
| ![](./asset/reference/v1/normal.gif) | ![](./asset/reference/v1/tweak.gif) |
| **Bottleneck simulation** | **Line of sight simulation** |
| ![](./asset/reference/v1/bottleneck.gif) | ![](./asset/reference/v1/los.gif) |
| **Path density heatmap** | **Population density heatmap** |
| ![](./asset/reference/v1/path.gif) | ![](./asset/reference/v1/population.gif) |

## Structure

| File / Folder name | Purpose |
| :--- | :--- |
| [`chiyoda/`](./chiyoda) | Core Python package. |
| [`chiyoda/cli.py`](./chiyoda/cli.py) | CLI entrypoint for running simulations. |
| [`chiyoda/agents/`](./chiyoda/agents) | Agent models (commuter, groups, behaviors). |
| [`chiyoda/analysis/`](./chiyoda/analysis) | Metrics, telemetry, and report generation. |
| [`chiyoda/core/`](./chiyoda/core) | Simulation engine. |
| [`chiyoda/environment/`](./chiyoda/environment) | Layout, obstacles, exits, and hazards. |
| [`chiyoda/navigation/`](./chiyoda/navigation) | Pathfinding, social force, and spatial indexing. |
| [`chiyoda/scenarios/`](./chiyoda/scenarios) | Scenario loading and management. |
| [`chiyoda/visualization/`](./chiyoda/visualization) | Plotly dashboard visualisation. |
| [`src/`](./src) | Standalone v1 Matplotlib scripts. |
| [`scenarios/`](./scenarios) | YAML scenario definitions and sample layouts. |
| [`tests/`](./tests) | Test suite. |

## Reference

The name `Chiyoda` is in reference to the [Chiyoda Line](https://en.wikipedia.org/wiki/Tokyo_Metro_Chiyoda_Line) of the [Tokyo Metro](https://www.tokyometro.jp/en/index.html), the first line where sarin gas was released *(by [Yasuo Hayashi](https://en.wikipedia.org/wiki/Lin_Tainan) near [Shin-Ochanomizu station](https://en.wikipedia.org/wiki/Shin-ochanomizu_Station) on 20 March 1995, 7:50 am)* as part of the [Tokyo subway sarin attacks](https://en.wikipedia.org/wiki/Tokyo_subway_sarin_attack) enacted by the [Aum Shinrikyo](https://en.wikipedia.org/wiki/Aum_Shinrikyo) cult.

![](./asset/logo/map.png)

## Research

* [*Boids algorithm demonstration*](https://eater.net/boids) by Ben Eater
* [*Dynamic models of commuter behavior: Experimental investigation and application to the analysis of planned traffic disruptions*](https://www.sciencedirect.com/science/article/abs/pii/0191260790900366) by Hani S. Mahmassani
* [*Cellular Automata and Complexity: Collected Papers*](https://www.stephenwolfram.com/publications/cellular-automata-complexity/) by Stephen Wolfram
* [*Flocking and swarming in a multi-agent dynamical system*](https://pubs.aip.org/aip/cha/article-abstract/33/12/123126/2930567/Flocking-and-swarming-in-a-multi-agent-dynamical?redirectedFrom=fulltext) by Gourab Kumar Sar and Dibakar Ghosh
* [*Exploring Fungal Morphology Simulation and Dynamic Light*](https://dl.acm.org/doi/10.1145/3680530.3695440) by Kexin Wang, Ivy He, Jinke Li, Ali Asadipour and Yitong Sun
* [*Cellular Automata and Applications*](https://www.whitman.edu/Documents/Academics/Mathematics/andrewgw.pdf) by Gavin Andrews
* [*Simulation of a Bio-Inspired Flocking-Based Aggregation Behaviour for Swarm Robotics*](https://www.mdpi.com/2313-7673/9/11/668) by Samira Rasouli, Kerstin Dautenhahn and Chrystopher L. Nehaniv
* [*On complexity of colloid cellular automata*](https://www.nature.com/articles/s41598-024-72107-6) by Andrew Adamatzky, Nic Roberts, Raphael Fortulan, Noushin Raeisi Kheirabadi, Panagiotis Mougkogiannis, Michail-Antisthenis Tsompanas, Genaro J. Martínez, Georgios Ch. Sirakoulis and Alessandro Chiolerio 
* [*Recent trends in robot learning and evolution for swarm robotics*](https://www.frontiersin.org/journals/robotics-and-ai/articles/10.3389/frobt.2023.1134841/full) by Jonas Kuckling
* [*Neural Cellular Automata and Deep Equilibrium Models*](http://arxiv.org/pdf/2501.03573.pdf) by Zhibai Jia
* [*A Cellular Automaton Model for Pedestrians' Movements Influenced by Gaseous Hazardous Material Spreading*](https://onlinelibrary.wiley.com/doi/10.1155/2020/3402198) by J. Makmul
* [*Simulation of Pedestrian Movements Using Fine Grid Cellular Automata*](https://arxiv.org/ftp/arxiv/papers/1406/1406.3567.pdf) by Siamak Sarmadya, Fazilah Harona and Abdullah Zawawi Taliba
* [*Using Cellular Automata to Model High Density Pedestrian Dynamics*](https://pmc.ncbi.nlm.nih.gov/articles/PMC7302238/) by Grzegorz Bazior, Dariusz Pałka and Jarosław Wąs
