# Generated benchmark protocol

Chiyoda evaluates runtime generalization with a deterministic constraint-based
generator rather than a permanently fixed set of scenarios. The generator is
not a source of empirical truth: it produces only structurally valid candidate
experiments.

## Round lifecycle

1. Publish a generator version, constraints, fixture seeds, evidence manifest,
   scoring code, and calibration split.
2. Commit the SHA-256 hash of the held-out evaluation seed list before results
   are accepted.
3. Run the reference and submitted runtimes against the sealed seed list.
4. Publish artifacts, scores, seed list, and verification instructions at the
   end of the round.
5. Start the next round with a new committed seed list.

This permits genuine held-out evaluation while making every completed round
independently reproducible. Hosted-only permanent secret evaluation is out of
scope.

## Current status

`0.14.0-alpha.1` includes the generator, fixture-seed protocol, and manifest
validator. It now also includes a content-locked candidate source and a
descriptive 2D platform-trajectory intake path. It does not publish an
empirical round: the source is neither a calibrated runtime nor independent
facility/primitives validation. See the [evidence boundary](evidence.md).

## Uncalibrated structural sweeps

`chiyoda sweep --seed FIRST --count COUNT -o DIRECTORY` generates and runs a
contiguous seed range without consulting any evidence catalog or benchmark
manifest. It writes canonical source and a hash-verifiable run bundle under
`seed-SEED/` for every run, plus `summary.json` listing each bundle hash and
basic outcome metrics. `chiyoda verify-sweep DIRECTORY` cross-checks that
summary against every bundle hash, metric, and canonical source. The supplied
output directory must be empty so an existing experiment artifact cannot be
silently overwritten.

This command supports structural exploration and regression investigation. It
does not produce a benchmark score, calibration result, or predictive claim.
