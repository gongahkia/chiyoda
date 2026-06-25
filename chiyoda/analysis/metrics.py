"""
ITED simulation analytics with information-theoretic metrics,
fundamental diagram extraction, and CBRN-specific measures.
"""

from __future__ import annotations

import json
from typing import Any

import numpy as np
import pandas as pd

CAUSAL_DELTA_METRICS = (
    "agents_evacuated",
    "mean_travel_time_s",
    "mean_hazard_exposure",
    "harmful_convergence_index",
    "information_safety_efficiency",
)
ELDERLY_AGE_BANDS = {
    "senior",
    "elderly",
    "older_adult",
    "older-adult",
    "older adult",
    "65+",
}
EQUITY_IMPAIRMENT_THRESHOLD = 0.1
FAMILIARITY_LOW_MAX = 0.33
FAMILIARITY_HIGH_MIN = 0.67


def equity_subgroup_metrics(agents: pd.DataFrame) -> pd.DataFrame:
    if agents.empty:
        return pd.DataFrame()

    frame = agents.copy()
    if "is_responder" in frame.columns:
        frame = frame[~_bool_series(frame["is_responder"])]
    if "is_hostile" in frame.columns:
        frame = frame[~_bool_series(frame["is_hostile"])]
    if frame.empty:
        return pd.DataFrame()

    identity_defaults = {
        "study_name": "unknown",
        "scenario_name": "unknown",
        "variant_name": "baseline",
        "seed": np.nan,
        "run_id": "run_1",
    }
    for column, default in identity_defaults.items():
        if column not in frame.columns:
            frame[column] = default

    for column, default in {
        "impairment": 0.0,
        "familiarity": np.nan,
        "age_band": "",
        "evacuated": False,
        "travel_time_s": 0.0,
        "hazard_exposure": 0.0,
    }.items():
        if column not in frame.columns:
            frame[column] = default

    frame["impairment"] = pd.to_numeric(frame["impairment"], errors="coerce").fillna(
        0.0
    )
    frame["familiarity"] = pd.to_numeric(frame["familiarity"], errors="coerce")
    frame["travel_time_s"] = pd.to_numeric(
        frame["travel_time_s"], errors="coerce"
    ).fillna(0.0)
    frame["hazard_exposure"] = pd.to_numeric(
        frame["hazard_exposure"], errors="coerce"
    ).fillna(0.0)
    frame["evacuated"] = _bool_series(frame["evacuated"])
    frame["age_band"] = frame["age_band"].fillna("").astype(str).str.lower()

    rows: list[dict[str, Any]] = []
    group_cols = ["study_name", "scenario_name", "variant_name", "seed", "run_id"]
    for run_key, run_frame in frame.groupby(group_cols, dropna=False, sort=False):
        run_meta = dict(zip(group_cols, run_key, strict=False))
        run_evacuated = run_frame["evacuated"]
        run_rate = float(run_evacuated.mean()) if len(run_frame) else 0.0
        run_travel = run_frame.loc[run_evacuated, "travel_time_s"]
        run_mean_travel = float(run_travel.mean()) if not run_travel.empty else 0.0
        for subgroup_type, subgroup_tag, subgroup_label, mask in _equity_masks(
            run_frame
        ):
            subgroup = run_frame[mask]
            if subgroup.empty:
                continue
            rows.append(
                _equity_row(
                    run_meta,
                    subgroup_type=subgroup_type,
                    subgroup_tag=subgroup_tag,
                    subgroup_label=subgroup_label,
                    subgroup=subgroup,
                    run_evacuation_rate=run_rate,
                    run_mean_travel_s=run_mean_travel,
                )
            )
    return pd.DataFrame(rows)


