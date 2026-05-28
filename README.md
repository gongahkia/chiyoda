[![](https://img.shields.io/badge/gibson_1.0.0-passing-light_green)](https://github.com/gongahkia/gibson/releases/tag/1.0.0) 
[![](https://img.shields.io/badge/gibson_2.0.0-passing-green)](https://github.com/gongahkia/gibson/releases/tag/2.0.0) 
[![CI](https://github.com/gongahkia/gibson/actions/workflows/ci.yml/badge.svg)](https://github.com/gongahkia/gibson/actions/workflows/ci.yml)

# `Gibson`

[Rust](#stack) megastructure [generator](#seed) for [cyberpunk dense-urban forms](#research).

## Stack

* *Script*: [Rust](https://rust-lang.org/)
* *Graphics*: [Macroquad](https://macroquad.rs/), [miniquad](https://github.com/not-fl3/miniquad), [GLSL](https://www.khronos.org/opengl/wiki/OpenGL_Shading_Language)
* *Generation*: [Simplex noise](https://en.wikipedia.org/wiki/Simplex_noise), [Wave Function Collapse](https://github.com/mxgmn/WaveFunctionCollapse), [L-system](https://en.wikipedia.org/wiki/L-system), [Catmull-Rom spline](https://en.wikipedia.org/wiki/Centripetal_Catmull%E2%80%93Rom_spline)
* *Serialization*: [Serde](https://serde.rs/), [JSON](https://www.json.org/json-en.html)

## Screenshot

![](./asset/reference/v2/2.png)

## Usage

The below instructions are for locally running `Gibson`.

1. First install the repo on your machine.

```console
$ git clone https://github.com/gongahkia/gibson && cd gibson
```

2. Then run any of the below to use `Gibson`'s functionality.

```console
$ cargo run --release
$ cargo run --release -- ABCD1234
$ cargo run --release -- --seed ABCD1234 --profile neon
$ cargo run --release -- --seed ABCD1234 --profile balanced --typology linear-city
$ cargo run --release -- --seed ABCD1234 --profile dense --headless --export structure.json
$ cargo run --release -- --seed ABCD1234 --config presets/blackout-core.json --headless --bundle out/blackout
$ cargo run --release -- --validate-rules rules/kowloon-decay.json
$ cargo run --release -- --seed ABCD1234 --profile decayed --rules rules/kowloon-decay.json --headless --export structure.json
$ cargo run --release -- --inspect structure.json --summary --routes --quality
$ cargo run --release -- --validate structure.json
```

## Controls

* `R` to regenerate
* `,`/`.` to cycle megastructure typology/generation pattern
* `S` for screenshots
* `I` to inspect cells
* `T/Z/X/V/B/C` for semantic overlays; `B` cycles typology frame, construction eras, stress/load paths, section quality, and scenario consequences
* `U`/`J`/`N`/`M`/`K` while using `V` to pause, change speed, scrub phases, select entity kinds, and toggle kind visibility
* `G` for an in-renderer rule-pack browser
* `H` or `Shift+R` to hot reload edited rule JSON
* `E`/`1-9`/`-`/`=`/`O` to edit and export structure and entity rule weights

## Configuration

`Gibson`'s predefined profiles are `balanced`, `dense`, `vertical`, `decayed`, and `neon`. Megastructure typologies include `dense-enclave`, `arcology-spire`, `linear-city`, `bridge-void`, `marine-platform`, `orbital-ring`, `underground-hive`, `mountain-burrow`, `desert-arcology`, `airport-city`, `dam-city`, and `shipyard-stack`. Dynamic generation controls include `entity_density`, `entity_layout_pressure`, and `advanced_pattern_complexity`. Rule packs can target a typology and tune entity density, layout pressure, patrols, crowds, and builder swarms.

## Seed

Randomly generated [megastructure](https://en.wikipedia.org/wiki/Megastructure)s are seeded at `current_seed.txt` and serialised at `structure.json` with generation metadata, counts, profile, typology frame, typology quality metrics, construction history, section quality, stress/load paths, config snapshot, circulation routes, strata, semantic room labels, resource networks, stress-influenced hazards, rule packs, rule influence traces, deterministic entity movement, pressure fields, and layout mutations. Checked-in scenario examples live in `examples/scenarios/` for non-Kowloon typologies.

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

The name `Gibson` is in reference to American author [William Gibson](https://en.wikipedia.org/wiki/William_Gibson), whose debut novel [*Neuromancer*](https://en.wikipedia.org/wiki/Neuromancer) heavily influenced the [Cyberpunk](https://en.wikipedia.org/wiki/Cyberpunk) aesthetic, going on to inspire works such as [Tsutomu Nihei](https://en.wikipedia.org/wiki/Tsutomu_Nihei)'s (弐瓶 勉) [*Blame!*](https://en.wikipedia.org/wiki/Blame!) and [Masamune Shirow](https://en.wikipedia.org/wiki/Masamune_Shirow)'s (太田正典) [*Ghost in the Shell*](https://en.wikipedia.org/wiki/Ghost_in_the_Shell).

![](./asset/logo/gibson.jpg)
