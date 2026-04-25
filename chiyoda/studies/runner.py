from __future__ import annotations

from copy import deepcopy
from datetime import datetime, timezone
from itertools import product
from pathlib import Path
from typing import Any, Dict, Iterable, List, Sequence

import numpy as np
import pandas as pd
import yaml

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.models import ComparisonResult, StudyBundle
from chiyoda.studies.schema import InterventionConfig, StudyConfig, StudyVariant, SweepParameter


def load_study_config(path: str | Path) -> StudyConfig:
    study_path = Path(path).resolve()
    with study_path.open("r") as handle:
        payload = yaml.safe_load(handle)
    raw = payload.get("study", payload)
    config = StudyConfig.model_validate(raw)
    scenario_path = Path(config.scenario_file)
    if not scenario_path.is_absolute():
        scenario_path = study_path.parent / scenario_path
    return config.model_copy(update={"scenario_file": str(scenario_path.resolve())})


def run_study(study: str | Path | StudyConfig) -> StudyBundle:
    config = _coerce_study_input(study)
    manager = ScenarioManager()
    variants = _materialize_variants(config)

    summary_frames: List[pd.DataFrame] = []
    steps_frames: List[pd.DataFrame] = []
    cells_frames: List[pd.DataFrame] = []
    agent_steps_frames: List[pd.DataFrame] = []
    agents_frames: List[pd.DataFrame] = []
    bottlenecks_frames: List[pd.DataFrame] = []
    dwell_frames: List[pd.DataFrame] = []
    exits_frames: List[pd.DataFrame] = []
    hazards_frames: List[pd.DataFrame] = []

    runs_manifest: List[Dict[str, Any]] = []
    first_layout_text = None
    first_bottlenecks: List[Dict[str, Any]] = []
    first_exit_labels: Dict[str, str] = {}
    scenario_name = None
    analytics = SimulationAnalytics()

    run_index = 0
    for variant in variants:
        seeds = _resolve_seeds(config, variant)
        for seed in seeds:
            prepared = _prepare_scenario(manager, config.scenario_file, variant, seed)
            simulation = manager.build_simulation(prepared)
            simulation.run()

            scenario_name = prepared.get("name", Path(config.scenario_file).stem)
            if first_layout_text is None:
                first_layout_text = manager.serialize_layout(simulation.layout)
                first_bottlenecks = [
                    {
                        "zone_id": zone.zone_id,
                        "cells": [list(cell) for cell in zone.cells],
                        "orientation": zone.orientation,
                        "centroid": list(zone.centroid),
                    }
                    for zone in simulation.bottleneck_zones
                ]
                first_exit_labels = {
                    f"{cell[0]},{cell[1]}": label
                    for cell, label in simulation.exit_labels.items()
                }

            run_id = f"{variant.name}__seed_{seed}__run_{run_index + 1}"
            run_index += 1
            tables = _collect_run_tables(
                simulation=simulation,
                analytics=analytics,
                study_name=config.name,
                scenario_name=scenario_name,
                variant_name=variant.name,
                seed=seed,
                run_id=run_id,
            )
            summary_frames.append(tables["summary"])
            steps_frames.append(tables["steps"])
            cells_frames.append(tables["cells"])
            agent_steps_frames.append(tables["agent_steps"])
            agents_frames.append(tables["agents"])
            bottlenecks_frames.append(tables["bottlenecks"])
            dwell_frames.append(tables["dwell_samples"])
            exits_frames.append(tables["exits"])
            hazards_frames.append(tables["hazards"])
            runs_manifest.append(
                {
                    "run_id": run_id,
                    "variant_name": variant.name,
                    "seed": seed,
                    "acceleration_backend": simulation.acceleration.name,
                    "requested_acceleration_backend": simulation.acceleration.requested_backend,
                    "agents_total": len(simulation.agents),
                    "agents_evacuated": len(simulation.completed_agents),
                }
            )

    summary = _concat(summary_frames)
    summary = pd.concat([summary, _aggregate_summary(summary)], ignore_index=True)

    metadata = {
        "study_name": config.name,
        "description": config.description,
        "scenario_file": config.scenario_file,
        "scenario_name": scenario_name or Path(config.scenario_file).stem,
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "export_config": config.export.model_dump(),
        "acceleration_backend": runs_manifest[0]["acceleration_backend"] if runs_manifest else "python",
        "requested_acceleration_backend": (
            runs_manifest[0]["requested_acceleration_backend"] if runs_manifest else "auto"
        ),
        "layout_text": first_layout_text or "",
        "layout_width": summary["layout_width"].dropna().iloc[0] if not summary.empty else 0,
        "layout_height": summary["layout_height"].dropna().iloc[0] if not summary.empty else 0,
        "layout_origin_x": summary["layout_origin_x"].dropna().iloc[0] if not summary.empty else 0.0,
        "layout_origin_y": summary["layout_origin_y"].dropna().iloc[0] if not summary.empty else 0.0,
        "layout_cell_size": summary["layout_cell_size"].dropna().iloc[0] if not summary.empty else 1.0,
        "bottleneck_zones": first_bottlenecks,
        "exit_labels": first_exit_labels,
        "variants": [variant.model_dump() for variant in variants],
        "runs": runs_manifest,
        "representative_run_id": runs_manifest[0]["run_id"] if runs_manifest else None,
    }

    return StudyBundle(
        metadata=metadata,
        summary=summary,
        steps=_concat(steps_frames),
        cells=_concat(cells_frames),
        agent_steps=_concat(agent_steps_frames),
        agents=_concat(agents_frames),
        bottlenecks=_concat(bottlenecks_frames),
        dwell_samples=_concat(dwell_frames),
        exits=_concat(exits_frames),
        hazards=_concat(hazards_frames),
    )


