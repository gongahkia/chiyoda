# Reproducibility Guide

This document records the commands and artifact layout for the current Chiyoda
paper. The goal is to make the empirical package regenerable without relying on
manual notebook state.

## Environment

From the repository root:

```sh
uv venv .venv --python python3.12
uv pip install --python .venv/bin/python -r requirements.txt
```

The long study commands below use `.venv/bin/python` because the paper figures
and Parquet exports require the project dependencies from `requirements.txt`.

## Completed Study Commands

The primary study is the 50-seed information-control comparison:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_information_control.yaml \
  -o out/information_control_50 \
  --seed-count 50
```

The first supporting study is the 30-seed intervention ablation:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_intervention_ablation.yaml \
  -o out/intervention_ablation_30 \
  --seed-count 30
```

The second supporting study is the 30-seed message-quality stress test:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_message_quality.yaml \
  -o out/message_quality_30 \
  --seed-count 30
```

If an output directory already exists, move it aside or remove it before
rerunning a study. The exported CSV trajectory tables are large, so keep enough
free disk space for the full artifact package.

## Artifact Index

Each study directory has the same structure:

```text
out/<study_name>/
  metadata.json
  tables/
    summary.parquet
    summary.csv
    steps.parquet
    cells.parquet
    agent_steps.parquet
    agents.parquet
    bottlenecks.parquet
    dwell_samples.parquet
    exits.parquet
    hazards.parquet
    gossip.parquet
    interventions.parquet
    *.csv
  figures/
    01_layout_and_keyframes.{pdf,png,svg}
    02_occupancy_and_slowdown.{pdf,png,svg}
    03_bottleneck_dynamics.{pdf,png,svg}
    04_exit_and_flow.{pdf,png,svg}
    05_distributions.{pdf,png,svg}
    06_scenario_comparison.{pdf,png,svg}
    07_entropy_heatmap.{pdf,png,svg}
    08_fundamental_diagram.{pdf,png,svg}
    09_belief_survival.{pdf,png,svg}
    10_responder_timing.{pdf,png,svg}
    11_info_flow_network.{pdf,png,svg}
    12_intervention_timeline.{pdf,png,svg}
    13_information_safety_frontier.{pdf,png,svg}
```

The paper currently uses the 50-seed primary study for generated statistics
and the two main evaluation figures:

```text
out/information_control_50/tables/summary.parquet
out/information_control_50/tables/interventions.parquet
out/information_control_50/figures/12_intervention_timeline.pdf
out/information_control_50/figures/13_information_safety_frontier.pdf
```

The ablation and message-quality studies provide the support-study aggregate
tables in `paper/sections/evaluation.tex`.

## Regenerating Paper Statistics and PDFs

From the `paper/` directory:

```sh
make stats STUDY_DIR=../out/information_control_50 PYTHON=../.venv/bin/python
make smoke STUDY_DIR=../out/information_control_50 PYTHON=../.venv/bin/python
```

If `acmart.cls` and `latexmk` are available, build the main ACM-style preprint:

```sh
make paper STUDY_DIR=../out/information_control_50 PYTHON=../.venv/bin/python
```

The smoke build writes `paper/main_smoke.pdf`. The full build writes
`paper/main.pdf`.

## Verification

From the repository root:

```sh
PYTHONPATH=. pytest -q
```

The current completed verification target is a passing Python test suite and a
successful paper smoke build against `out/information_control_50`.
