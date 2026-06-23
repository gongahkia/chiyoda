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

---

## P7 — Reproducibility, CI, performance

---

## P8 — LLM layer hardening

---

## P9 — Documentation and positioning

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
