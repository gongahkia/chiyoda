# Chiyoda -- research paper

LaTeX scaffold for the Chiyoda paper, modeled after `../yuho/paper`.
The paper is structured as an ACM-style preprint with a lightweight
article-class smoke build for machines without `acmart.cls`.

## Layout

```text
paper/
  main.tex              # acmart entry point, metadata, abstract, section wiring
  main_smoke.tex        # article-class fallback build
  references.bib        # seeded citations
  stats.tex             # generated from an exported study bundle
  REPRODUCIBILITY.md    # exact study commands and artifact index
  Makefile              # paper / smoke / stats / figures / clean
  scripts/
    gen_stats.py        # study bundle -> stats.tex
    gen_llm_tables.py   # LLM study artifacts -> llm_tables.tex
    plot_regime_robustness.py # robustness summary -> heatmap PDF
  sections/
    introduction.tex
    background.tex
    design.tex
    implementation.tex
    evaluation.tex
    robustness_plan.tex
    related.tex
    limitations.tex
    future_work.tex
    conclusion.tex
  figures/
    architecture.mmd
    intervention-loop.mmd
    policy-taxonomy.mmd
    information-safety-frontier.pdf
    intervention-timeline.pdf
    regime-robustness-heatmap.pdf
```

## Build

```sh
cd paper
make stats STUDY_DIR=../out/information_control_50 PYTHON=../.venv/bin/python
make figures
make llm-tables PYTHON=../.venv/bin/python
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

## Zenodo deposit

The main paper target builds `paper/main.pdf` with PDFLaTeX/BibTeX. For Zenodo,
deposit `paper/main.pdf` as the manuscript artifact. Optionally include a source
archive of the repository or paper directory if you want the TeX source,
generated tables, figures, and reproducibility scripts to be preserved alongside
the PDF.

## Current Thesis

The paper studies evacuation as an information-control problem: reducing
belief entropy can improve route choice and exposure avoidance, but poorly
timed or poorly targeted information can synchronize movement, amplify
bottlenecks, and increase hazard exposure.
