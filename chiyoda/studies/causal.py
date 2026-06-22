"""Matched-pair causal estimators over exported study bundles."""

from __future__ import annotations

from collections.abc import Iterable, Sequence
from dataclasses import dataclass

import numpy as np
import pandas as pd

from chiyoda.studies.models import StudyBundle


@dataclass(frozen=True)
class CounterfactualEstimator:
    bootstrap_samples: int = 1000
    ci: float = 0.95
    random_seed: int = 42

    def compare(
        self,
        baseline: StudyBundle,
        treated: StudyBundle,
        *,
        metrics: Sequence[str],
        estimator: str = "ate",
    ) -> pd.DataFrame:
        if estimator != "ate":
            raise ValueError("Only estimator='ate' is currently supported")
        rows = [self._estimate_metric(baseline, treated, metric) for metric in metrics]
        return pd.DataFrame([row for row in rows if row is not None])

    def _estimate_metric(
        self,
        baseline: StudyBundle,
        treated: StudyBundle,
        metric: str,
    ) -> dict[str, float | int | str] | None:
        base = _run_metric_by_seed(baseline.summary, metric)
        test = _run_metric_by_seed(treated.summary, metric)
        pairs = base.join(test, how="inner", lsuffix="_baseline", rsuffix="_treated")
        if pairs.empty:
            return None
        diffs = pairs[f"{metric}_treated"].to_numpy(dtype=float) - pairs[
            f"{metric}_baseline"
        ].to_numpy(dtype=float)
        ate = float(np.mean(diffs))
        ci_low, ci_high = self._bootstrap_ci(diffs)
        sensitivity = _seed_sensitivity(diffs)
        baseline_mean = float(
            np.mean(pairs[f"{metric}_baseline"].to_numpy(dtype=float))
        )
        treated_mean = float(np.mean(pairs[f"{metric}_treated"].to_numpy(dtype=float)))
        return {
            "metric": metric,
            "estimator": "ate",
            "n_pairs": int(len(pairs)),
            "baseline_mean": baseline_mean,
            "treated_mean": treated_mean,
            "ate": ate,
            "ci_lower": ci_low,
            "ci_upper": ci_high,
            "seed_sensitivity_min": sensitivity["min"],
            "seed_sensitivity_max": sensitivity["max"],
            "seed_sensitivity_max_abs_shift": sensitivity["max_abs_shift"],
            "e_value": _e_value_from_means(baseline_mean, treated_mean),
        }

    def _bootstrap_ci(self, diffs: np.ndarray) -> tuple[float, float]:
        if len(diffs) == 0:
            return 0.0, 0.0
        if len(diffs) == 1 or self.bootstrap_samples <= 0:
            value = float(np.mean(diffs))
            return value, value
        rng = np.random.default_rng(self.random_seed)
        samples = rng.choice(
            diffs, size=(self.bootstrap_samples, len(diffs)), replace=True
        )
        means = samples.mean(axis=1)
        alpha = (1.0 - self.ci) / 2.0
        return (
            float(np.percentile(means, alpha * 100.0)),
            float(np.percentile(means, (1.0 - alpha) * 100.0)),
        )


def compare_bundles(
    baseline: StudyBundle,
    treated: StudyBundle,
    *,
    metrics: Iterable[str],
    estimator: str = "ate",
    bootstrap_samples: int = 1000,
    random_seed: int = 42,
) -> pd.DataFrame:
    return CounterfactualEstimator(
        bootstrap_samples=bootstrap_samples,
        random_seed=random_seed,
    ).compare(
        baseline,
        treated,
        metrics=list(metrics),
        estimator=estimator,
    )


def _run_metric_by_seed(summary: pd.DataFrame, metric: str) -> pd.DataFrame:
    if metric not in summary.columns:
        raise ValueError(f"Metric not found in summary: {metric}")
    frame = summary.copy()
    if "record_type" in frame.columns:
        frame = frame[frame["record_type"] == "run"].copy()
    if "seed" not in frame.columns:
        raise ValueError(
            "Study summary must contain seed for matched-pair causal comparison"
        )
    if frame.empty:
        return pd.DataFrame(columns=[metric]).rename_axis("seed")
    return (
        frame[["seed", metric]]
        .dropna()
        .assign(seed=lambda item: item["seed"].astype(int))
        .groupby("seed", as_index=True)[metric]
        .mean()
        .to_frame(metric)
    )


def _seed_sensitivity(diffs: np.ndarray) -> dict[str, float]:
    ate = float(np.mean(diffs)) if len(diffs) else 0.0
    if len(diffs) <= 1:
        return {"min": ate, "max": ate, "max_abs_shift": 0.0}
    estimates = [float(np.mean(np.delete(diffs, index))) for index in range(len(diffs))]
    return {
        "min": float(min(estimates)),
        "max": float(max(estimates)),
        "max_abs_shift": float(max(abs(value - ate) for value in estimates)),
    }


def _e_value_from_means(baseline_mean: float, treated_mean: float) -> float:
    if baseline_mean <= 0 or treated_mean <= 0:
        return 1.0
    ratio = max(treated_mean / baseline_mean, baseline_mean / treated_mean)
    if ratio < 1.0:
        return 1.0
    return float(ratio + np.sqrt(ratio * max(0.0, ratio - 1.0)))