def compare_studies(
    baseline: str | Path | StudyBundle,
    variant: str | Path | StudyBundle,
) -> ComparisonResult:
    baseline_bundle = StudyBundle.load(baseline) if not isinstance(baseline, StudyBundle) else baseline
    variant_bundle = StudyBundle.load(variant) if not isinstance(variant, StudyBundle) else variant

    baseline_summary = _study_aggregate_row(baseline_bundle.summary)
    variant_summary = _study_aggregate_row(variant_bundle.summary)
    numeric_cols = [
        column
        for column in baseline_summary.index
        if isinstance(baseline_summary[column], (int, float, np.number))
        and column not in {"run_count", "seed"}
    ]

    metrics_rows: List[Dict[str, Any]] = []
    for metric in numeric_cols:
        baseline_value = float(baseline_summary[metric])
        variant_value = float(variant_summary[metric])
        delta = variant_value - baseline_value
        pct_change = 0.0 if abs(baseline_value) < 1e-9 else (delta / baseline_value) * 100.0
        metrics_rows.append(
            {
                "metric": metric,
                "baseline_value": baseline_value,
                "variant_value": variant_value,
                "delta": delta,
                "pct_change": pct_change,
            }
        )

    timeseries = pd.concat(
        [
            _aggregate_timeseries(baseline_bundle.steps).assign(series="baseline"),
            _aggregate_timeseries(variant_bundle.steps).assign(series="variant"),
        ],
        ignore_index=True,
    )

    summary = pd.DataFrame(
        [
            baseline_summary.to_dict() | {"series": "baseline"},
            variant_summary.to_dict() | {"series": "variant"},
        ]
    )
    metadata = {
        "baseline_study_name": baseline_bundle.metadata.get("study_name"),
        "variant_study_name": variant_bundle.metadata.get("study_name"),
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
    }

    return ComparisonResult(
        metadata=metadata,
        summary=summary,
        timeseries=timeseries,
        metrics=pd.DataFrame(metrics_rows),
    )


def _coerce_study_input(study: str | Path | StudyConfig) -> StudyConfig:
    if isinstance(study, StudyConfig):
        return study

    path = Path(study).resolve()
    with path.open("r") as handle:
        payload = yaml.safe_load(handle)

    if isinstance(payload, dict) and ("study" in payload or "scenario_file" in payload):
        return load_study_config(path)

    return StudyConfig(
        name=path.stem,
        scenario_file=str(path),
        seeds=[],
        repetitions=1,
    )


def _materialize_variants(config: StudyConfig) -> List[StudyVariant]:
    variants = list(config.variants)

    if config.sweep:
        for combo in product(*[parameter.values for parameter in config.sweep]):
            scenario_overrides: Dict[str, Any] = {}
            labels: List[str] = []
            for parameter, value in zip(config.sweep, combo):
                _set_nested_value(scenario_overrides, parameter.path, value)
                token = parameter.label or parameter.path.split(".")[-1]
                labels.append(f"{token}_{value}")
            variants.append(
                StudyVariant(
                    name="__".join(labels),
                    description="Auto-generated sweep variant",
                    scenario_overrides=scenario_overrides,
                )
            )

    if not variants:
        variants = [StudyVariant(name="baseline")]
    return variants


