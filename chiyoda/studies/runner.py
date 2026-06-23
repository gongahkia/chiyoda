from __future__ import annotations

import json
from collections.abc import Sequence
from concurrent.futures import ProcessPoolExecutor
from copy import deepcopy
from datetime import UTC, datetime
from itertools import product
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
import yaml

from chiyoda.analysis.metrics import (
    SimulationAnalytics,
    causal_delta_payload,
    equity_subgroup_metrics,
)
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.models import ComparisonResult, StudyBundle
from chiyoda.studies.schema import (
    InterventionConfig,
    StudyConfig,
    StudyVariant,
)


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


def run_study(
    study: str | Path | StudyConfig, *, per_step_intent: bool = False
) -> StudyBundle:
    from chiyoda._logging import log_event

    config = _coerce_study_input(study)
    variants = _materialize_variants(config)
    log_event(
        None,
        "study.run.start",
        scenario_file=str(getattr(config, "scenario_file", "")),
        variant_count=len(variants),
    )

    summary_frames: list[pd.DataFrame] = []
    steps_frames: list[pd.DataFrame] = []
    cells_frames: list[pd.DataFrame] = []
    intent_path_usage_frames: list[pd.DataFrame] = []
    agent_steps_frames: list[pd.DataFrame] = []
    agents_frames: list[pd.DataFrame] = []
    equity_subgroups_frames: list[pd.DataFrame] = []
    bottlenecks_frames: list[pd.DataFrame] = []
    dwell_frames: list[pd.DataFrame] = []
    exits_frames: list[pd.DataFrame] = []
    hazards_frames: list[pd.DataFrame] = []
    measurements_frames: list[pd.DataFrame] = []
    gossip_frames: list[pd.DataFrame] = []
    intervention_frames: list[pd.DataFrame] = []
    llm_decision_frames: list[pd.DataFrame] = []
    llm_call_frames: list[pd.DataFrame] = []
    runs_manifest: list[dict[str, Any]] = []

    first_layout_text = None
    first_layout_floors: list[dict[str, Any]] = []
    first_layout_connectors: list[dict[str, Any]] = []
    first_bottlenecks: list[dict[str, Any]] = []
    first_exit_labels: dict[str, str] = {}
    first_scenario_metadata: dict[str, Any] = {}
    scenario_name = None

    run_index = 0
    for variant in variants:
        seeds = _resolve_seeds(config, variant)
        log_event(
            None,
            "study.variant.start",
            study_name=config.name,
            variant_name=variant.name,
            seed_count=len(seeds),
        )
        tasks = []
        for seed in seeds:
            run_id = f"{variant.name}__seed_{seed}__run_{run_index + 1}"
            log_event(
                None,
                "study.seed_run.start",
                study_name=config.name,
                variant_name=variant.name,
                seed=int(seed),
                run_id=run_id,
            )
            tasks.append(
                {
                    "scenario_file": config.scenario_file,
                    "variant": variant.model_dump(),
                    "seed": int(seed),
                    "run_id": run_id,
                    "study_name": config.name,
                    "per_step_intent": bool(per_step_intent),
                }
            )
            run_index += 1

        for result in _execute_study_tasks(tasks, jobs=int(config.jobs)):
            tables = result["tables"]
            manifest = result["manifest"]
            scenario_name = result["scenario_name"]
            if first_layout_text is None:
                first_layout_text = result["layout_text"]
                first_layout_floors = result["layout_floors"]
                first_layout_connectors = result["layout_connectors"]
                first_bottlenecks = result["bottleneck_zones"]
                first_exit_labels = result["exit_labels"]
                first_scenario_metadata = result["scenario_metadata"]
            summary_frames.append(tables["summary"])
            steps_frames.append(tables["steps"])
            cells_frames.append(tables["cells"])
            intent_path_usage_frames.append(tables["intent_path_usage"])
            agent_steps_frames.append(tables["agent_steps"])
            agents_frames.append(tables["agents"])
            equity_subgroups_frames.append(tables["equity_subgroups"])
            bottlenecks_frames.append(tables["bottlenecks"])
            dwell_frames.append(tables["dwell_samples"])
            exits_frames.append(tables["exits"])
            hazards_frames.append(tables["hazards"])
            measurements_frames.append(tables["measurements"])
            gossip_frames.append(tables["gossip"])
            intervention_frames.append(tables["interventions"])
            llm_decision_frames.append(tables["llm_decisions"])
            llm_call_frames.append(tables["llm_calls"])
            manifest["treatment_assignment"] = config.treatment_assignments.get(
                int(manifest["seed"]), manifest["variant_name"]
            )
            runs_manifest.append(manifest)
            log_event(
                None,
                "study.seed_run.end",
                study_name=config.name,
                variant_name=manifest["variant_name"],
                seed=int(manifest["seed"]),
                run_id=manifest["run_id"],
                steps=int(result["steps"]),
                agents_evacuated=int(manifest["agents_evacuated"]),
            )
        log_event(
            None,
            "study.variant.end",
            study_name=config.name,
            variant_name=variant.name,
            seed_count=len(seeds),
        )

    summary = _concat(summary_frames)
    summary = pd.concat([summary, _aggregate_summary(summary)], ignore_index=True)

    metadata = {
        "study_name": config.name,
        "description": config.description,
        "scenario_file": config.scenario_file,
        "scenario_name": scenario_name or Path(config.scenario_file).stem,
        "created_at_utc": datetime.now(UTC).isoformat(),
        "export_config": config.export.model_dump(),
        "acceleration_backend": (
            runs_manifest[0]["acceleration_backend"] if runs_manifest else "python"
        ),
        "requested_acceleration_backend": (
            runs_manifest[0]["requested_acceleration_backend"]
            if runs_manifest
            else "auto"
        ),
        "requested_pathfinding_strategy": (
            runs_manifest[0].get("requested_pathfinding_strategy", "auto")
            if runs_manifest
            else "auto"
        ),
        "effective_pathfinding_strategy": (
            runs_manifest[0].get("effective_pathfinding_strategy", "")
            if runs_manifest
            else ""
        ),
        "pathfinding_strategy_counts": (
            runs_manifest[0].get("pathfinding_strategy_counts", {})
            if runs_manifest
            else {}
        ),
        "layout_text": first_layout_text or "",
        "layout_floors": first_layout_floors,
        "layout_connectors": first_layout_connectors,
        "layout_width": (
            summary["layout_width"].dropna().iloc[0] if not summary.empty else 0
        ),
        "layout_height": (
            summary["layout_height"].dropna().iloc[0] if not summary.empty else 0
        ),
        "layout_origin_x": (
            summary["layout_origin_x"].dropna().iloc[0] if not summary.empty else 0.0
        ),
        "layout_origin_y": (
            summary["layout_origin_y"].dropna().iloc[0] if not summary.empty else 0.0
        ),
        "layout_cell_size": (
            summary["layout_cell_size"].dropna().iloc[0] if not summary.empty else 1.0
        ),
        "bottleneck_zones": first_bottlenecks,
        "exit_labels": first_exit_labels,
        "scenario_metadata": first_scenario_metadata,
        "station_provenance": first_scenario_metadata.get("station_provenance"),
        "variants": [variant.model_dump() for variant in variants],
        "treatment_assignments": dict(config.treatment_assignments),
        "runs": runs_manifest,
        "representative_run_id": runs_manifest[0]["run_id"] if runs_manifest else None,
        "per_step_intent": bool(per_step_intent),
        "per_step_intent_budget": "sparse rows only; at most active agents per step before intent/cell grouping",
    }

    log_event(
        None,
        "study.run.complete",
        study_name=config.name,
        run_count=len(runs_manifest),
        variant_count=len(variants),
    )

    return StudyBundle(
        metadata=metadata,
        summary=summary,
        steps=_concat(steps_frames),
        cells=_concat(cells_frames),
        intent_path_usage=_concat(intent_path_usage_frames),
        agent_steps=_concat(agent_steps_frames),
        agents=_concat(agents_frames),
        equity_subgroups=_concat(equity_subgroups_frames),
        bottlenecks=_concat(bottlenecks_frames),
        dwell_samples=_concat(dwell_frames),
        exits=_concat(exits_frames),
        hazards=_concat(hazards_frames),
        measurements=_concat(measurements_frames),
        gossip=_concat(gossip_frames),
        interventions=_concat(intervention_frames),
        llm_decisions=_concat(llm_decision_frames),
        llm_calls=_concat(llm_call_frames),
    )


