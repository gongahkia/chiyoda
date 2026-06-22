# Typing Baseline

This file records the current `mypy --strict` error count for `chiyoda/`.
New code should not regress against this baseline; CI gates on no-regression.

## Current Baseline

Run from repo root with `.venv` activated:

```sh
.venv/bin/python -m mypy chiyoda
```

| Date | mypy version | Files checked | Errors |
|:--|:--|:--|:--|
| 2026-06-22 | 2.1.0 | 60 | 389 |

The baseline is intentionally non-zero. The runtime predates strict typing
and we are not adopting strict typing project-wide in one commit. New
modules should aim for strict-clean; existing modules can be migrated
incrementally.

## Policy

- New modules: must add type annotations and pass `mypy --strict`.
- Modified modules: prefer to fix typing locally before merging.
- CI fails if the total error count increases relative to this baseline.

## Configuration

Strict knobs live in `pyproject.toml`:

```toml
[tool.mypy]
python_version = "3.12"
strict = true
warn_unused_ignores = true
ignore_missing_imports = true
```
