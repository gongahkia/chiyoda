"""
Statistical rigor module for ITED research publications.

Provides tools for evaluating simulation results with academic rigor:
- Bootstrap confidence intervals
- Mann-Whitney U non-parametric significance testing
- Cohen's d effect sizes
"""
from __future__ import annotations
import numpy as np
import pandas as pd
from typing import Dict, List, Tuple, Any

from chiyoda.studies.models import StudyBundle


def bootstrap_ci(data: np.ndarray, num_samples: int = 1000, ci: float = 0.95) -> Tuple[float, float]:
    """
    Calculate bootstrap confidence interval for the mean.
    
    Args:
        data: 1D array of observations
        num_samples: Number of bootstrap samples
        ci: Confidence level (0.0 to 1.0)
        
    Returns:
        (lower_bound, upper_bound)
    """
    if len(data) == 0:
        return (0.0, 0.0)
    if len(data) == 1:
        return (float(data[0]), float(data[0]))
        
    samples = np.random.choice(data, size=(num_samples, len(data)), replace=True)
    means = np.mean(samples, axis=1)
    
    alpha = (1.0 - ci) / 2.0
    lower = float(np.percentile(means, alpha * 100))
    upper = float(np.percentile(means, (1.0 - alpha) * 100))
    return (lower, upper)


def cohens_d(group1: np.ndarray, group2: np.ndarray) -> float:
    """
    Calculate Cohen's d effect size for two independent groups.
    d = (mean1 - mean2) / pooled_std
    """
    n1, n2 = len(group1), len(group2)
    if n1 == 0 or n2 == 0:
        return 0.0
        
    var1 = np.var(group1, ddof=1) if n1 > 1 else 0.0
    var2 = np.var(group2, ddof=1) if n2 > 1 else 0.0
    
    # Pooled standard deviation
    pooled_std = np.sqrt(((n1 - 1) * var1 + (n2 - 1) * var2) / (n1 + n2 - 2)) if n1 + n2 > 2 else 0.0
    
    if pooled_std == 0:
        return 0.0
        
    return float((np.mean(group1) - np.mean(group2)) / pooled_std)


def mann_whitney_u(group1: np.ndarray, group2: np.ndarray) -> Tuple[float, float]:
    """
    Perform Mann-Whitney U test (non-parametric).
    
    Returns:
        (statistic, p-value)
    """
    try:
        from scipy.stats import mannwhitneyu
        stat, p_val = mannwhitneyu(group1, group2, alternative='two-sided')
        return float(stat), float(p_val)
    except ImportError:
        # Fallback if scipy not available: return dummy values
        return 0.0, 1.0


def compare_variants(
    bundle: StudyBundle, 
    baseline_variant: str, 
    test_variant: str,
    metrics: List[str] = None
) -> pd.DataFrame:
    """
    Rigorous statistical comparison between two variants in a StudyBundle.
    
    Args:
        bundle: The StudyBundle containing multiple runs of variants
        baseline_variant: Name of the control/baseline variant
        test_variant: Name of the intervention variant
        metrics: List of column names from bundle.summary to compare
        
    Returns:
        DataFrame with rows for each metric, containing means, CIs, p-values, 
        and effect sizes.
    """
    if metrics is None:
        metrics = [
            "mean_travel_time_s", "agents_evacuated", "agents_incapacitated",
            "mean_entropy", "entropy_reduction", "peak_mean_density",
            "information_safety_efficiency", "harmful_convergence_index",
            "intervention_entropy_reduction", "intervention_accuracy_gain",
        ]
        
    summary = bundle.summary
    if summary.empty:
        return pd.DataFrame()
        
    base_data = summary[summary["variant_name"] == baseline_variant]
    test_data = summary[summary["variant_name"] == test_variant]
    
    if base_data.empty or test_data.empty:
        raise ValueError(f"Variants not found. Available: {summary['variant_name'].unique()}")
        
    results = []
    
    for metric in metrics:
        if metric not in base_data.columns:
            continue
            
        b_vals = base_data[metric].dropna().values.astype(float)
        t_vals = test_data[metric].dropna().values.astype(float)
        
        if len(b_vals) == 0 or len(t_vals) == 0:
            continue
            
        b_mean = float(np.mean(b_vals))
        t_mean = float(np.mean(t_vals))
        
        b_ci = bootstrap_ci(b_vals)
        t_ci = bootstrap_ci(t_vals)
        
        stat, p_val = mann_whitney_u(t_vals, b_vals)
        effect_size = cohens_d(t_vals, b_vals)
        
        results.append({
            "metric": metric,
            "baseline_mean": b_mean,
            "baseline_ci_lower": b_ci[0],
            "baseline_ci_upper": b_ci[1],
            "test_mean": t_mean,
            "test_ci_lower": t_ci[0],
            "test_ci_upper": t_ci[1],
            "diff_absolute": t_mean - b_mean,
            "diff_percent": ((t_mean - b_mean) / b_mean * 100) if b_mean != 0 else float('nan'),
            "p_value": p_val,
            "significant": p_val < 0.05,
            "cohens_d": effect_size,
            "n_baseline": len(b_vals),
            "n_test": len(t_vals),
        })
        
    return pd.DataFrame(results)
