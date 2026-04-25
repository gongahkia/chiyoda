"""
Algorithms for detecting emergent behaviors in ITED simulations.

Detects macro-level phenomena arising from micro-level agent interactions:
1. Faster-Is-Slower (FIS) effect
2. Herding-Induced Bottleneck Amplification
3. Information Cascades
4. Counter-Flow Lane Formation
"""
from __future__ import annotations
import numpy as np
import pandas as pd
from typing import Dict, Any

from chiyoda.studies.models import StudyBundle

def _representative_run_id(bundle: StudyBundle) -> str:
    return str(bundle.metadata.get("representative_run_id", "baseline"))


def detect_faster_is_slower(bundle: StudyBundle) -> Dict[str, Any]:
    """
    Detect Faster-Is-Slower effect at bottlenecks.
    
    FIS occurs when high pressure/density at a bottleneck reduces the actual 
    outflow due to arching and friction.
    
    Returns:
        Dict with fis_score (correlation between queue length and dwell time),
        and whether it exceeds the detection threshold.
    """
    bn = bundle.bottlenecks.copy()
    if bn.empty:
        return {"detected": False, "fis_score": 0.0, "reason": "No bottleneck telemetry"}

    run_id = _representative_run_id(bundle)
    b_run = bn[bn["run_id"] == run_id]
    
    if b_run.empty or len(b_run) < 10:
        return {"detected": False, "fis_score": 0.0, "reason": "Insufficient data"}
        
    # FIS implies queue_length strongly predicts mean_dwell_s with a positive correlation
    # Check correlation between queue length and mean dwell time (lagged if possible, but static is okay for index)
    try:
        corr = b_run["queue_length"].corr(b_run["mean_dwell_s"])
        if pd.isna(corr):
            corr = 0.0
    except Exception:
        corr = 0.0
        
    # High positive correlation (>0.7) suggests severe congestion causing disproportionate delays
    # True FIS also implies outflow drops when queue increases
    try:
        flow_corr = b_run["queue_length"].corr(b_run["outflow"])
        if pd.isna(flow_corr):
            flow_corr = 0.0
    except Exception:
        flow_corr = 0.0

    # Strong FIS index: High wait times AND dropping outflow under pressure
    fis_score = (corr * 0.5) + (-flow_corr * 0.5)
    
    return {
        "detected": bool(fis_score > 0.4),
        "fis_score": float(fis_score),
        "queue_dwell_corr": float(corr),
        "queue_outflow_corr": float(flow_corr),
    }


def detect_herding(bundle: StudyBundle) -> Dict[str, Any]:
    """
    Detect Herding-Induced Bottleneck Amplification.
    
    Occurs when agents ignore closer exits to follow the crowd, causing
    imbalanced exit utilization.
    """
    agents = bundle.agents.copy()
    if agents.empty or "evacuated_via" not in agents.columns:
        return {"detected": False, "herding_index": 0.0}
        
    run_id = _representative_run_id(bundle)
    a_run = agents[(agents["run_id"] == run_id) & (agents["evacuated"] == True)]
    
    if a_run.empty:
        return {"detected": False, "herding_index": 0.0}
        
    # Measure utilization imbalance (Gini coefficient of exit usage)
    exit_counts = a_run["evacuated_via"].value_counts().values
    if len(exit_counts) <= 1:
        return {"detected": False, "herding_index": 0.0, "reason": "Only 1 exit used"}
        
    # Calculate Gini coefficient
    exit_counts = np.sort(exit_counts)
    n = len(exit_counts)
    cumulative = np.cumsum(exit_counts)
    gini = (n + 1 - 2 * np.sum(cumulative) / cumulative[-1]) / n
    
    # Very high Gini (> 0.6) indicates severe imbalance, characteristic of herding
    return {
        "detected": bool(gini > 0.6),
        "herding_index": float(gini),
    }


def detect_information_cascade(bundle: StudyBundle) -> Dict[str, Any]:
    """
    Detect Information Cascades.
    
    Rapid, non-linear adoption of beliefs across the population.
    Measured via the peak derivative of entropy reduction.
    """
    steps = bundle.steps.copy()
    if steps.empty or "global_entropy" not in steps.columns:
        return {"detected": False, "max_cascade_rate": 0.0}
        
    run_id = _representative_run_id(bundle)
    s_run = steps[steps["run_id"] == run_id].sort_values("step")
    
    if len(s_run) < 5:
        return {"detected": False, "max_cascade_rate": 0.0}
        
    # Calculate rate of entropy reduction (smoothed)
    entropy = s_run["global_entropy"].rolling(window=5, min_periods=1).mean().values
    time_s = s_run["time_s"].values
    
    # Derivative dH/dt
    dt = np.diff(time_s)
    dh = np.diff(entropy)
    
    # Avoid division by zero
    valid = dt > 0
    if not np.any(valid):
        return {"detected": False, "max_cascade_rate": 0.0}
        
    rates = np.zeros_like(dt)
    rates[valid] = -dh[valid] / dt[valid]  # Positive rate means entropy is decreasing
    
    max_rate = float(np.max(rates)) if len(rates) > 0 else 0.0
    
    # Cascade detection: sharp drop in entropy (e.g. > 0.1 nats per second)
    return {
        "detected": bool(max_rate > 0.1),
        "max_cascade_rate": max_rate,
    }


def detect_lane_formation(bundle: StudyBundle) -> Dict[str, Any]:
    """
    Detect Counter-Flow Lane Formation.
    
    Calculates polarization order parameter. When two groups move in opposite
    directions, they spontaneously form lanes to minimize friction.
    """
    # This is complex to calculate purely from macroscopic grids. 
    # We approximate it by checking path_usage_grid variance along cross-sections.
    cells = bundle.cells.copy()
    if cells.empty:
        return {"detected": False, "lane_index": 0.0}
        
    run_id = _representative_run_id(bundle)
    c_run = cells[cells["run_id"] == run_id]
    
    if c_run.empty:
        return {"detected": False, "lane_index": 0.0}
        
    # Use path usage to detect spatial clustering of trajectories
    # A high variance in path usage across a corridor indicates discrete lanes
    # Aggregate path usage over time to get the final spatial map
    spatial_usage = c_run.groupby(["x", "y"])["path_usage"].max().reset_index()
    
    if spatial_usage.empty:
        return {"detected": False, "lane_index": 0.0}
        
    # Calculate coefficient of variation of path usage
    mean_usage = spatial_usage["path_usage"].mean()
    if mean_usage < 1e-5:
        return {"detected": False, "lane_index": 0.0}
        
    cv = spatial_usage["path_usage"].std() / mean_usage
    
    # High CV indicates traffic is concentrated in specific "lanes" rather than uniform
    return {
        "detected": bool(cv > 1.5),
        "lane_index": float(cv),
    }

def run_all_emergence_detectors(bundle: StudyBundle) -> Dict[str, Dict[str, Any]]:
    """Run all emergent behavior detectors on a StudyBundle."""
    return {
        "faster_is_slower": detect_faster_is_slower(bundle),
        "herding": detect_herding(bundle),
        "information_cascade": detect_information_cascade(bundle),
        "lane_formation": detect_lane_formation(bundle),
    }
