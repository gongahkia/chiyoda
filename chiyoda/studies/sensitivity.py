"""
Parametric sensitivity analysis for ITED simulations.

Sweeps one parameter at a time across a value range, runs the simulation
for each value × N seeds, and collects output metrics. Computes elasticity
(Δmetric/Δparam) and monotonicity for each parameter-metric pair.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple
import copy
import numpy as np
import pandas as pd

from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.analysis.metrics import SimulationAnalytics


@dataclass
class SensitivityResult:
    """Result of a single-parameter sweep."""
    parameter_path: str
    parameter_values: List[Any]
    seeds: List[int]
    metrics: pd.DataFrame  # columns: param_value, seed, metric_name, metric_value
    summary: pd.DataFrame  # columns: param_value, metric_name, mean, std, min, max

    def elasticity(self, metric_name: str) -> List[float]:
        """
        Compute local elasticity (Δmetric/Δparam) at each interval.

        Returns list of len(parameter_values) - 1 elasticities.
        """
        sub = self.summary[self.summary["metric_name"] == metric_name].sort_values("param_value")
        if len(sub) < 2:
            return []
        values = sub["param_value"].values.astype(float)
        means = sub["mean"].values.astype(float)
        elasticities = []
        for i in range(len(values) - 1):
            dp = values[i + 1] - values[i]
            dm = means[i + 1] - means[i]
            if abs(dp) > 1e-12:
                elasticities.append(dm / dp)
            else:
                elasticities.append(0.0)
        return elasticities

    def is_monotonic(self, metric_name: str) -> Optional[bool]:
        """
        Check if the metric response is monotonic across the parameter range.

        Returns True (monotone increasing), False (monotone decreasing),
        or None (non-monotonic).
        """
        e = self.elasticity(metric_name)
        if not e:
            return None
        if all(x >= 0 for x in e):
            return True
        if all(x <= 0 for x in e):
            return False
        return None


class SensitivitySweep:
    """
    Run a one-at-a-time parametric sweep on a base scenario.

    Usage:
        sweep = SensitivitySweep(
            scenario_file="scenarios/station_sarin.yaml",
            parameter_path="information.decay_rate",
            values=[0.001, 0.005, 0.01, 0.02, 0.05],
            seeds=[42, 137, 256],
        )
        result = sweep.run()
    """

    def __init__(
        self,
        scenario_file: str,
        parameter_path: str,
        values: List[Any],
        seeds: List[int] = None,
        metric_names: List[str] = None,
    ) -> None:
        self.scenario_file = scenario_file
        self.parameter_path = parameter_path
        self.values = values
        self.seeds = seeds or [42]
        self.metric_names = metric_names or [
            "mean_travel_time_s", "agents_evacuated", "agents_incapacitated",
            "mean_entropy", "entropy_reduction", "peak_mean_density",
            "mean_fd_speed",
        ]
        self._mgr = ScenarioManager()
        self._analytics = SimulationAnalytics()

    def _set_nested(self, d: Dict, path: str, value: Any) -> Dict:
        """Set a nested dict value by dotted path. Returns modified copy."""
        result = copy.deepcopy(d)
        keys = path.split(".")
        current = result
        for key in keys[:-1]:
            if key not in current:
                current[key] = {}
            current = current[key]
        current[keys[-1]] = value
        return result

    def run(self, progress_callback: Callable[[str], None] = None) -> SensitivityResult:
        """Execute the sweep and return results."""
        all_rows: List[Dict[str, Any]] = []

        for val in self.values:
            for seed in self.seeds:
                if progress_callback:
                    progress_callback(f"{self.parameter_path}={val}, seed={seed}")

                override = self._set_nested({}, self.parameter_path, val)
                try:
                    sim = self._mgr.load_scenario(
                        self.scenario_file,
                        overrides=override,
                        random_seed=seed,
                    )
                    sim.run()
                    metrics = self._analytics.calculate_performance_metrics(sim)
                except Exception as e:
                    metrics = {m: float("nan") for m in self.metric_names}
                    metrics["_error"] = str(e)

                for metric_name in self.metric_names:
                    all_rows.append({
                        "param_value": val,
                        "seed": seed,
                        "metric_name": metric_name,
                        "metric_value": float(metrics.get(metric_name, 0.0)),
                    })

        metrics_df = pd.DataFrame(all_rows)
        summary_df = (
            metrics_df.groupby(["param_value", "metric_name"])["metric_value"]
            .agg(["mean", "std", "min", "max"])
            .reset_index()
        )

        return SensitivityResult(
            parameter_path=self.parameter_path,
            parameter_values=self.values,
            seeds=self.seeds,
            metrics=metrics_df,
            summary=summary_df,
        )


def run_multi_sweep(
    scenario_file: str,
    sweeps: Dict[str, List[Any]],
    seeds: List[int] = None,
    metric_names: List[str] = None,
    progress_callback: Callable[[str], None] = None,
) -> Dict[str, SensitivityResult]:
    """
    Run multiple one-at-a-time sweeps.

    Args:
        scenario_file: base scenario YAML
        sweeps: {parameter_path: [values]}
        seeds: random seeds for repetition
        metric_names: metrics to collect

    Returns:
        {parameter_path: SensitivityResult}
    """
    results = {}
    for param_path, values in sweeps.items():
        sweep = SensitivitySweep(
            scenario_file=scenario_file,
            parameter_path=param_path,
            values=values,
            seeds=seeds,
            metric_names=metric_names,
        )
        results[param_path] = sweep.run(progress_callback=progress_callback)
    return results
