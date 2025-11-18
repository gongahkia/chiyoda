from __future__ import annotations

from typing import Dict, Any
import numpy as np


class SimulationAnalytics:
    def calculate_performance_metrics(self, simulation) -> Dict[str, Any]:
        evac_times = simulation.evacuated_at_step
        dt = simulation.config.dt
        times_s = [t * dt for t in evac_times]
        density = simulation.density_history

        return {
            "total_time_s": simulation.time_s,
            "agents_total": len(simulation.agents),
            "agents_evacuated": len(simulation.completed_agents),
            "agents_remaining": len([a for a in simulation.agents if not a.has_evacuated]),
            "mean_evac_time_s": float(np.mean(times_s)) if times_s else 0.0,
            "p95_evac_time_s": float(np.percentile(times_s, 95)) if times_s else 0.0,
            "max_density": float(np.max(density)) if density else 0.0,
            "avg_density": float(np.mean(density)) if density else 0.0,
        }
