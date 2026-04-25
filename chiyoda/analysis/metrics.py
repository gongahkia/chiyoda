"""
ITED simulation analytics with information-theoretic metrics,
fundamental diagram extraction, and CBRN-specific measures.
"""
from __future__ import annotations
from typing import Any, Dict, List, Tuple
import numpy as np


class SimulationAnalytics:
    def calculate_performance_metrics(self, simulation) -> Dict[str, Any]:
        history = simulation.step_history
        travel_times = simulation.travel_times_s
        exit_usage = history[-1].exit_flow_cumulative if history else {}
        exposures = [float(a.hazard_exposure) for a in simulation.agents]
        risk_scores = [float(a.hazard_risk) for a in simulation.agents]
        pending = len([a for a in simulation.agents if not a.has_evacuated and a.release_step > simulation.current_step])

        peak_queue = peak_throughput = peak_cell_occupancy = 0
        if history:
            peak_cell_occupancy = int(max(np.max(s.occupancy_grid) for s in history))
            for step in history:
                for m in step.bottlenecks.values():
                    peak_queue = max(peak_queue, m.queue_length)
                    peak_throughput = max(peak_throughput, m.outflow)

        dominant_exit = None
        exit_imbalance = 0.0
        if exit_usage:
            dominant_exit = max(exit_usage.items(), key=lambda x: x[1])[0]
            total_exit_flow = float(sum(exit_usage.values()))
            if total_exit_flow > 0:
                exit_imbalance = float(max(exit_usage.values()) / total_exit_flow)

        # ITED: information metrics
        entropy_series = getattr(simulation, 'entropy_history', [])
        initial_entropy = entropy_series[0] if entropy_series else 0.0
        final_entropy = entropy_series[-1] if entropy_series else 0.0
        peak_entropy = max(entropy_series) if entropy_series else 0.0
        mean_entropy = float(np.mean(entropy_series)) if entropy_series else 0.0

        # ITED: incapacitation count
        incapacitated = sum(
            1 for a in simulation.agents
            if hasattr(a, 'physiology') and a.physiology.incapacitated
        )

        # ITED: decision quality — fraction using objectively correct route
        correct_route = 0
        total_evacuated = len(simulation.completed_agents)
        if total_evacuated > 0 and dominant_exit:
            correct_route = sum(
                1 for a in simulation.completed_agents
                if getattr(a, 'evacuated_via', None) == dominant_exit
            ) / total_evacuated

        # fundamental diagram data: extract speed-density pairs
        fd_speeds = []
        fd_densities = []
        for step in history:
            for a in step.agents:
                if a.speed > 0.01:
                    fd_speeds.append(a.speed)
                    fd_densities.append(a.local_density)

        mean_fd_speed = float(np.mean(fd_speeds)) if fd_speeds else 0.0
        mean_fd_density = float(np.mean(fd_densities)) if fd_densities else 0.0

        interventions = list(getattr(simulation, "intervention_events", []))
        intervention_count = len(interventions)
        intervention_recipients = int(sum(event.recipients for event in interventions))
        intervention_entropy_reduction = float(
            sum(max(0.0, event.entropy_before - event.entropy_after) for event in interventions)
        )
        intervention_accuracy_gain = float(
            sum(max(0.0, event.accuracy_after - event.accuracy_before) for event in interventions)
        )
        intervention_exposure_pressure = float(
            sum(event.mean_hazard_load * max(1, event.recipients) for event in interventions)
        )
        intervention_queue_pressure = float(
            sum(event.peak_queue_length for event in interventions)
        )
        information_safety_efficiency = (
            (intervention_entropy_reduction + intervention_accuracy_gain)
            / (1.0 + intervention_exposure_pressure + intervention_queue_pressure)
            if intervention_count > 0
            else 0.0
        )
        harmful_convergence_index = 0.0
        if intervention_entropy_reduction > 0:
            exposure_factor = 1.0 + (float(np.mean(exposures)) if exposures else 0.0)
            harmful_convergence_index = float(
                exit_imbalance
                * (1.0 + peak_queue)
                * exposure_factor
                / (1.0 + intervention_entropy_reduction)
            )

        return {
            "total_time_s": simulation.time_s,
            "agents_total": len(simulation.agents),
            "agents_evacuated": len(simulation.completed_agents),
            "agents_remaining": len([a for a in simulation.agents if not a.has_evacuated]),
            "agents_pending_release": pending,
            "agents_incapacitated": incapacitated,
            "mean_travel_time_s": float(np.mean(travel_times)) if travel_times else 0.0,
            "p95_travel_time_s": float(np.percentile(travel_times, 95)) if travel_times else 0.0,
            "peak_mean_density": float(max(simulation.density_history) if simulation.density_history else 0.0),
            "peak_cell_occupancy": peak_cell_occupancy,
            "peak_bottleneck_queue": peak_queue,
            "peak_bottleneck_throughput": peak_throughput,
            "dominant_exit": dominant_exit or "n/a",
            "exit_imbalance": exit_imbalance,
            "mean_hazard_exposure": float(np.mean(exposures)) if exposures else 0.0,
            "p95_hazard_exposure": float(np.percentile(exposures, 95)) if exposures else 0.0,
            "peak_hazard_risk": float(max(risk_scores)) if risk_scores else 0.0,
            "bottleneck_zone_count": len(simulation.bottleneck_zones),
            # ITED metrics
            "initial_entropy": initial_entropy,
            "final_entropy": final_entropy,
            "peak_entropy": peak_entropy,
            "mean_entropy": mean_entropy,
            "entropy_reduction": initial_entropy - final_entropy,
            "correct_route_fraction": correct_route,
            "mean_fd_speed": mean_fd_speed,
            "mean_fd_density": mean_fd_density,
            "intervention_count": intervention_count,
            "intervention_recipients": intervention_recipients,
            "intervention_entropy_reduction": intervention_entropy_reduction,
            "intervention_accuracy_gain": intervention_accuracy_gain,
            "intervention_exposure_pressure": intervention_exposure_pressure,
            "intervention_queue_pressure": intervention_queue_pressure,
            "information_safety_efficiency": information_safety_efficiency,
            "harmful_convergence_index": harmful_convergence_index,
        }