def run_counterfactual_pair(
    scenario_file: str | Path,
    *,
    seeds: Sequence[int] | None = None,
    repetitions: int = 1,
    jobs: int = 1,
    bootstrap_samples: int = 1000,
    random_seed: int = 42,
    per_step_intent: bool = False,
) -> dict[str, Any]:
    scenario_path = Path(scenario_file).resolve()
    seed_list = list(seeds or [])
    baseline = run_study(
        StudyConfig(
            name=f"{scenario_path.stem}_no_intervention",
            scenario_file=str(scenario_path),
            seeds=seed_list,
            repetitions=repetitions,
            jobs=jobs,
            variants=[
                StudyVariant(
                    name="no_intervention",
                    scenario_overrides={"interventions": {"policy": "none"}},
                )
            ],
        ),
        per_step_intent=per_step_intent,
    )
    treated = run_study(
        StudyConfig(
            name=f"{scenario_path.stem}_treated",
            scenario_file=str(scenario_path),
            seeds=seed_list,
            repetitions=repetitions,
            jobs=jobs,
            variants=[StudyVariant(name="treated")],
        ),
        per_step_intent=per_step_intent,
    )
    scenario = ScenarioManager().load_config(str(scenario_path))
    interventions = _intervention_descriptors(scenario.get("interventions"))
    delta = causal_delta_payload(
        baseline,
        treated,
        interventions=interventions,
        bootstrap_samples=bootstrap_samples,
        random_seed=random_seed,
    )
    delta["metadata"] = {
        "scenario_file": str(scenario_path),
        "baseline_study_name": baseline.metadata.get("study_name"),
        "treated_study_name": treated.metadata.get("study_name"),
        "baseline_variant": "no_intervention",
        "treated_variant": "treated",
        "bootstrap_samples": int(bootstrap_samples),
        "random_seed": int(random_seed),
    }
    return {"baseline": baseline, "treated": treated, "causal_delta": delta}