def _resolve_seeds(config: StudyConfig, variant: StudyVariant) -> List[int]:
    if variant.seeds:
        return list(variant.seeds)
    if config.seeds:
        return list(config.seeds)
    return [42 + index for index in range(config.repetitions)]


def _prepare_scenario(
    manager: ScenarioManager,
    scenario_file: str,
    variant: StudyVariant,
    seed: int,
) -> Dict[str, Any]:
    scenario = deepcopy(manager.load_config(scenario_file))
    if variant.scenario_overrides:
        scenario = manager._deep_merge(scenario, deepcopy(variant.scenario_overrides))
    for intervention in variant.interventions:
        scenario = _apply_intervention(manager, scenario, intervention)
    scenario.setdefault("simulation", {})
    scenario["simulation"]["random_seed"] = seed
    return scenario


def _apply_intervention(
    manager: ScenarioManager,
    scenario: Dict[str, Any],
    intervention: InterventionConfig,
) -> Dict[str, Any]:
    updated = deepcopy(scenario)

    if intervention.type in {"corridor_narrowing", "block_cells"}:
        return manager.apply_layout_cells(updated, cells=list(intervention.cells), fill=manager.wall_token())
    if intervention.type in {"corridor_widening", "clear_cells"}:
        return manager.apply_layout_cells(updated, cells=list(intervention.cells), fill=manager.empty_token())
    if intervention.type == "exit_closure":
        return manager.apply_layout_cells(updated, cells=list(intervention.exits), fill=manager.wall_token())

    if intervention.type == "staggered_release":
        updated = _ensure_population_cohorts(manager, updated)
        for cohort in updated["population"]["cohorts"]:
            if intervention.cohort is None or cohort["name"] == intervention.cohort:
                cohort["release_step"] = int(intervention.release_step or 0)
        return updated

    if intervention.type == "demand_surge":
        population = updated.setdefault("population", {})
        cohorts = list(population.get("cohorts", []) or [])
        cohorts.append(
            {
                "name": intervention.name or f"demand_surge_{len(cohorts) + 1}",
                "count": int(intervention.count or 0),
                "personality": intervention.personality,
                "calmness": intervention.calmness,
                "base_speed_multiplier": intervention.base_speed_multiplier,
                "release_step": int(intervention.release_step or 0),
                "group_size": intervention.group_size,
                "spawn_cells": [list(cell) for cell in intervention.spawn_cells],
            }
        )
        population["cohorts"] = cohorts
        population["total"] = int(population.get("total", 0)) + int(intervention.count or 0)
        return updated

    raise ValueError(f"Unsupported intervention type: {intervention.type}")


def _ensure_population_cohorts(manager: ScenarioManager, scenario: Dict[str, Any]) -> Dict[str, Any]:
    updated = deepcopy(scenario)
    population = updated.setdefault("population", {})
    if population.get("cohorts"):
        return updated

    layout = manager._build_layout(updated)
    baseline_total = int(population.get("total", len(layout.people_positions()) or 100))
    population["cohorts"] = [
        {
            "name": "baseline",
            "count": baseline_total,
            "personality": "NORMAL",
            "calmness": 0.8,
            "base_speed_multiplier": 1.0,
            "release_step": 0,
            "group_size": 1,
        }
    ]
    population["total"] = baseline_total
    return updated


def _set_nested_value(target: Dict[str, Any], path: str, value: Any) -> None:
    parts = path.split(".")
    cursor: Any = target
    for index, part in enumerate(parts):
        is_last = index == len(parts) - 1
        if part.isdigit():
            numeric_index = int(part)
            if not isinstance(cursor, list):
                raise ValueError(f"Path component {part} expects a list in {path}")
            while len(cursor) <= numeric_index:
                cursor.append({})
            if is_last:
                cursor[numeric_index] = value
            else:
                if not isinstance(cursor[numeric_index], (dict, list)):
                    cursor[numeric_index] = {}
                cursor = cursor[numeric_index]
            continue

        if is_last:
            cursor[part] = value
            return
        next_part = parts[index + 1]
        if part not in cursor:
            cursor[part] = [] if next_part.isdigit() else {}
        cursor = cursor[part]