def _equity_masks(frame: pd.DataFrame):
    impaired = frame["impairment"] >= EQUITY_IMPAIRMENT_THRESHOLD
    age_known = frame["age_band"].str.len() > 0
    elderly = frame["age_band"].isin(ELDERLY_AGE_BANDS)
    familiarity = frame["familiarity"]
    return [
        ("impairment", "impaired", "Final impairment >= 0.1", impaired),
        ("impairment", "not_impaired", "Final impairment < 0.1", ~impaired),
        ("age", "elderly", "age_band in elderly labels", elderly),
        ("age", "non_elderly", "Known non-elderly age_band", age_known & ~elderly),
        ("age", "unknown_age", "No age_band exported", ~age_known),
        (
            "familiarity_prior",
            "low_familiarity",
            "familiarity < 0.33",
            familiarity < FAMILIARITY_LOW_MAX,
        ),
        (
            "familiarity_prior",
            "medium_familiarity",
            "0.33 <= familiarity < 0.67",
            (familiarity >= FAMILIARITY_LOW_MAX) & (familiarity < FAMILIARITY_HIGH_MIN),
        ),
        (
            "familiarity_prior",
            "high_familiarity",
            "familiarity >= 0.67",
            familiarity >= FAMILIARITY_HIGH_MIN,
        ),
        (
            "familiarity_prior",
            "unknown_familiarity",
            "No familiarity prior exported",
            familiarity.isna(),
        ),
    ]


def _equity_row(
    run_meta: dict[str, Any],
    *,
    subgroup_type: str,
    subgroup_tag: str,
    subgroup_label: str,
    subgroup: pd.DataFrame,
    run_evacuation_rate: float,
    run_mean_travel_s: float,
) -> dict[str, Any]:
    evacuated = subgroup["evacuated"]
    travel = subgroup.loc[evacuated, "travel_time_s"]
    exposure = subgroup["hazard_exposure"]
    mean_travel = float(travel.mean()) if not travel.empty else 0.0
    return {
        **run_meta,
        "subgroup_type": subgroup_type,
        "subgroup_tag": subgroup_tag,
        "subgroup_label": subgroup_label,
        "agent_count": int(len(subgroup)),
        "evacuated_count": int(evacuated.sum()),
        "remaining_count": int((~evacuated).sum()),
        "evacuation_rate": float(evacuated.mean()) if len(subgroup) else 0.0,
        "run_evacuation_rate": run_evacuation_rate,
        "evacuation_rate_gap_vs_run": (
            float(evacuated.mean()) - run_evacuation_rate if len(subgroup) else 0.0
        ),
        "mean_travel_time_s": mean_travel,
        "p95_travel_time_s": (
            float(np.percentile(travel, 95)) if not travel.empty else 0.0
        ),
        "run_mean_travel_time_s": run_mean_travel_s,
        "travel_time_gap_vs_run_s": mean_travel - run_mean_travel_s,
        "equity_time_gap_s": abs(mean_travel - run_mean_travel_s),
        "mean_hazard_exposure": float(exposure.mean()) if len(exposure) else 0.0,
        "p95_hazard_exposure": (
            float(np.percentile(exposure, 95)) if len(exposure) else 0.0
        ),
        "mean_impairment": float(subgroup["impairment"].mean()),
        "mean_familiarity": (
            float(subgroup["familiarity"].mean())
            if subgroup["familiarity"].notna().any()
            else 0.0
        ),
    }


def _bool_series(series: pd.Series) -> pd.Series:
    if pd.api.types.is_bool_dtype(series):
        return series.fillna(False)
    return series.fillna(False).astype(str).str.lower().isin({"1", "true", "yes"})


def causal_delta_payload(
    baseline_bundle,
    treated_bundle,
    *,
    interventions: list[dict[str, Any]],
    metrics: tuple[str, ...] = CAUSAL_DELTA_METRICS,
    bootstrap_samples: int = 1000,
    random_seed: int = 42,
) -> dict[str, Any]:
    from chiyoda.studies.causal import compare_bundles

    frame = compare_bundles(
        baseline_bundle,
        treated_bundle,
        metrics=metrics,
        bootstrap_samples=bootstrap_samples,
        random_seed=random_seed,
    )
    metric_rows = frame.to_dict(orient="records")
    return {
        "estimator": "matched_seed_ate",
        "metrics": metric_rows,
        "interventions": [
            {
                "intervention": intervention,
                "metrics": metric_rows,
            }
            for intervention in interventions
        ],
        "assumptions": {
            "document": "docs/causal_layer_assumptions.md",
            "labels_required": True,
        },
    }


