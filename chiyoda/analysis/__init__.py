"""Analytics: metrics, telemetry, and report generation."""

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.analysis.trajectory_reference import (
    compare_trajectory_reference,
    load_trajectory_table,
    summarize_trajectory_frame,
)

__all__ = [
    "SimulationAnalytics",
    "compare_trajectory_reference",
    "load_trajectory_table",
    "summarize_trajectory_frame",
]
