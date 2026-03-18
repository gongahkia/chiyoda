from __future__ import annotations

from typing import Any, Dict

import numpy as np


class SimulationAnalytics:
    def calculate_performance_metrics(self, simulation) -> Dict[str, Any]:
        history = simulation.step_history
        travel_times = simulation.travel_times_s
        exit_usage = history[-1].exit_flow_cumulative if history else {}

        peak_queue = 0
        peak_throughput = 0
        peak_cell_occupancy = 0
        if history:
            peak_cell_occupancy = int(max(np.max(step.occupancy_grid) for step in history))
            for step in history:
                for metrics in step.bottlenecks.values():
                    peak_queue = max(peak_queue, metrics.queue_length)
                    peak_throughput = max(peak_throughput, metrics.outflow)

        dominant_exit = None
        if exit_usage:
            dominant_exit = max(exit_usage.items(), key=lambda item: item[1])[0]

        return {
            "total_time_s": simulation.time_s,
            "agents_total": len(simulation.agents),
            "agents_evacuated": len(simulation.completed_agents),
            "agents_remaining": len([a for a in simulation.agents if not a.has_evacuated]),
            "mean_travel_time_s": float(np.mean(travel_times)) if travel_times else 0.0,
            "p95_travel_time_s": float(np.percentile(travel_times, 95)) if travel_times else 0.0,
            "peak_mean_density": float(max(simulation.density_history) if simulation.density_history else 0.0),
            "peak_cell_occupancy": peak_cell_occupancy,
            "peak_bottleneck_queue": peak_queue,
            "peak_bottleneck_throughput": peak_throughput,
            "dominant_exit": dominant_exit or "n/a",
        }