def compare_studies(
    baseline: str | Path | StudyBundle,
    variant: str | Path | StudyBundle,
) -> ComparisonResult:
    baseline_bundle = (
        StudyBundle.load(baseline)
        if not isinstance(baseline, StudyBundle)
        else baseline
    )
    variant_bundle = (
        StudyBundle.load(variant) if not isinstance(variant, StudyBundle) else variant
    )

    baseline_summary = _study_aggregate_row(baseline_bundle.summary)
    variant_summary = _study_aggregate_row(variant_bundle.summary)
    numeric_cols = [
        column
        for column in baseline_summary.index
        if isinstance(baseline_summary[column], (int, float, np.number))
        and column not in {"run_count", "seed"}
    ]

    metrics_rows: list[dict[str, Any]] = []
    for metric in numeric_cols:
        baseline_value = float(baseline_summary[metric])
        variant_value = float(variant_summary[metric])
        delta = variant_value - baseline_value
        pct_change = (
            0.0 if abs(baseline_value) < 1e-9 else (delta / baseline_value) * 100.0
        )
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
        "created_at_utc": datetime.now(UTC).isoformat(),
    }
    from chiyoda.analysis.reports import llm_cost_report

    metadata["llm_cost_report"] = {
        "baseline": llm_cost_report(baseline_bundle.llm_calls),
        "variant": llm_cost_report(variant_bundle.llm_calls),
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


def _materialize_variants(config: StudyConfig) -> list[StudyVariant]:
    variants = list(config.variants)

    if config.sweep:
        for combo in product(*[parameter.values for parameter in config.sweep]):
            scenario_overrides: dict[str, Any] = {}
            labels: list[str] = []
            for parameter, value in zip(config.sweep, combo, strict=False):
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

    if config.adversarial is not None:
        idx = config.adversarial.hostile_channel_index
        for budget in config.adversarial.attacker_budget:
            for policy in config.adversarial.defender_policy:
                scenario_overrides: dict[str, Any] = {}
                _set_nested_value(
                    scenario_overrides, f"hostile_channels.{idx}.budget", int(budget)
                )
                _set_nested_value(
                    scenario_overrides, "interventions.policy", str(policy)
                )
                variants.append(
                    StudyVariant(
                        name=f"adv_{config.adversarial.pairing}__budget_{budget}__defender_{policy}",
                        description="Auto-generated adversarial attacker/defender pairing",
                        scenario_overrides=scenario_overrides,
                    )
                )

    if not variants:
        variants = [StudyVariant(name="baseline")]
    return variants


def _intervention_descriptors(raw: Any) -> list[dict[str, Any]]:
    if raw is None:
        return [{"policy": "none"}]
    if isinstance(raw, list):
        return [_intervention_descriptor(item) for item in raw]
    return [_intervention_descriptor(raw)]


def _intervention_descriptor(raw: Any) -> dict[str, Any]:
    if not isinstance(raw, dict):
        return {"policy": str(raw)}
    result = {
        "policy": str(raw.get("policy", "unknown")),
    }
    for key in ("message_type", "interval_steps", "target", "budget_per_interval"):
        if key in raw:
            result[key] = raw[key]
    return result


def _resolve_seeds(config: StudyConfig, variant: StudyVariant) -> list[int]:
    if variant.seeds:
        return list(variant.seeds)
    if config.seeds:
        return list(config.seeds)
    return [42 + index for index in range(config.repetitions)]


def _execute_study_tasks(
    tasks: Sequence[dict[str, Any]],
    *,
    jobs: int,
) -> list[dict[str, Any]]:
    if jobs <= 1 or len(tasks) <= 1:
        return [_execute_study_task(task) for task in tasks]
    with ProcessPoolExecutor(max_workers=jobs) as executor:
        return list(executor.map(_execute_study_task, tasks))


def _execute_study_task(task: dict[str, Any]) -> dict[str, Any]:
    manager = ScenarioManager()
    analytics = SimulationAnalytics()
    variant = StudyVariant.model_validate(task["variant"])
    seed = int(task["seed"])
    run_id = str(task["run_id"])
    study_name = str(task["study_name"])
    scenario_file = str(task["scenario_file"])
    prepared = _prepare_scenario(manager, scenario_file, variant, seed)
    simulation = manager.build_simulation(prepared)
    simulation.run()
    scenario_name = prepared.get("name", Path(scenario_file).stem)
    tables = _collect_run_tables(
        simulation=simulation,
        analytics=analytics,
        study_name=study_name,
        scenario_name=scenario_name,
        variant_name=variant.name,
        seed=seed,
        run_id=run_id,
        per_step_intent=bool(task.get("per_step_intent", False)),
    )
    exit_labels = {
        f"{cell[0]},{cell[1]},{cell[2]}": label
        for cell, label in simulation.exit_labels.items()
    }
    pathfinding_stats = simulation.pathfinding_stats()
    return {
        "tables": tables,
        "scenario_name": scenario_name,
        "steps": int(simulation.current_step),
        "layout_text": manager.serialize_layout(simulation.layout),
        "layout_floors": manager.serialize_layout_floors(simulation.layout),
        "layout_connectors": manager.serialize_layout_connectors(simulation.layout),
        "bottleneck_zones": [
            {
                "zone_id": zone.zone_id,
                "cells": [list(cell) for cell in zone.cells],
                "orientation": zone.orientation,
                "centroid": list(zone.centroid),
            }
            for zone in simulation.bottleneck_zones
        ],
        "exit_labels": exit_labels,
        "scenario_metadata": dict(prepared.get("metadata", {}) or {}),
        "manifest": {
            "run_id": run_id,
            "variant_name": variant.name,
            "seed": seed,
            "acceleration_backend": simulation.acceleration.name,
            "requested_acceleration_backend": simulation.acceleration.requested_backend,
            **pathfinding_stats,
            "agents_total": len(simulation.agents),
            "agents_evacuated": len(simulation.completed_agents),
        },
    }


def _prepare_scenario(
    manager: ScenarioManager,
    scenario_file: str,
    variant: StudyVariant,
    seed: int,
) -> dict[str, Any]:
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
    scenario: dict[str, Any],
    intervention: InterventionConfig,
) -> dict[str, Any]:
    updated = deepcopy(scenario)

    if intervention.type in {"corridor_narrowing", "block_cells"}:
        return manager.apply_layout_cells(
            updated, cells=list(intervention.cells), fill=manager.wall_token()
        )
    if intervention.type in {"corridor_widening", "clear_cells"}:
        return manager.apply_layout_cells(
            updated, cells=list(intervention.cells), fill=manager.empty_token()
        )
    if intervention.type == "exit_closure":
        return manager.apply_layout_cells(
            updated, cells=list(intervention.exits), fill=manager.wall_token()
        )

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
                "spawn_cells": [
                    dict(cell) if isinstance(cell, dict) else list(cell)
                    for cell in intervention.spawn_cells
                ],
            }
        )
        population["cohorts"] = cohorts
        population["total"] = int(population.get("total", 0)) + int(
            intervention.count or 0
        )
        return updated

    raise ValueError(f"Unsupported intervention type: {intervention.type}")


