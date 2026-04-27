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

The external-validity robustness study is the 900-run regime grid:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_regime_robustness.yaml \
  -o out/regime_robustness_900 \
  --checkpoint-dir out/regime_robustness_900.checkpoints \
  --resume
```

If an output directory already exists, move it aside or remove it before
rerunning a study. The exported CSV trajectory tables are large, so keep enough
free disk space for the full artifact package.

The robustness study intentionally exports Parquet tables only and skips the
default figure bundle. That does not change simulation semantics or study
results; it only avoids writing very large CSV files and unreadable 45-variant
overview figures. The checkpoint directory stores per-run Parquet outputs so a
terminated terminal session can be resumed without changing the study design or
rerunning completed seeds.

After the robustness run finishes, generate a compact regime summary with:

```sh
PYTHONPATH=. .venv/bin/python scripts/summarize_regime_robustness.py \
  out/regime_robustness_900
```

Then regenerate the paper robustness heatmap:

```sh
cd paper
../.venv/bin/python scripts/plot_regime_robustness.py \
  ../out/regime_robustness_900/tables/regime_summary.csv \
  -o figures/regime-robustness-heatmap.pdf
```

## Optional LLM Extension Pilot

The LLM extension is not part of the completed baseline evidence. It exists as
a controlled, replayable extension for future paper work. The default pilot uses a
deterministic template provider and writes generated-message cache artifacts
without requiring live API keys:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_extension.yaml \
  -o out/llm_extension_pilot
```

The `llm_guidance` policy records generated text, provider/model metadata,
validation status, validation reasons, and cache keys in the intervention
table. Live-provider implementations must preserve deterministic replay from
cache before their outputs can be used in paper comparisons.

For live OpenAI smoke tests, place the API key in `.env` as
`OPENAI_API_KEY=...`. The loader also accepts the legacy local spellings
`OPENAI-API-KEY` and `OPEN-AI-API-KEY`. Set `OPENAI_MODEL` externally or set
`llm_model` in the scenario if a specific model is required; otherwise the
OpenAI provider uses a small default model for bounded smoke tests.

The smallest opt-in live/replay pilot is:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_openai_pilot.yaml \
  -o out/llm_openai_pilot
```

Its OpenAI variant uses `llm_cache_mode: cache_first`; the replay variant uses
the same cache path with `llm_provider: replay`. Do not treat the live pilot as
a paper result until the cached artifacts, validation summary, and deterministic
replay run have been inspected. In the current artifact set, the tiny OpenAI
pilot completed with 8 accepted live messages, 0 rejected live messages, 0
fallbacks, and exact replay coverage for the cached messages.

After any LLM pilot, summarize generated-message telemetry with:

```sh
PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py \
  out/llm_openai_pilot
```

This writes `llm_generation_summary.csv`, `llm_validation_reasons.csv`, and,
when aggregate study metrics are available, `llm_policy_comparison.csv`.

After the tiny pilot is clean, the medium LLM study can be run with:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_medium.yaml \
  -o out/llm_medium
PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py \
  out/llm_medium
```

The medium design includes deterministic baselines, template generation,
OpenAI prompt-style ablations, validator-profile ablations, and replay-only
verification. The current medium run contains 80 completed runs across 8
variants and 10 seeds. Treat it as an extension study; it should not replace
the deterministic baseline evidence.

The focused LLM regime robustness extension can be run with checkpoints:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_regime_robustness.yaml \
  -o out/llm_regime_robustness \
  --checkpoint-dir out/llm_regime_robustness.checkpoints \
  --resume \
  --no-figures
PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py \
  out/llm_regime_robustness
