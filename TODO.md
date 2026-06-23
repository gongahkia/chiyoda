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

### [ ] T3.1 In-browser sim loop (single-floor first)
- Scope: port the core stepper to a WASM target or JS port for a constrained subset (single floor, ≤200 agents, no LLM). Goal: viewer becomes interactive, not replay-only.
- Files: new `chiyoda/acceleration/wasm/` (or `viewer/js/sim/`), wiring in `chiyoda/analysis/viewer.py`, `docs/3d_viewer_simulation.md` (already exists — extend).
- Acceptance:
  - Viewer can run a 60s scenario locally in browser at ≥10 steps/s on a mid-range laptop. [Speculation] perf target — confirm with profiling.
  - Round-trip parity: browser run vs CLI run match within 5% on egress count for ≥3 small scenarios.
- Refs: web-based JuPedSim browser sim (Forschungszentrum Jülich).

### [ ] T3.2 Browser authoring of connectors and hostile channels
- Scope: extend viewer author mode to create/edit connectors (stairs, ramps, escalators, elevators) and hostile-channel configs. Currently only raster floor painting works (`docs/implementation_audit.md`).
- Files: `chiyoda/analysis/viewer.py`, viewer JS assets.
- Acceptance:
  - User can drag two cells across floors to author a connector with type and capacity.
  - User can drop a hostile-channel actor with objective, target population, credibility.
  - Exported YAML round-trips through `validate-scenario`.

### [ ] T3.3 Source-preserving scenario patch on export
- Scope: viewer currently exports a minimal runnable YAML, not a patch onto the source scenario. Track an `_origin` pointer plus diff.
- Files: `chiyoda/scenarios/` loader, viewer export pipeline.
- Acceptance:
  - Exported file contains `origin.path`, `origin.sha256`, and `patch.ops` (RFC 6902-style).
  - Re-applying patch to original yields byte-identical (modulo timestamps) runnable scenario.

### [ ] T3.4 Dispatcher / HITL panel
- Scope: add a viewer panel for a human operator to author a message during a paused run, see projected belief / HCI / exposure deltas BEFORE committing.
- Files: viewer JS, new `chiyoda/information/projection.py` (lightweight forward model for short horizon).
- Acceptance:
  - Operator workflow demoed in `docs/dispatcher_demo.md` (new) with screenshots.
  - Projection runs in <500ms on baseline scenario. [Speculation] perf target.
- Refs: AnyLogic metro DT crowd management case; InControl simulation DT.

### [ ] T3.5 Ops-shaped policy comparison brief
- Scope: alongside research figures, emit a 1-page "policy brief" PDF/MD per comparison: time-to-clear, expected exposure, attacker-induced harm delta (with CI), recommended policy.
- Files: `chiyoda/analysis/reports.py`, templates under `templates/`.
- Acceptance:
  - `chiyoda compare` produces both research figures AND `policy_brief.md` by default.
  - Brief is < 1 page, no jargon, includes uncertainty.

---

## P4 — Standards & data ingestion

### [ ] T4.1 First-class GTFS Pathways indoor importer
- Scope: GeoJSON converter is "pragmatic, not standards-complete" (`docs/implementation_audit.md`). Promote GTFS Pathways indoor to first-class — full level/pathway/node coverage.
- Files: new `chiyoda/environment/gtfs_pathways.py`, CLI `chiyoda convert-gtfs <dir> <out.yaml>`.
- Acceptance:
  - One real station feed converted (e.g., a published GTFS Pathways sample).
  - Level/pathway IDs preserved in scenario metadata.
- Refs: GTFS Pathways spec.

### [ ] T4.2 IFC import promoted from optional
- Scope: optional IFC importer (commit `caa5658`) is the right primitive — wire to scenario validation, role inference, and viewer.
- Files: `chiyoda/environment/layout.py`, IFC importer module.
- Acceptance:
  - IFC → `layout.floors` round-trip preserves spaces, walls, doors with `role` tags inferred.
  - At least one open IFC sample (e.g., Building Smart sample files) ships under `data/ifc_samples/`.
- Refs: buildingSMART IFC4 spec.

### [ ] T4.3 OSM indoor + level= tag full support
- Scope: OSM converter is a "pragmatic bridge". Cover full `indoor=*` and `level=*` tag grammar.
- Files: `chiyoda/environment/obstacles.py` (or OSM-specific converter), tests with real OSM extract.
- Acceptance:
  - Multi-level OSM extract converts without manual fix-ups for at least one named station.

---

## P5 — Calibration

### [ ] T5.1 Vision-calibrated social force defaults
- Scope: replace generic SFM defaults with a published vision-calibrated parameter set (e.g., YOLOv5-derived, MDPI Sensors 2024). Tag parameters with provenance.
- Files: `chiyoda/navigation/` SFM module, new `data/sfm_calibrations/`.
- Acceptance:
  - At least 2 calibration sets selectable from YAML (`generic_legacy`, `yolov5_mdpi_2024`).
  - Sensitivity test documents per-parameter Δ on a baseline scenario.
- Refs: MDPI Sensors 2024 (SFM YOLOv5 calibration); Physica A 2024 counterflow SFM.

### [ ] T5.2 Population calibration against station observations
- Scope: extend toy population calibration to ingest at least one real source (turnstile counts, GTFS-derived demand, or published CCTV densities).
- Files: `chiyoda/information/route_choice_calibration.py`, `scripts/run_toy_calibrations.py` → `scripts/run_population_calibration.py`.
- Acceptance:
  - One station feed calibrated; calibration residuals reported.
  - Doc `docs/population_calibration.md` updated with the new pipeline.
- Refs: Nature Cities 2025 mobile-data DT; existing `docs/generated_population_calibration.md`.

### [ ] T5.3 Counterflow + visual-range extensions to SFM
- Scope: add counterflow avoidance and limited visual range terms (cited 2024 critiques of vanilla SFM).
- Files: `chiyoda/navigation/` SFM module.
- Acceptance:
  - Unit test demonstrates lane formation in a corridor scenario.
  - Visual range parameter overridable per scenario.
- Refs: Physica A 2024 counterflow SFM; ScienceDirect S037843712300016X (visual range SFM).

---

## P6 — Telemetry, causality, and analysis

### [ ] T6.1 Causal counterfactual export
- Scope: HCI is reported as correlation. Add a counterfactual run pair per intervention (with vs without the message), and export the causal delta with CIs.
- Files: `chiyoda/analysis/metrics.py`, `chiyoda/studies/`, `docs/causal_layer_assumptions.md`.
- Acceptance:
  - `chiyoda run --counterfactual` produces both runs and a `causal_delta.json` per intervention.
  - Doc explicitly lists assumptions (no-interference, SUTVA approximation, etc.) and labels them `[Inference]`.

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
