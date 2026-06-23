# Submitting a Benchmark Policy

Chiyoda's benchmark layer is intentionally small and replayable. Community
submissions are welcome for suites `v1`, `v2`, and `v3`.

## Overview

A submission is a single PR that:

1. Adds an entry to the append-only `docs/benchmark/leaderboard.jsonl`.
2. Provides the reproducibility manifest, policy hash, and seeds that
   produced the entry.
3. Carries the `benchmark-submission` GitHub label so CI runs the
   benchmark smoke workflow.

## Required Fields

Submission JSON line (one per run, append to `docs/benchmark/leaderboard.jsonl`):

```json
{
  "submitter": "github_handle_or_team",
  "suite": "v1|v2|v3",
  "policy_path": "policies/my_policy.yaml",
  "policy_hash": "44136fa355b3678a",
  "mean_score": 62.34,
  "score_ci_low": 59.10,
  "score_ci_high": 65.42,
  "seeds": [42, 137],
  "seed_count": 2,
  "bootstrap_n": 1000,
  "tier": "smoke",
  "manifest_sha256": "...",
  "commit": "<short sha>",
  "submitted_at_utc": "2026-06-22T00:00:00Z"
}
```

The PR description should include:

- the command that produced the submission (typically
  `chiyoda benchmark submit --suite <v?> --policy policies/<file>.yaml`),
- the Python version and OS used,
- any non-default knobs touched (must be in `allowed_knobs` for the suite),
- a link to the policy YAML inside the PR.

## PR Template

Use the `Benchmark submission` issue template or write a PR body with:

```markdown
## Submission

- Suite: v1
- Policy hash: 44136fa355b3678a
- Mean composite score: 62.34 [59.10, 65.42]
- Seeds: 42, 137
- Tier: smoke
- Manifest hash: <sha256>

## Reproduction

\`\`\`sh
.venv/bin/python -m chiyoda.cli benchmark submit \
  --suite v1 \
  --policy policies/my_policy.yaml \
  -o out/benchmark_submission_pr
\`\`\`

## Notes

<optional context, ablations, surprises>
```

## CI

The `benchmark-smoke` workflow runs on PRs labeled `benchmark-smoke` or
`benchmark-submission`. It executes `tests/test_benchmark.py` and
`tests/test_benchmark_v2_v3.py`, then performs a `chiyoda benchmark submit`
smoke run, and uploads the resulting bundle as a workflow artifact.

Official benchmark submissions require at least 20 distinct seeds. The bundled
two-seed command is a smoke-tier reproducibility check.

## Leaderboard

`docs/benchmark/leaderboard.jsonl` is append-only. Maintain ordering by
date; do not edit prior entries. Removals require a follow-up entry with
`"superseded_by": "<commit-sha>"`.