def _collect_run_tables(
    *,
    simulation,
    analytics: SimulationAnalytics,
    study_name: str,
    scenario_name: str,
    variant_name: str,
    seed: int,
    run_id: str,
) -> Dict[str, pd.DataFrame]:
    steps_rows: List[Dict[str, Any]] = []
    cells_rows: List[Dict[str, Any]] = []
    agent_step_rows: List[Dict[str, Any]] = []
    exit_rows: List[Dict[str, Any]] = []
    bottleneck_rows: List[Dict[str, Any]] = []
    hazard_rows: List[Dict[str, Any]] = []
    agent_rows: List[Dict[str, Any]] = []
    dwell_rows: List[Dict[str, Any]] = []

    for step in simulation.step_history:
        steps_rows.append(
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
                "step": step.step,
                "time_s": step.time_s,
                "evacuated_total": step.evacuated_total,
                "remaining": step.remaining,
                "pending_release": step.pending_release,
                "mean_speed": step.mean_speed,
                "mean_density": step.mean_density,
                "peak_cell_occupancy": int(step.occupancy_grid.max()) if step.occupancy_grid.size else 0,
                "global_entropy": float(getattr(step, 'global_entropy', 0.0)),
            }
        )

        active_cells = np.argwhere(
            (step.occupancy_grid > 0)
            | (step.path_usage_grid > 0)
            | (step.speed_grid > 0)
            | (step.density_grid > 0)
        )
        for y, x in active_cells:
            cells_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "step": step.step,
                    "time_s": step.time_s,
                    "x": int(x),
                    "y": int(y),
                    "occupancy": int(step.occupancy_grid[y, x]),
                    "density": float(step.density_grid[y, x]),
                    "speed": float(step.speed_grid[y, x]),
                    "path_usage": int(step.path_usage_grid[y, x]),
                }
            )

        for agent in step.agents:
            target_exit_x = None if agent.target_exit is None else agent.target_exit[0]
            target_exit_y = None if agent.target_exit is None else agent.target_exit[1]
            agent_step_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "step": step.step,
                    "time_s": step.time_s,
                    "agent_id": agent.agent_id,
                    "x": float(agent.position[0]),
                    "y": float(agent.position[1]),
                    "cell_x": int(agent.cell[0]),
                    "cell_y": int(agent.cell[1]),
                    "state": agent.state,
                    "speed": float(agent.speed),
                    "local_density": float(agent.local_density),
                    "target_exit_x": target_exit_x,
                    "target_exit_y": target_exit_y,
                    "cohort_name": agent.cohort_name,
                    "group_id": agent.group_id,
                    "leader_id": agent.leader_id,
                    "hazard_exposure": float(agent.hazard_exposure),
                    "hazard_load": float(agent.hazard_load),
                    "entropy": float(getattr(agent, 'entropy', 0.0)),
                    "belief_accuracy": float(getattr(agent, 'belief_accuracy', 1.0)),
                    "impairment": float(getattr(agent, 'impairment', 0.0)),
                    "decision_mode": str(getattr(agent, 'decision_mode', 'EVACUATE')),
                }
            )

        for exit_label in simulation.exit_labels.values():
            exit_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "step": step.step,
                    "time_s": step.time_s,
                    "exit_label": exit_label,
                    "flow_step": int(step.exit_flow_step.get(exit_label, 0)),
                    "flow_cumulative": int(step.exit_flow_cumulative.get(exit_label, 0)),
                }
            )

        for zone_id, metrics in step.bottlenecks.items():
            bottleneck_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "step": step.step,
                    "time_s": step.time_s,
                    "zone_id": zone_id,
                    "occupancy": int(metrics.occupancy),
                    "inflow": int(metrics.inflow),
                    "outflow": int(metrics.outflow),
                    "queue_length": int(metrics.queue_length),
                    "mean_dwell_s": float(metrics.mean_dwell_s),
                    "mean_speed": float(metrics.mean_speed),
                    "mean_density": float(metrics.mean_density),
                }
            )

        for hazard_index, hazard in enumerate(step.hazards, start=1):
            hazard_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "step": step.step,
                    "time_s": step.time_s,
                    "hazard_id": f"haz_{hazard_index}",
                    "kind": hazard.get("kind", "GAS"),
                    "x": float(hazard["pos"][0]),
                    "y": float(hazard["pos"][1]),
                    "radius": float(hazard.get("radius", 0.0)),
                    "severity": float(hazard.get("severity", 0.0)),
                }
            )

    for agent in simulation.agents:
        agent_rows.append(
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
                "agent_id": agent.id,
                "cohort_name": getattr(agent, "cohort_name", "baseline"),
                "personality": getattr(agent, "personality", "NORMAL"),
                "calmness": float(getattr(agent, "calmness", 0.8)),
                "release_step": int(getattr(agent, "release_step", 0)),
                "group_id": getattr(agent, "group_id", None),
                "leader_id": getattr(agent, "leader_id", None),
                "assisted_agent_id": getattr(agent, "assisted_agent_id", None),
                "evacuated": bool(agent.has_evacuated),
                "travel_time_s": float(agent.travel_time_s),
                "hazard_exposure": float(agent.hazard_exposure),
                "hazard_risk": float(agent.hazard_risk),
                "evacuated_via": getattr(agent, "evacuated_via", None),
                "base_speed": float(agent.base_speed),
            }
        )

    for zone_id, samples in simulation.bottleneck_dwell_samples.items():
        for sample in samples:
            dwell_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "zone_id": zone_id,
                    "dwell_s": float(sample),
                }
            )

    summary_metrics = analytics.calculate_performance_metrics(simulation)
    summary_row = pd.DataFrame(
        [
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
                "record_type": "run",
                "layout_width": simulation.layout.width,
                "layout_height": simulation.layout.height,
                "layout_origin_x": float(simulation.layout.origin[0]),
                "layout_origin_y": float(simulation.layout.origin[1]),
                "layout_cell_size": float(simulation.layout.cell_size),
                "acceleration_backend": simulation.acceleration.name,
                "requested_acceleration_backend": simulation.acceleration.requested_backend,
                **summary_metrics,
            }
        ]
    )

    return {
        "summary": summary_row,
        "steps": pd.DataFrame(steps_rows),
        "cells": pd.DataFrame(cells_rows),
        "agent_steps": pd.DataFrame(agent_step_rows),
        "agents": pd.DataFrame(agent_rows),
        "bottlenecks": pd.DataFrame(bottleneck_rows),
        "dwell_samples": pd.DataFrame(dwell_rows),
        "exits": pd.DataFrame(exit_rows),
        "hazards": pd.DataFrame(hazard_rows),
    }


