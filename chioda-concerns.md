# Chiyoda Concerns

This document records the current engineering and product concerns identified
in a repository-local audit. It does not assess external validation, novelty,
or the validity of any underlying subject-matter claims.

## High: no universal CI quality gate

The repository has useful specialised workflows, but none runs the normal
test suite, lint, scenario audit, and package build for every pull request and
push to `main`.

- `make verify` runs only `pytest -m "not slow"`.
- The install workflow runs only editable-install and CLI-help smoke checks.
- RiMEA and benchmark checks cover selected areas; benchmark tests require a
  label.
- The reproducibility workflow is path-limited.
- The performance workflow does not replace functional testing.

`CONTRIBUTING.md` says `main` always passes `make verify`, but the checked-in
workflows do not demonstrate that claim. A single required CI workflow should
run the default test suite, formatter, linter, scenario audit, type check, and
package build for all relevant changes.

## High: strict typing has a large accepted error budget

`docs/typing_baseline.md` records 493 `mypy --strict` errors. The CI policy
only rejects an increased total, so it does not establish that modified code
or newly added modules are type-clean. Incrementally reducing the baseline and
enforcing strict cleanliness for changed modules would turn typing from a
no-regression metric into a dependable engineering control.

## Medium: delivery environments and documentation disagree

The Docker documentation says that the runtime image excludes pytest and
developer tooling, but `requirements-lock.txt` includes both `pytest` and
`black`. Docker uses this lockfile, while local setup and CI resolve from
`requirements.txt` and `requirements-dev.txt`. The direct dependencies are
pinned, but the environment contract is not consistently applied.

The Docker image also runs copied source directly rather than installing the
package artifact. Editable installation is smoke-tested separately, but the
image is not a test of the built wheel or source distribution.

## Medium: supply-chain and security posture is incomplete

GitHub Actions are referenced by mutable major-version tags such as `@v4` and
`@v5`, rather than immutable commit SHAs. The workflows do not declare
job-level `permissions`; the effective token scope consequently depends on
repository settings. No repository-local security policy or automated
dependency-update configuration is present.

## Medium: the front page is not sufficiently curated for a product audience

The README advertises a broad simulator but opens with a personal hypothesis
instead of a concrete user, workflow, and outcome. Its research bibliography
contains duplicate entries and material that does not match the stated
evacuation/information-control focus. This is a documentation-quality concern:
it makes the repository appear less deliberate than the implementation.

The browser viewer's constrained role is well documented elsewhere: it is a
single-floor, replay-seeded preview with no hazard-physics or multi-floor
parity. The README should surface this boundary near its viewer claim so users
do not mistake the preview for the reference simulator.

## Low: one CLI command masks failures

`chiyoda generate` catches every exception while attempting to import and run
its generator, then silently writes a basic fallback layout. This makes a
generator bug indistinguishable from an unavailable optional feature. It
should catch only expected import/availability errors and report unexpected
failures.

## Product scope and maintenance pressure

The project combines hazard simulation, crowd movement, information spread,
hostile channels, optional LLM generation, scenario import, studies,
benchmarks, exports, and a viewer. This demonstrates strong systems breadth,
but it also makes the default product identity unclear. The repository's own
modeling-gap and implementation-audit documents do a good job of drawing
boundaries; the main product narrative should make the same narrowing visible
immediately.

The checked-in calibration data, reference data, and baseline artifacts appear
defensible because they support named workflows. They should remain explicitly
inventoried and tied to those workflows so the repository stays reviewable as
the data footprint grows.

