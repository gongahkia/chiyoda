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

`0.1.0-alpha.1` includes the generator, fixture-seed protocol, and manifest
validator. It does not publish an empirical round because no qualifying public
calibration/held-out corpus has yet been ingested and reviewed.