def _aggregate_summary(summary: pd.DataFrame) -> pd.DataFrame:
    if summary.empty:
        return pd.DataFrame()

    run_rows = summary[summary["record_type"] == "run"].copy()
    numeric_cols = [
        column
        for column in run_rows.columns
        if pd.api.types.is_numeric_dtype(run_rows[column]) and column not in {"seed"}
    ]

    aggregate_rows: List[Dict[str, Any]] = []
    for variant_name, group in run_rows.groupby("variant_name", sort=False):
        row = {
            "study_name": group["study_name"].iloc[0],
            "scenario_name": group["scenario_name"].iloc[0],
            "variant_name": variant_name,
            "seed": np.nan,
            "run_id": None,
            "record_type": "variant_aggregate",
            "run_count": len(group),
        }
        for column in numeric_cols:
            row[column] = float(group[column].mean())
            row[f"{column}_std"] = float(group[column].std(ddof=0)) if len(group) > 1 else 0.0
        aggregate_rows.append(row)

    whole = {
        "study_name": run_rows["study_name"].iloc[0],
        "scenario_name": run_rows["scenario_name"].iloc[0],
        "variant_name": "study_total",
        "seed": np.nan,
        "run_id": None,
        "record_type": "study_aggregate",
        "run_count": len(run_rows),
    }
    for column in numeric_cols:
        whole[column] = float(run_rows[column].mean())
        whole[f"{column}_std"] = float(run_rows[column].std(ddof=0)) if len(run_rows) > 1 else 0.0
    aggregate_rows.append(whole)

    return pd.DataFrame(aggregate_rows)


def _study_aggregate_row(summary: pd.DataFrame) -> pd.Series:
    study_rows = summary[summary["record_type"] == "study_aggregate"]
    if not study_rows.empty:
        return study_rows.iloc[0]
    variant_rows = summary[summary["record_type"] == "variant_aggregate"]
    if not variant_rows.empty:
        return variant_rows.iloc[0]
    return summary.iloc[0]


def _aggregate_timeseries(steps: pd.DataFrame) -> pd.DataFrame:
    if steps.empty:
        return pd.DataFrame(columns=["time_s", "evacuated_total", "mean_speed", "mean_density"])
    return (
        steps.groupby("time_s", as_index=False)[["evacuated_total", "mean_speed", "mean_density"]]
        .mean()
        .sort_values("time_s")
    )


def _concat(frames: Sequence[pd.DataFrame]) -> pd.DataFrame:
    frames = [frame for frame in frames if frame is not None]
    if not frames:
        return pd.DataFrame()
    return pd.concat(frames, ignore_index=True)