```

The current run contains 90 completed runs: nine hazard/familiarity regimes,
five seeds per regime, one live OpenAI safety-strict cache-population variant,
and one replay-only variant per regime. It produced 360 live generated-message
events, 359 accepted OpenAI messages, one rejected congested-exit
recommendation handled by deterministic fallback, and exact replay agreement
on aggregate outcomes.

The LLM target-selection ablation can be run without live API calls:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_target_selection_ablation.yaml \
  -o out/llm_target_selection_ablation \
  --checkpoint-dir out/llm_target_selection_ablation.checkpoints \
  --resume \
  --no-figures
PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py \
  out/llm_target_selection_ablation
```

The current run contains 90 completed runs: three deterministic baselines and
six generated-message target selectors over ten seeds. All generated template
messages passed validation. The best generated-message ISE came from
bottleneck targeting (0.0450), followed by entropy targeting (0.0422), density
targeting (0.0391), and static targeting (0.0354). Global and exposure
targeting were much weaker under the same generated-message budget.

The live prompt-objective ablation tests whether safety, hazard-avoidance,
anti-convergence, and urgency prompt framings change downstream outcomes while
holding target selection, validator, cadence, radius, budget, provider, and
model fixed:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_prompt_objective_ablation.yaml \
  -o out/llm_prompt_objective_ablation \
  --checkpoint-dir out/llm_prompt_objective_ablation.checkpoints \
  --resume \
  --no-figures
PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py \
  out/llm_prompt_objective_ablation
```

The current run contains 110 completed runs. The four live OpenAI prompt
variants produced 320 generated-message events, all accepted by validation,
with exact replay agreement. Safety prompting has the highest generated-policy
ISE, hazard-avoidance prompting has the lowest generated-policy HCI, and the
explicit anti-convergence prompt does not lower HCI in this scenario.

The live budget-equivalence sweep tests whether the sparse LLM efficiency
advantage survives when generated guidance receives static-beacon or
entropy-targeted intervention budgets:

```sh
PYTHONPATH=. .venv/bin/python scripts/run_study_progress.py \
  scenarios/study_llm_budget_equivalence.yaml \
  -o out/llm_budget_equivalence \
  --checkpoint-dir out/llm_budget_equivalence.checkpoints \
  --resume \
  --no-figures
PYTHONPATH=. .venv/bin/python scripts/summarize_llm_interventions.py \
  out/llm_budget_equivalence
```

The current run contains 45 completed runs. Sparse OpenAI guidance remains
highly efficient, but static-equivalent and entropy-equivalent generated
budgets substantially reduce ISE. The equal-budget variants descriptively lower
HCI relative to sparse LLM guidance, but they do not dominate the conservative
deterministic baselines.

After the LLM studies have been summarized, regenerate the combined synthesis,
seed-level statistical comparisons, and cache-usage audit:

```sh
PYTHONPATH=. .venv/bin/python scripts/synthesize_llm_results.py \
  -o out/llm_synthesis
PYTHONPATH=. .venv/bin/python scripts/compare_llm_claims.py \
  -o out/llm_synthesis
PYTHONPATH=. .venv/bin/python scripts/audit_llm_cache_usage.py \
  --cache-root out/llm_cache \
  -o out/llm_synthesis
```

The audit script also accepts optional pricing parameters if cost estimates are
needed:

```sh
PYTHONPATH=. .venv/bin/python scripts/audit_llm_cache_usage.py \
  --cache-root out/llm_cache \
  -o out/llm_synthesis \
  --input-usd-per-mtok <input price> \
  --output-usd-per-mtok <output price>
```

Regenerate the LaTeX tables used by the LLM extension section with:

```sh
cd paper
../.venv/bin/python scripts/gen_llm_tables.py -o llm_tables.tex
```

This reads the medium LLM, target-selection, LLM regime robustness, and
deterministic regime robustness CSV artifacts, plus the prompt-objective,
budget-equivalence, and seed-level claim-statistics artifacts. The generated
`llm_tables.tex` includes all LLM tables used in
`paper/sections/limitations.tex`.

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
paper/figures/regime-robustness-heatmap.pdf
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
