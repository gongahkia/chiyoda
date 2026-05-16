[![](https://img.shields.io/badge/gibson_1.0.0-passing-light_green)](https://github.com/gongahkia/gibson/releases/tag/1.0.0) 
[![](https://img.shields.io/badge/gibson_2.0.0-passing-green)](https://github.com/gongahkia/gibson/releases/tag/2.0.0) 

# `Gibson`

Rust megastructure [generator](#seed) for cyberpunk dense-urban forms.

## Stack

* *Language*: [Rust](https://rust-lang.org/)
* *Graphics*: [Macroquad](https://macroquad.rs/), [miniquad](https://github.com/not-fl3/miniquad), [GLSL](https://www.khronos.org/opengl/wiki/OpenGL_Shading_Language)
* *Generation*: [Simplex noise](https://en.wikipedia.org/wiki/Simplex_noise), [Wave Function Collapse](https://github.com/mxgmn/WaveFunctionCollapse), [L-system](https://en.wikipedia.org/wiki/L-system), [Catmull-Rom spline](https://en.wikipedia.org/wiki/Centripetal_Catmull%E2%80%93Rom_spline)
* *Serialization*: [Serde](https://serde.rs/), [JSON](https://www.json.org/json-en.html)

## Screenshot

![](./asset/reference/v2/2.png)

## Usage

```console
$ git clone https://github.com/gongahkia/gibson && cd gibson
$ cargo run --release
$ cargo run --release -- ABCD1234
$ cargo run --release -- --seed ABCD1234 --profile neon
$ cargo run --release -- --seed ABCD1234 --profile dense --headless --export structure.json
$ cargo run --release -- --seed ABCD1234 --config presets/blackout-core.json --headless --bundle out/blackout
$ cargo run --release -- --validate-rules rules/kowloon-decay.json
$ cargo run --release -- --seed ABCD1234 --profile decayed --rules rules/kowloon-decay.json --headless --export structure.json
```

Profiles are `balanced`, `dense`, `vertical`, `decayed`, and `neon`. JSON config files can override profile defaults with `--config path.json`; checked-in config presets live under `presets/`, and editable procedural rule packs live under `rules/`.

## Seed

Randomly generated [megastructure](https://en.wikipedia.org/wiki/Megastructure)s are seeded at `current_seed.txt` and serialised at `structure.json` with generation metadata, counts, profile, config snapshot, circulation routes, strata, and semantic room labels.

## Reference

The name `Gibson` is in reference to American author [William Gibson](https://en.wikipedia.org/wiki/William_Gibson), whose debut novel [*Neuromancer*](https://en.wikipedia.org/wiki/Neuromancer) heavily influenced the [Cyberpunk](https://en.wikipedia.org/wiki/Cyberpunk) aesthetic, going on to inspire works such as [Tsutomu Nihei](https://en.wikipedia.org/wiki/Tsutomu_Nihei)'s (弐瓶 勉) [*Blame!*](https://en.wikipedia.org/wiki/Blame!) and [Masamune Shirow](https://en.wikipedia.org/wiki/Masamune_Shirow)'s (太田正典) [*Ghost in the Shell*](https://en.wikipedia.org/wiki/Ghost_in_the_Shell).

![](./asset/logo/gibson.jpg)

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