class SimulationAnalytics:
    def calculate_performance_metrics(self, simulation) -> dict[str, Any]:
        history = simulation.step_history
        travel_times = simulation.travel_times_s
        exit_usage = history[-1].exit_flow_cumulative if history else {}
        exposures = [float(a.hazard_exposure) for a in simulation.agents]
        risk_scores = [float(a.hazard_risk) for a in simulation.agents]
        pending = len(
            [
                a
                for a in simulation.agents
                if not a.has_evacuated and a.release_step > simulation.current_step
            ]
        )
        evacuees = [
            a
            for a in simulation.agents
            if not getattr(a, "is_responder", False)
            and not getattr(a, "is_hostile", False)
        ]

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
        entropy_series = getattr(simulation, "entropy_history", [])
        initial_entropy = entropy_series[0] if entropy_series else 0.0
        final_entropy = entropy_series[-1] if entropy_series else 0.0
        peak_entropy = max(entropy_series) if entropy_series else 0.0
        mean_entropy = float(np.mean(entropy_series)) if entropy_series else 0.0

        # ITED: incapacitation count
        incapacitated = sum(
            1
            for a in simulation.agents
            if hasattr(a, "physiology") and a.physiology.incapacitated
        )

        # ITED: decision quality — fraction using objectively correct route
        correct_route = 0
        total_evacuated = len(simulation.completed_agents)
        if total_evacuated > 0 and dominant_exit:
            correct_route = (
                sum(
                    1
                    for a in simulation.completed_agents
                    if getattr(a, "evacuated_via", None) == dominant_exit
                )
                / total_evacuated
            )

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
            sum(
                max(0.0, event.entropy_before - event.entropy_after)
                for event in interventions
            )
        )
        intervention_accuracy_gain = float(
            sum(
                max(0.0, event.accuracy_after - event.accuracy_before)
                for event in interventions
            )
        )
        intervention_exposure_pressure = float(
            sum(
                event.mean_hazard_load * max(1, event.recipients)
                for event in interventions
            )
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
        hostile_events = list(getattr(simulation, "hostile_channel_events", []))
        hostile_event_count = len(hostile_events)
        hostile_recipients = int(sum(event.recipients for event in hostile_events))
        hostile_mean_credibility = (
            float(np.mean([event.credibility for event in hostile_events]))
            if hostile_events
            else 0.0
        )
        induced_convergence_pressure = float(
            exit_imbalance * hostile_recipients * hostile_mean_credibility
        )
        harmful_convergence_index_accidental = harmful_convergence_index
        harmful_convergence_index_induced = float(
            harmful_convergence_index + induced_convergence_pressure
        )
        information_safety_efficiency_adversarial = (
            intervention_entropy_reduction + intervention_accuracy_gain
        ) / (
            1.0
            + intervention_exposure_pressure
            + intervention_queue_pressure
            + hostile_recipients
        ) - induced_convergence_pressure
        shooter_events = list(getattr(simulation, "hostile_agent_events", []))
        shooter_event_count = len(shooter_events)
        exposure_to_los = float(
            sum(event.get("accuracy", 0.0) for event in shooter_events)
        )
        decision_modes = [
            str(getattr(agent_step, "decision_mode", ""))
            for step in history
            for agent_step in step.agents
        ]
        mode_total = max(
            1, sum(1 for mode in decision_modes if mode in {"RUN", "HIDE", "FIGHT"})
        )
        run_hide_fight = {
            "run": sum(1 for mode in decision_modes if mode == "RUN") / mode_total,
            "hide": sum(1 for mode in decision_modes if mode == "HIDE") / mode_total,
            "fight": sum(1 for mode in decision_modes if mode == "FIGHT") / mode_total,
        }
        shelter_times = [
            step.time_s
            for step in history
            if any(
                getattr(agent_step, "decision_mode", "") == "HIDE"
                for agent_step in step.agents
            )
        ]
        time_to_shelter_s = float(min(shelter_times)) if shelter_times else 0.0
        left_behind_index = _left_behind_index(evacuees)
        exposure_by_group = _mean_exposure_by(evacuees, "cohort_name")
        exposure_by_mobility_class = _mean_exposure_by(evacuees, "mobility_class")
        percentile_gap_time_to_safety_s = (
            float(np.percentile(travel_times, 95) - np.percentile(travel_times, 50))
            if travel_times
            else 0.0
        )

        return {
            "total_time_s": simulation.time_s,
            "agents_total": len(simulation.agents),
            "agents_evacuated": len(simulation.completed_agents),
            "agents_remaining": len(
                [a for a in simulation.agents if not a.has_evacuated]
            ),
            "agents_pending_release": pending,
            "agents_incapacitated": incapacitated,
            "mean_travel_time_s": float(np.mean(travel_times)) if travel_times else 0.0,
            "p95_travel_time_s": (
                float(np.percentile(travel_times, 95)) if travel_times else 0.0
            ),
            "peak_mean_density": float(
                max(simulation.density_history) if simulation.density_history else 0.0
            ),
            "peak_cell_occupancy": peak_cell_occupancy,
            "peak_bottleneck_queue": peak_queue,
            "peak_bottleneck_throughput": peak_throughput,
            "dominant_exit": dominant_exit or "n/a",
            "exit_imbalance": exit_imbalance,
            "mean_hazard_exposure": float(np.mean(exposures)) if exposures else 0.0,
            "p95_hazard_exposure": (
                float(np.percentile(exposures, 95)) if exposures else 0.0
            ),
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
            "hostile_channel_event_count": hostile_event_count,
            "hostile_channel_recipients": hostile_recipients,
            "hostile_channel_mean_credibility": hostile_mean_credibility,
            "harmful_convergence_index_accidental": harmful_convergence_index_accidental,
            "harmful_convergence_index_induced": harmful_convergence_index_induced,
            "information_safety_efficiency_adversarial": information_safety_efficiency_adversarial,
            "active_shooter_event_count": shooter_event_count,
            "exposure_to_los": exposure_to_los,
            "time_to_shelter_s": time_to_shelter_s,
            "run_fraction": run_hide_fight["run"],
            "hide_fraction": run_hide_fight["hide"],
            "fight_fraction": run_hide_fight["fight"],
            "left_behind_index": left_behind_index,
            "exposure_by_group": json.dumps(exposure_by_group, sort_keys=True),
            "exposure_by_mobility_class": json.dumps(
                exposure_by_mobility_class, sort_keys=True
            ),
            "percentile_gap_time_to_safety_s": percentile_gap_time_to_safety_s,
        }


def _left_behind_index(agents) -> float:
    grouped: dict[str, list] = {}
    for agent in agents:
        grouped.setdefault(str(getattr(agent, "cohort_name", "baseline")), []).append(
            agent
        )
    rates = []
    for members in grouped.values():
        if members:
            rates.append(
                sum(1 for agent in members if not agent.has_evacuated) / len(members)
            )
    return float(max(rates) - min(rates)) if len(rates) >= 2 else 0.0


def _mean_exposure_by(agents, attr: str) -> dict[str, float]:
    grouped: dict[str, list[float]] = {}
    for agent in agents:
        key = str(getattr(agent, attr, "unknown"))
        grouped.setdefault(key, []).append(
            float(getattr(agent, "hazard_exposure", 0.0))
        )
    return {
        key: float(np.mean(values)) if values else 0.0
        for key, values in sorted(grouped.items())
    }
