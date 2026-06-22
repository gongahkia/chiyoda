# Changelog

All notable changes to this project will be documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to semantic versioning where practical.

## [Unreleased]

### Added
- Benchmark suites v2 (`wildfire_wui`, `transit_shooter`) and v3
  (`flood_urban`, `quake_aftershock`) with shared composite_v1 scoring.
- `chiyoda/_logging.py` structured logger gated by `CHIYODA_LOG_FORMAT=json`.
- `pyproject.toml` with black/ruff/mypy configuration.
- `docs/typing_baseline.md` recording the current `mypy --strict` baseline.
- `requirements-lock.txt` pinned from the verified `.venv`.
- `CONTRIBUTING.md`, `CHANGELOG.md`, and `.github/ISSUE_TEMPLATE/*` templates.

### Changed
- `docs/implementation_audit.md` now documents wildfire, flood, aftershock,
  and shooter runtime paths.
- `README.md` hazard capability cell expanded to the full ten-kind set.

## [3.0.0] - 2026-06-21

### Added
- Benchmark v1 suite (`transit_cbrn`, `transit_shooter`, `transit_mixed`),
  composite_v1 scoring, reproducibility manifest, and leaderboard site.
- Hostile information channels with provenance schema and per-channel
  audit telemetry.
- LLM agent decisions and LLM-driven messaging with cache audit table,
  selective controls, and budget guard.
- Route-choice calibration pipeline (figshare 2024 records).
- 3D mobility, homophily, and connector queue models.
- Flood and aftershock hazard scenarios with terrain damage and
  re-evacuation wave mechanics.

### Changed
- Multi-floor layout is now the canonical scenario format; raster
  authoring exports strict `layout.floors`.
- Pathfinding traverses connector edges; validation extends across floors.

[Unreleased]: https://github.com/gongahkia/chiyoda/compare/v3.0.0...HEAD
[3.0.0]: https://github.com/gongahkia/chiyoda/releases/tag/v3.0.0
