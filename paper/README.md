# Chiyoda -- research paper

LaTeX scaffold for the Chiyoda paper, modeled after `../yuho/paper`.
The paper is structured as an arXiv-style ACM preprint with a lightweight
article-class smoke build for machines without `acmart.cls`.

## Layout

```text
paper/
  main.tex              # acmart entry point, metadata, abstract, section wiring
  main_smoke.tex        # article-class fallback build
  references.bib        # seeded citations
  stats.tex             # generated from an exported study bundle
  REPRODUCIBILITY.md    # exact study commands and artifact index
  Makefile              # paper / smoke / stats / figures / arxiv / clean
  scripts/
    gen_stats.py        # study bundle -> stats.tex
  sections/
    introduction.tex
    claims.tex
    background.tex
    design.tex
    implementation.tex
    evaluation.tex
    related.tex
    limitations.tex
    conclusion.tex
  figures/
    architecture.mmd
    intervention-loop.mmd
    policy-taxonomy.mmd
```

## Build

```sh
cd paper
make stats STUDY_DIR=../out/information_control_50 PYTHON=../.venv/bin/python
make figures
make paper
make smoke
```

The default `STUDY_DIR` is `../out/information_control_50`. Generate the
primary paper artifact from the repo root with:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py scenarios/study_information_control.yaml -o out/information_control_50 --seed-count 50
```

See [`REPRODUCIBILITY.md`](./REPRODUCIBILITY.md) for the complete 50-seed
primary study, 30-seed support studies, artifact index, and regeneration
commands.

## Current Thesis

The paper studies evacuation as an information-control problem: reducing
belief entropy can improve route choice and exposure avoidance, but poorly
timed or poorly targeted information can synchronize movement, amplify
bottlenecks, and increase hazard exposure.
