# Chiyoda TODO

Actionable backlog of functional changes to move Chiyoda from a research artifact toward a genuinely novel, externally credible, applied tool. Grouped by priority block. Each item is sized for an independent coding agent with web-search access; per-item: scope, files, acceptance, refs.

Conventions:
- `[ ]` open, `[x]` done. Sub-bullets are mandatory acceptance checks.
- Tags: `[Inference]`, `[Speculation]`, `[Unverified]` when a claim is not directly sourced.
- Touch only the listed files plus minimal supporting modules; do not auto-refactor.
- Add tests under `tests/` mirroring the package path. Keep YAML scenarios under `scenarios/` strict (`layout.floors` + connectors).
- Every new external claim in docs MUST cite a primary source (paper, dataset, standard).

---

## P0 — Core novelty: ground the information layer

## P1 — Validation: drop the "not externally validated" caveat

---

## P2 — Benchmark v1 → real leaderboard

---

## P3 — Authoring & ops surfaces (non-CLI users)

---

## P4 — Standards & data ingestion

---

## P5 — Calibration

---

## P6 — Telemetry, causality, and analysis

### [ ] T6.2 Per-step route-intent path usage
- Scope: path-usage debug is aggregate max-per-cell (`implementation_audit.md`). Add per-step intent tensor; gate behind a CLI flag to control file size.
- Files: `chiyoda/analysis/telemetry.py`, viewer overlay loader.
- Acceptance:
  - Flag `--per-step-intent` works.
  - File size budget documented (e.g., compressed parquet under a stated cap).

### [ ] T6.3 Equity reporting beyond aggregate
- Scope: equity term in composite score is single-number. Add subgroup breakdowns (impaired, elderly, familiarity prior).
- Files: `chiyoda/analysis/metrics.py`, reports.
- Acceptance:
  - Per-subgroup metrics in study bundle.
  - Spec doc explains what each subgroup tag means.

---

## P7 — Reproducibility, CI, performance

### [ ] T7.1 Pin matplotlib (and other missing dev deps) into install path
- Scope: `python -m chiyoda.cli --help` currently fails without matplotlib (observed locally). Either lazy-import or pin in core requirements.
- Files: `pyproject.toml`, `requirements.txt`, `requirements-lock.txt`, CLI imports.
- Acceptance:
  - `pip install -e .` followed by `python -m chiyoda.cli --help` works on a clean venv.
  - CI job verifies this on Linux + macOS.

### [ ] T7.2 Reproducibility manifest hash audit on every release
- Scope: `docs/reproducibility_kit.md` exists. Add CI step that recomputes manifest and fails if drift.
- Files: `.github/workflows/`, `scripts/repro_audit.py` (new).
- Acceptance:
  - CI job on `main` recomputes scenario + benchmark hashes and posts a diff if mismatched.

### [ ] T7.3 Perf regression gating
- Scope: `scripts/perf_regression_suite.py` exists. Wire it into CI with explicit thresholds.
- Files: `.github/workflows/`, `docs/perf_baselines.md` (new).
- Acceptance:
  - CI publishes per-PR perf delta vs `main` baseline; >10% regression flags for review. [Speculation] threshold.

### [ ] T7.4 Determinism tests
- Scope: lock RNG paths; add a test that two runs with same seed and config produce byte-identical telemetry tables.
- Files: `tests/test_determinism.py` (new).
- Acceptance:
  - Test passes for ≥3 baseline scenarios across Python 3.10–3.12.

---

## P8 — LLM layer hardening

### [ ] T8.1 Threat-model coverage of LLM-MAS attacks
- Scope: extend `docs/llm_selective_controls.md` and `chiyoda/information/llm.py` to explicitly test AiTM and persuasion attacks within the bounded LLM pipeline.
- Files: `chiyoda/information/llm.py`, `chiyoda/information/llm_judge.py`, new `tests/test_llm_aitm.py`.
- Acceptance:
  - Red-team scenario YAML triggers an AiTM-style intercepted message and verifies validator catches it.
- Refs: AiTM (arXiv 2508.03125); Trustworthy LLM agents (arXiv 2503.09648).

### [ ] T8.2 Replay audit completeness
- Scope: `llm_calls` export exists. Add tamper-evident hashing (hash chain) per study.
- Files: `chiyoda/information/llm.py`, study bundle writer.
- Acceptance:
  - Each `llm_calls` row contains a cryptographic chain link; a corrupted row breaks verification.
  - CLI `chiyoda audit llm_calls <bundle>` reports verification result.

### [ ] T8.3 Provider-cost transparency
- Scope: add a per-study cost report (tokens, USD estimate, provider, model). Already partly there (budget guard) — surface in reports.
- Files: `chiyoda/analysis/reports.py`.
- Acceptance:
  - Cost summary appears in study bundle and policy brief.

---

## P9 — Documentation and positioning

### [ ] T9.1 Related-work table in paper outline
- Scope: `docs/paper_outline_info_warfare.md` lists "related work anchors" but does not contrast with JuPedSim, Vadere, BDI-bombing crowd model, psychology-driven LLM panic predictor, LLM-emergency policy work.
- Files: `docs/paper_outline_info_warfare.md`.
- Acceptance:
  - Side-by-side feature table (information layer, hostile channel, hazard model, validation, benchmark) vs ≥4 named systems.
  - Each row cited.
- Refs: JuPedSim, Vadere, Frontiers 2023 bombing BDI, arXiv 2505.16455, arXiv 2509.21868.

### [ ] T9.2 Threats-to-validity expansion
- Scope: current TTV list is short. Expand to cover calibration provenance, hazard fidelity, LLM nondeterminism, hostile-channel construct validity.
- Files: `docs/paper_outline_info_warfare.md`, `docs/causal_layer_assumptions.md`.
- Acceptance:
  - Each threat names a mitigation in the codebase or an explicit "unmitigated" label.

### [ ] T9.3 Policy / standards engagement note
- Scope: add a short doc framing Chiyoda's relevance to alerting authorities (FCC WEA rulemaking 2025, FEMA best practices, ICAO/UNDRR equivalents).
- Files: new `docs/policy_engagement.md`.
- Acceptance:
  - Document cites the open FCC docket and FEMA WEA best-practices page and states what Chiyoda outputs would be relevant to each.
- Refs: FCC WEA rulemaking 2025; FEMA WEA best practices.

### [ ] T9.4 Glossary
- Scope: many overloaded terms (entropy, HCI, belief, intent). Add a single glossary.
- Files: new `docs/glossary.md`.
- Acceptance:
  - Every term used in `paper_outline_info_warfare.md` and `architecture_overview.md` is defined once.

---

## Out of scope for this TODO

- Migrating off Python.
- Replacing social-force with a learned crowd model.
- Productizing as SaaS / hosted service.
- Real-time integration with operational alerting systems beyond data ingestion.

## How an agent should pick up work

1. Run `python -m chiyoda.cli --help` on a clean venv. If it fails, do T7.1 first.
2. Pick the lowest-numbered open item that has no unmet dependency on a higher-priority block (e.g., T1.* depends on T7.1).
3. Open a PR per task. Title format: `<area>: <task id> <short title>`. Example: `info: T0.1 padm-anchored belief update`.
4. PR description must include: scope delta, files changed, acceptance evidence (test names, doc paths, screenshots if viewer), and references with primary URLs.
5. Update this TODO.md in the same PR: tick the box, add a one-line note pointing at the PR.
