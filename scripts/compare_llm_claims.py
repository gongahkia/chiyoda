#!/usr/bin/env python3
"""Run seed-level statistical comparisons for LLM extension claims."""
from __future__ import annotations

import argparse
from pathlib import Path
from typing import Sequence

import numpy as np
import pandas as pd

from chiyoda.analysis.statistics import bootstrap_ci, cohens_d, mann_whitney_u


DEFAULT_STUDIES = {
    "prompt_objective": "out/llm_prompt_objective_ablation",
    "budget_equivalence": "out/llm_budget_equivalence",
}

DEFAULT_COMPARISONS = [
    {
        "claim": "sparse_safety_vs_static",
        "study": "prompt_objective",
        "baseline": "static_beacon",
        "test": "llm_openai_safety",
    },
    {
        "claim": "sparse_safety_vs_entropy",
        "study": "prompt_objective",
        "baseline": "entropy_targeted",
        "test": "llm_openai_safety",
    },
    {
        "claim": "sparse_safety_vs_bottleneck",
        "study": "prompt_objective",
        "baseline": "bottleneck_avoidance",
        "test": "llm_openai_safety",
    },
    {
        "claim": "hazard_prompt_vs_safety_prompt",
        "study": "prompt_objective",
        "baseline": "llm_openai_safety",
        "test": "llm_openai_hazard_avoidance",
    },
    {
        "claim": "anti_convergence_prompt_vs_safety_prompt",
        "study": "prompt_objective",
        "baseline": "llm_openai_safety",
        "test": "llm_openai_anti_convergence",
    },
    {
        "claim": "urgency_prompt_vs_safety_prompt",
        "study": "prompt_objective",
        "baseline": "llm_openai_safety",
        "test": "llm_openai_urgency",
    },
    {
        "claim": "static_budget_vs_sparse_llm",
        "study": "budget_equivalence",
        "baseline": "llm_openai_sparse",
        "test": "llm_openai_static_equivalent",
    },
    {
        "claim": "entropy_budget_vs_sparse_llm",
        "study": "budget_equivalence",
        "baseline": "llm_openai_sparse",
        "test": "llm_openai_entropy_equivalent",
    },
]

METRICS = [
    "information_safety_efficiency",
    "harmful_convergence_index",
    "agents_evacuated",
    "intervention_recipients",
]

LOWER_IS_BETTER = {"harmful_convergence_index", "intervention_recipients"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-o",
        "--out",
        default="out/llm_synthesis",
        help="Output directory for statistical comparison CSV artifacts.",
    )
    for name, default in DEFAULT_STUDIES.items():
        parser.add_argument(
            f"--{name.replace('_', '-')}",
            default=default,
            help=f"Study directory for {name}.",
        )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    study_dirs = {name: Path(getattr(args, name)) for name in DEFAULT_STUDIES}
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    rows = compare_default_claims(study_dirs)
    rows.to_csv(out_dir / "llm_claim_statistics.csv", index=False)
    print(f"wrote {out_dir / 'llm_claim_statistics.csv'}")
    return 0


def compare_default_claims(study_dirs: dict[str, Path]) -> pd.DataFrame:
    summaries = {
        name: _read_run_summary(study_dir)
        for name, study_dir in study_dirs.items()
        if (study_dir / "tables" / "summary.parquet").exists()
    }
    rows: list[dict[str, object]] = []
    for comparison in DEFAULT_COMPARISONS:
        study = comparison["study"]
        if study not in summaries:
            continue
        rows.extend(
            compare_variants_for_claim(
                summaries[study],
                claim=comparison["claim"],
                study=study,
                baseline=comparison["baseline"],
                test=comparison["test"],
                metrics=METRICS,
            )
        )
    return pd.DataFrame(rows).sort_values(["claim", "metric"])


def compare_variants_for_claim(
    summary: pd.DataFrame,
    *,
    claim: str,
    study: str,
    baseline: str,
    test: str,
    metrics: Sequence[str],
) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    base = summary[summary["variant_name"] == baseline].copy()
    candidate = summary[summary["variant_name"] == test].copy()
    if base.empty or candidate.empty:
        return rows
    for metric in metrics:
        if metric not in base.columns or metric not in candidate.columns:
            continue
        base_values = pd.to_numeric(base[metric], errors="coerce").dropna().to_numpy(dtype=float)
        test_values = pd.to_numeric(candidate[metric], errors="coerce").dropna().to_numpy(dtype=float)
        if len(base_values) == 0 or len(test_values) == 0:
            continue
        mw_stat, mw_p = mann_whitney_u(test_values, base_values)
        paired = _paired_values(base, candidate, metric)
        wilcoxon_stat, wilcoxon_p = _wilcoxon(paired)
        base_ci = bootstrap_ci(base_values)
        test_ci = bootstrap_ci(test_values)
        base_mean = float(np.mean(base_values))
        test_mean = float(np.mean(test_values))
        delta = test_mean - base_mean
        rows.append(
            {
                "claim": claim,
                "study": study,
                "metric": metric,
                "baseline_variant": baseline,
                "test_variant": test,
                "baseline_mean": base_mean,
                "baseline_ci_low": base_ci[0],
                "baseline_ci_high": base_ci[1],
                "test_mean": test_mean,
                "test_ci_low": test_ci[0],
                "test_ci_high": test_ci[1],
                "delta": delta,
                "better_direction": "lower" if metric in LOWER_IS_BETTER else "higher",
                "supports_test": delta < 0.0 if metric in LOWER_IS_BETTER else delta > 0.0,
                "mann_whitney_u": mw_stat,
                "mann_whitney_p": mw_p,
                "wilcoxon_stat": wilcoxon_stat,
                "wilcoxon_p": wilcoxon_p,
                "cohens_d": cohens_d(test_values, base_values),
                "n_baseline": len(base_values),
                "n_test": len(test_values),
                "n_paired": len(paired),
            }
        )
    return rows


def _read_run_summary(study_dir: Path) -> pd.DataFrame:
    summary = pd.read_parquet(study_dir / "tables" / "summary.parquet")
    if "record_type" not in summary.columns:
        return summary
    return summary[summary["record_type"] == "run"].copy()


def _paired_values(base: pd.DataFrame, test: pd.DataFrame, metric: str) -> np.ndarray:
    if "seed" not in base.columns or "seed" not in test.columns:
        return np.empty((0, 2))
    merged = base[["seed", metric]].merge(
        test[["seed", metric]],
        on="seed",
        suffixes=("_baseline", "_test"),
    )
    if merged.empty:
        return np.empty((0, 2))
    return merged[[f"{metric}_baseline", f"{metric}_test"]].to_numpy(dtype=float)


def _wilcoxon(paired: np.ndarray) -> tuple[float, float]:
    if len(paired) == 0:
        return 0.0, 1.0
    deltas = paired[:, 1] - paired[:, 0]
    if np.allclose(deltas, 0.0):
        return 0.0, 1.0
    try:
        from scipy.stats import wilcoxon

        stat, p_value = wilcoxon(deltas, alternative="two-sided")
        return float(stat), float(p_value)
    except Exception:
        return 0.0, 1.0


if __name__ == "__main__":
    raise SystemExit(main())