def _ensure_population_cohorts(
    manager: ScenarioManager, scenario: dict[str, Any]
) -> dict[str, Any]:
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


def _set_nested_value(target: dict[str, Any], path: str, value: Any) -> None:
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
    per_step_intent: bool = False,
) -> dict[str, pd.DataFrame]:
    steps_rows: list[dict[str, Any]] = []
    cells_rows: list[dict[str, Any]] = []
    intent_path_usage_rows: list[dict[str, Any]] = []
    agent_step_rows: list[dict[str, Any]] = []
    exit_rows: list[dict[str, Any]] = []
    bottleneck_rows: list[dict[str, Any]] = []
    hazard_rows: list[dict[str, Any]] = []
    agent_rows: list[dict[str, Any]] = []
    dwell_rows: list[dict[str, Any]] = []
    measurement_rows: list[dict[str, Any]] = []
    gossip_rows: list[dict[str, Any]] = []
    intervention_rows: list[dict[str, Any]] = []
    llm_decision_rows: list[dict[str, Any]] = []
    llm_call_rows: list[dict[str, Any]] = []

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
                "peak_cell_occupancy": (
                    int(step.occupancy_grid.max()) if step.occupancy_grid.size else 0
                ),
                "global_entropy": float(getattr(step, "global_entropy", 0.0)),
                "connector_flow": int(
                    sum(getattr(step, "connector_flow", {}).values())
                ),
                "connector_capacity": int(
                    sum(getattr(step, "connector_capacity", {}).values())
                ),
                "connector_queue_length": int(
                    sum(getattr(step, "connector_queue_length", {}).values())
                ),
                "connector_capacity_used": int(
                    sum(getattr(step, "connector_capacity_used", {}).values())
                ),
            }
        )

        for floor_id, grids in step.floor_grids.items():
            occupancy = grids["occupancy_grid"]
            density = grids["density_grid"]
            speed = grids["speed_grid"]
            path_usage = grids["path_usage_grid"]
            active_cells = np.argwhere(
                (occupancy > 0) | (path_usage > 0) | (speed > 0) | (density > 0)
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
                        "floor_id": floor_id,
                        "z": float(simulation.layout.floor_z(floor_id)),
                        "x": int(x),
                        "y": int(y),
                        "occupancy": int(occupancy[y, x]),
                        "density": float(density[y, x]),
                        "speed": float(speed[y, x]),
                        "path_usage": int(path_usage[y, x]),
                    }
                )

        for agent in step.agents:
            target_exit_floor = _cell_floor(agent.target_exit)
            target_exit_x, target_exit_y = _cell_xy(agent.target_exit)
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
                    "floor_id": agent.cell[0],
                    "x": float(agent.position[0]),
                    "y": float(agent.position[1]),
                    "z": float(agent.position[2]),
                    "cell_x": int(agent.cell[1]),
                    "cell_y": int(agent.cell[2]),
                    "state": agent.state,
                    "speed": float(agent.speed),
                    "local_density": float(agent.local_density),
                    "target_exit_floor": target_exit_floor,
                    "target_exit_x": target_exit_x,
                    "target_exit_y": target_exit_y,
                    "cohort_name": agent.cohort_name,
                    "group_id": agent.group_id,
                    "leader_id": agent.leader_id,
                    "family_id": agent.family_id,
                    "role_in_group": agent.role_in_group,
                    "mobility_class": agent.mobility_class,
                    "evacuation_mode": getattr(agent, "evacuation_mode", "pedestrian"),
                    "hazard_exposure": float(agent.hazard_exposure),
                    "hazard_load": float(agent.hazard_load),
                    "entropy": float(getattr(agent, "entropy", 0.0)),
                    "belief_accuracy": float(getattr(agent, "belief_accuracy", 1.0)),
                    "impairment": float(getattr(agent, "impairment", 0.0)),
                    "decision_mode": str(getattr(agent, "decision_mode", "EVACUATE")),
                    "padm_receive": int(getattr(agent, "padm_receive", 0)),
                    "padm_understand": int(getattr(agent, "padm_understand", 0)),
                    "padm_personalize": int(
                        getattr(agent, "padm_personalize", 0)
                    ),
                    "padm_decide": int(getattr(agent, "padm_decide", 0)),
                }
            )
            if per_step_intent:
                intent_path_usage_rows.append(
                    {
                        "study_name": study_name,
                        "scenario_name": scenario_name,
                        "variant_name": variant_name,
                        "seed": seed,
                        "run_id": run_id,
                        "step": step.step,
                        "time_s": step.time_s,
                        "floor_id": agent.cell[0],
                        "z": float(simulation.layout.floor_z(agent.cell[0])),
                        "x": int(agent.cell[1]),
                        "y": int(agent.cell[2]),
                        "intent": str(getattr(agent, "decision_mode", "EVACUATE")),
                        "count": 1,
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
                    "flow_cumulative": int(
                        step.exit_flow_cumulative.get(exit_label, 0)
                    ),
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
                    "z": (
                        float(hazard["pos"][2])
                        if len(hazard.get("pos", [])) >= 3
                        else 0.0
                    ),
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
                "family_id": getattr(agent, "family_id", None),
                "role_in_group": getattr(agent, "role_in_group", "solo"),
                "mobility_class": getattr(agent, "mobility_class", "standard"),
                "evacuation_mode": getattr(agent, "evacuation_mode", "pedestrian"),
                "age_band": str(getattr(agent, "age_band", "") or ""),
                "separation_anxiety_threshold": float(
                    getattr(agent, "separation_anxiety_threshold", 1.5)
                ),
                "breathing_height_m": float(getattr(agent, "breathing_height_m", 1.5)),
                "familiarity": float(getattr(agent, "familiarity", 0.5)),
                "homophily_weight": float(getattr(agent, "homophily_weight", 0.0)),
                "impairment": float(
                    getattr(getattr(agent, "physiology", None), "impairment_level", 0.0)
                ),
                "is_responder": bool(getattr(agent, "is_responder", False)),
                "is_hostile": bool(getattr(agent, "is_hostile", False)),
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
    pathfinding_stats = simulation.pathfinding_stats()
    summary_pathfinding_stats = dict(pathfinding_stats)
    strategy_counts = summary_pathfinding_stats.pop("pathfinding_strategy_counts", {})
    summary_pathfinding_stats.pop("routing_wall_time_s", None)
    summary_pathfinding_stats["pathfinding_strategy_counts_json"] = json.dumps(
        strategy_counts, sort_keys=True
    )
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
                **summary_pathfinding_stats,
                **summary_metrics,
            }
        ]
    )

    for ml in getattr(simulation, "measurement_lines", []):
        for rec in ml.records:
            measurement_rows.append(
                {
                    "study_name": study_name,
                    "scenario_name": scenario_name,
                    "variant_name": variant_name,
                    "seed": seed,
                    "run_id": run_id,
                    "line_name": ml.name,
                    "step": rec.step,
                    "time_s": rec.time_s,
                    "flow": rec.flow,
                    "density": rec.density,
                    "speed": rec.speed,
                    "n_crossing": rec.n_crossing,
                    "n_in_region": rec.n_in_region,
                }
            )

    for event in getattr(simulation, "gossip_events", []):
        gossip_rows.append(
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
                "step": event["step"],
                "time_s": event["time_s"],
                "sender_id": event["sender_id"],
                "receiver_id": event["receiver_id"],
                "distance": event["distance"],
            }
        )

    for event in getattr(simulation, "intervention_events", []):
        intervention_rows.append(
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
                "step": event.step,
                "time_s": event.time_s,
                "policy": event.policy,
                "message_type": event.message_type,
                "target_x": event.target_x,
                "target_y": event.target_y,
                "radius": event.radius,
                "recipients": event.recipients,
                "entropy_before": event.entropy_before,
                "entropy_after": event.entropy_after,
                "entropy_delta": event.entropy_after - event.entropy_before,
                "accuracy_before": event.accuracy_before,
                "accuracy_after": event.accuracy_after,
                "accuracy_delta": event.accuracy_after - event.accuracy_before,
                "mean_local_density": event.mean_local_density,
                "mean_hazard_load": event.mean_hazard_load,
                "peak_queue_length": event.peak_queue_length,
                "selected_reason": event.selected_reason,
                "target_score": event.target_score,
                "objective": event.objective,
                "generated_text": event.generated_text,
                "generation_provider": event.generation_provider,
                "generation_model": event.generation_model,
                "validation_status": event.validation_status,
                "validation_reasons": event.validation_reasons,
                "cache_key": event.cache_key,
                "cache_status": event.cache_status,
                "generated_recommended_exits": event.generated_recommended_exits,
                "generated_avoid_exits": event.generated_avoid_exits,
                "generated_confidence": event.generated_confidence,
                "used_fallback": event.used_fallback,
            }
        )

    for event in getattr(simulation, "agent_decision_events", []):
        llm_decision_rows.append(
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
                "step": event.step,
                "time_s": event.time_s,
                "agent_id": event.agent_id,
                "provider": event.provider,
                "model": event.model,
                "cache_key": event.cache_key,
                "cache_status": event.cache_status,
                "validation_status": event.validation_status,
                "validation_reasons": event.validation_reasons,
                "selected_intent": event.selected_intent,
                "target_exit_floor": event.target_exit_floor,
                "target_exit_x": event.target_exit_x,
                "target_exit_y": event.target_exit_y,
                "trust_delta": event.trust_delta,
                "avoid_congested": event.avoid_congested,
                "confidence": event.confidence,
                "rationale": event.rationale,
                "used_fallback": event.used_fallback,
                "objective": event.objective,
            }
        )

    for row in getattr(simulation, "llm_call_audit", []):
        item = dict(row)
        item.update(
            {
                "study_name": study_name,
                "scenario_name": scenario_name,
                "variant_name": variant_name,
                "seed": seed,
                "run_id": run_id,
            }
        )
        llm_call_rows.append(item)

    agents_frame = pd.DataFrame(agent_rows)

    return {
        "summary": summary_row,
        "steps": pd.DataFrame(steps_rows),
        "cells": pd.DataFrame(cells_rows),
        "intent_path_usage": _intent_path_usage_frame(intent_path_usage_rows),
        "agent_steps": pd.DataFrame(agent_step_rows),
        "agents": agents_frame,
        "equity_subgroups": equity_subgroup_metrics(agents_frame),
        "bottlenecks": pd.DataFrame(bottleneck_rows),
        "dwell_samples": pd.DataFrame(dwell_rows),
        "exits": pd.DataFrame(exit_rows),
        "hazards": pd.DataFrame(hazard_rows),
        "measurements": pd.DataFrame(measurement_rows),
        "gossip": pd.DataFrame(gossip_rows),
        "interventions": pd.DataFrame(intervention_rows),
        "llm_decisions": pd.DataFrame(llm_decision_rows),
        "llm_calls": pd.DataFrame(llm_call_rows),
    }


