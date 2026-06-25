# Contributing

Thanks for considering a contribution. Chiyoda is a research-oriented
evacuation/information simulator; we keep the bar for replayability and
typing high, but the change process is light.

## Branch Model

- `main` is the integration branch and always passes `make verify`.
- Topic branches: `feat/<short-slug>`, `fix/<short-slug>`, `docs/<short-slug>`,
  `chore/<short-slug>`.
- Open a PR against `main`. Rebase or squash-merge is preferred over a
  merge commit unless the change is a multi-author collaboration.

## Commit Messages

Conventional Commits are accepted but not required. Prefixes we use:
`feat`, `fix`, `docs`, `chore`, `build`, `ci`, `refactor`, `perf`,
`style`, `test`.

Use the imperative mood. Keep the subject under 72 characters. Body lines
should explain *why* the change is needed, not *what* the diff already shows.

## Local Test Policy

Before opening a PR:

```sh
make venv                # one time
make verify              # runs pytest
make lint PYTHON=.venv/bin/python
```

Typing is tracked by the CI no-regression baseline in
`docs/typing_baseline.md`; run `make typecheck PYTHON=.venv/bin/python` when
touching typed surfaces.

If you touched the benchmark layer, also run:

```sh
.venv/bin/python -m pytest tests/test_benchmark.py tests/test_benchmark_v2_v3.py
.venv/bin/python -m chiyoda.cli benchmark submit --suite v1 -o out/benchmark_submission_smoke
```

## Pre-commit Setup

Pre-commit runs black, ruff, codespell, and a few hygiene hooks. Install
it once:

```sh
make precommit
```

The `precommit` Makefile target installs hooks and runs them against the
full tree.

## Benchmark Submissions

See `docs/benchmark/submitting.md` for the submission flow, the
`benchmark-submission` label, and the leaderboard append protocol.
