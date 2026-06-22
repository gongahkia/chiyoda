# Summary

<!-- One paragraph: what changes and why. -->

## Type of change

- [ ] feat
- [ ] fix
- [ ] docs
- [ ] chore / build / ci
- [ ] refactor / perf / style
- [ ] test
- [ ] benchmark submission (see `docs/benchmark/submitting.md`)

## Test plan

- [ ] `make verify`
- [ ] `.venv/bin/python -m ruff check chiyoda tests scripts`
- [ ] `.venv/bin/python -m black --check chiyoda tests scripts`
- [ ] Benchmark smoke (if applicable): `.venv/bin/python -m pytest tests/test_benchmark.py tests/test_benchmark_v2_v3.py`

## Notes for reviewers

<!-- Open questions, alternatives considered, follow-up TODOs. -->