def _cell_floor(cell) -> str | None:
    if cell is None:
        return None
    return str(cell[0]) if len(cell) >= 3 and isinstance(cell[0], str) else None


def _cell_xy(cell) -> tuple[int | None, int | None]:
    if cell is None:
        return None, None
    if len(cell) >= 3 and isinstance(cell[0], str):
        return int(cell[1]), int(cell[2])
    return int(cell[0]), int(cell[1])


def _aggregate_summary(summary: pd.DataFrame) -> pd.DataFrame:
    if summary.empty:
        return pd.DataFrame()

    run_rows = summary[summary["record_type"] == "run"].copy()
    numeric_cols = [
        column
        for column in run_rows.columns
        if pd.api.types.is_numeric_dtype(run_rows[column]) and column not in {"seed"}
    ]

    aggregate_rows: list[dict[str, Any]] = []
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
            row[f"{column}_std"] = (
                float(group[column].std(ddof=0)) if len(group) > 1 else 0.0
            )
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
        whole[f"{column}_std"] = (
            float(run_rows[column].std(ddof=0)) if len(run_rows) > 1 else 0.0
        )
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
        return pd.DataFrame(
            columns=["time_s", "evacuated_total", "mean_speed", "mean_density"]
        )
    return (
        steps.groupby("time_s", as_index=False)[
            ["evacuated_total", "mean_speed", "mean_density"]
        ]
        .mean()
        .sort_values("time_s")
    )


def _concat(frames: Sequence[pd.DataFrame]) -> pd.DataFrame:
    frames = [frame for frame in frames if frame is not None]
    if not frames:
        return pd.DataFrame()
    return pd.concat(frames, ignore_index=True)


def _intent_path_usage_frame(rows: list[dict[str, Any]]) -> pd.DataFrame:
    if not rows:
        return pd.DataFrame()
    frame = pd.DataFrame(rows)
    group_cols = [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "floor_id",
        "z",
        "x",
        "y",
        "intent",
    ]
    return (
        frame.groupby(group_cols, as_index=False)["count"]
        .sum()
        .sort_values(["run_id", "step", "floor_id", "y", "x", "intent"])
        .reset_index(drop=True)
    )
