from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

import pandas as pd
from pandas.errors import EmptyDataError

from chiyoda.information.llm import (
    LLM_AUDIT_CHAIN_HASH,
    LLM_AUDIT_CHAIN_PREV_HASH,
    with_llm_audit_chain,
)

TABLE_COLUMNS: dict[str, list[str]] = {
    "summary": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "record_type",
    ],
    "steps": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "evacuated_total",
        "remaining",
        "pending_release",
        "mean_speed",
        "mean_density",
        "peak_cell_occupancy",
        "global_entropy",
    ],
    "cells": [
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
        "occupancy",
        "density",
        "speed",
        "path_usage",
    ],
    "intent_path_usage": [
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
        "count",
    ],
    "agent_steps": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "agent_id",
        "floor_id",
        "x",
        "y",
        "z",
        "cell_x",
        "cell_y",
        "state",
        "speed",
        "local_density",
        "target_exit_floor",
        "target_exit_x",
        "target_exit_y",
        "cohort_name",
        "group_id",
        "leader_id",
        "family_id",
        "role_in_group",
        "mobility_class",
        "evacuation_mode",
        "hazard_exposure",
        "hazard_load",
        "visibility",
        "flood_depth_m",
        "environment_speed_factor",
        "entropy",
        "belief_accuracy",
        "impairment",
        "decision_mode",
        "padm_receive",
        "padm_understand",
        "padm_personalize",
        "padm_decide",
    ],
    "agents": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "agent_id",
        "cohort_name",
        "personality",
        "calmness",
        "release_step",
        "group_id",
        "leader_id",
        "assisted_agent_id",
        "family_id",
        "role_in_group",
        "mobility_class",
        "evacuation_mode",
        "age_band",
        "separation_anxiety_threshold",
        "breathing_height_m",
        "familiarity",
        "homophily_weight",
        "impairment",
        "is_responder",
        "is_hostile",
        "evacuated",
        "travel_time_s",
        "hazard_exposure",
        "hazard_risk",
        "evacuated_via",
        "base_speed",
    ],
    "equity_subgroups": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "subgroup_type",
        "subgroup_tag",
        "subgroup_label",
        "agent_count",
        "evacuated_count",
        "remaining_count",
        "evacuation_rate",
        "run_evacuation_rate",
        "evacuation_rate_gap_vs_run",
        "mean_travel_time_s",
        "p95_travel_time_s",
        "run_mean_travel_time_s",
        "travel_time_gap_vs_run_s",
        "equity_time_gap_s",
        "mean_hazard_exposure",
        "p95_hazard_exposure",
        "mean_impairment",
        "mean_familiarity",
    ],
    "bottlenecks": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "zone_id",
        "occupancy",
        "inflow",
        "outflow",
        "queue_length",
        "mean_dwell_s",
        "mean_speed",
        "mean_density",
    ],
    "dwell_samples": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "zone_id",
        "dwell_s",
    ],
    "exits": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "exit_label",
        "flow_step",
        "flow_cumulative",
    ],
    "hazards": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "hazard_id",
        "kind",
        "x",
        "y",
        "z",
        "radius",
        "severity",
    ],
    "measurements": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "line_name",
        "step",
        "time_s",
        "flow",
        "density",
        "speed",
        "n_crossing",
        "n_in_region",
    ],
    "gossip": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "sender_id",
        "receiver_id",
        "distance",
    ],
    "interventions": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "policy",
        "message_type",
        "target_x",
        "target_y",
        "radius",
        "recipients",
        "entropy_before",
        "entropy_after",
        "entropy_delta",
        "accuracy_before",
        "accuracy_after",
        "accuracy_delta",
        "mean_local_density",
        "mean_hazard_load",
        "peak_queue_length",
        "selected_reason",
        "target_score",
        "objective",
        "generated_text",
        "generation_provider",
        "generation_model",
        "validation_status",
        "validation_reasons",
        "cache_key",
        "cache_status",
        "generated_recommended_exits",
        "generated_avoid_exits",
        "generated_confidence",
        "used_fallback",
    ],
    "llm_decisions": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "agent_id",
        "provider",
        "model",
        "cache_key",
        "cache_status",
        "validation_status",
        "validation_reasons",
        "selected_intent",
        "target_exit_floor",
        "target_exit_x",
        "target_exit_y",
        "trust_delta",
        "avoid_congested",
        "confidence",
        "rationale",
        "used_fallback",
        "objective",
    ],
    "llm_calls": [
        "study_name",
        "scenario_name",
        "variant_name",
        "seed",
        "run_id",
        "step",
        "time_s",
        "surface",
        "policy",
        "agent_id",
        "provider",
        "model",
        "cache_key",
        "cache_status",
        "validation_status",
        "validation_reasons",
        "judge_status",
        "judge_safety",
        "judge_specificity",
        "judge_alignment",
        "judge_reasons",
        "judge_provider",
        "used_fallback",
        "objective",
        "prompt_style",
        "target_x",
        "target_y",
        "estimated_input_tokens",
        "estimated_output_tokens",
        "estimated_total_tokens",
        "estimated_usd",
        "budget_reason",
        "raw_input_tokens",
        "raw_output_tokens",
        "raw_total_tokens",
        LLM_AUDIT_CHAIN_PREV_HASH,
        LLM_AUDIT_CHAIN_HASH,
    ],
}


def _empty_frame() -> pd.DataFrame:
    return pd.DataFrame()


def _write_table(
    frame: pd.DataFrame, directory: Path, name: str, table_format: str
) -> None:
    target = directory / f"{name}.{table_format}"
    frame = _with_known_columns(name, frame)
    if table_format == "parquet":
        frame.to_parquet(target, index=False)
    elif table_format == "csv":
        frame.to_csv(target, index=False)
    else:
        raise ValueError(f"Unsupported table format: {table_format}")


def _read_table(directory: Path, name: str) -> pd.DataFrame:
    parquet = directory / f"{name}.parquet"
    csv = directory / f"{name}.csv"
    if parquet.exists():
        return pd.read_parquet(parquet)
    if csv.exists():
        try:
            return pd.read_csv(csv)
        except EmptyDataError:
            return pd.DataFrame(columns=TABLE_COLUMNS.get(name, []))
    return pd.DataFrame(columns=TABLE_COLUMNS.get(name, []))


def _with_known_columns(name: str, frame: pd.DataFrame) -> pd.DataFrame:
    columns = TABLE_COLUMNS.get(name)
    if columns is None or len(frame.columns) > 0:
        return frame
    return pd.DataFrame(columns=columns)


@dataclass
class StudyBundle:
    metadata: dict[str, object]
    summary: pd.DataFrame
    steps: pd.DataFrame
    cells: pd.DataFrame
    agent_steps: pd.DataFrame
    agents: pd.DataFrame
    bottlenecks: pd.DataFrame
    dwell_samples: pd.DataFrame
    exits: pd.DataFrame
    hazards: pd.DataFrame
    intent_path_usage: pd.DataFrame = field(default_factory=_empty_frame)
    equity_subgroups: pd.DataFrame = field(default_factory=_empty_frame)
    measurements: pd.DataFrame = field(default_factory=_empty_frame)
    gossip: pd.DataFrame = field(default_factory=_empty_frame)
    interventions: pd.DataFrame = field(default_factory=_empty_frame)
    llm_decisions: pd.DataFrame = field(default_factory=_empty_frame)
    llm_calls: pd.DataFrame = field(default_factory=_empty_frame)

    def __post_init__(self) -> None:
        for table_name in self.tables():
            setattr(
                self,
                table_name,
                _with_known_columns(table_name, getattr(self, table_name)),
            )

    def export(
        self,
        output_dir: str | Path,
        table_formats: tuple[str, ...] = ("parquet", "csv"),
    ) -> Path:
        out = Path(output_dir)
        tables_dir = out / "tables"
        out.mkdir(parents=True, exist_ok=True)
        tables_dir.mkdir(parents=True, exist_ok=True)

        from chiyoda.analysis.reports import llm_cost_report

        self.metadata = dict(self.metadata)
        self.metadata["llm_cost_report"] = llm_cost_report(self.llm_calls)
        (out / "metadata.json").write_text(
            json.dumps(self.metadata, indent=2, default=str) + "\n"
        )

        tables = self.tables()
        tables["llm_calls"] = with_llm_audit_chain(
            _with_known_columns("llm_calls", tables["llm_calls"])
        )
        for table_name, frame in tables.items():
            for table_format in table_formats:
                _write_table(frame, tables_dir, table_name, table_format)
        return out

    def tables(self) -> dict[str, pd.DataFrame]:
        return {
            "summary": self.summary,
            "steps": self.steps,
            "cells": self.cells,
            "intent_path_usage": self.intent_path_usage,
            "agent_steps": self.agent_steps,
            "agents": self.agents,
            "equity_subgroups": self.equity_subgroups,
            "bottlenecks": self.bottlenecks,
            "dwell_samples": self.dwell_samples,
            "exits": self.exits,
            "hazards": self.hazards,
            "measurements": self.measurements,
            "gossip": self.gossip,
            "interventions": self.interventions,
            "llm_decisions": self.llm_decisions,
            "llm_calls": self.llm_calls,
        }

    @classmethod
    def load(cls, output_dir: str | Path) -> StudyBundle:
        root = Path(output_dir)
        tables_dir = root / "tables"
        metadata = json.loads((root / "metadata.json").read_text())
        return cls(
            metadata=metadata,
            summary=_read_table(tables_dir, "summary"),
            steps=_read_table(tables_dir, "steps"),
            cells=_read_table(tables_dir, "cells"),
            intent_path_usage=_read_table(tables_dir, "intent_path_usage"),
            agent_steps=_read_table(tables_dir, "agent_steps"),
            agents=_read_table(tables_dir, "agents"),
            equity_subgroups=_read_table(tables_dir, "equity_subgroups"),
            bottlenecks=_read_table(tables_dir, "bottlenecks"),
            dwell_samples=_read_table(tables_dir, "dwell_samples"),
            exits=_read_table(tables_dir, "exits"),
            hazards=_read_table(tables_dir, "hazards"),
            measurements=_read_table(tables_dir, "measurements"),
            gossip=_read_table(tables_dir, "gossip"),
            interventions=_read_table(tables_dir, "interventions"),
            llm_decisions=_read_table(tables_dir, "llm_decisions"),
            llm_calls=_read_table(tables_dir, "llm_calls"),
        )


@dataclass
class ComparisonResult:
    metadata: dict[str, object]
    summary: pd.DataFrame
    timeseries: pd.DataFrame
    metrics: pd.DataFrame

    def export(
        self,
        output_dir: str | Path,
        table_formats: tuple[str, ...] = ("parquet", "csv"),
    ) -> Path:
        out = Path(output_dir)
        tables_dir = out / "tables"
        out.mkdir(parents=True, exist_ok=True)
        tables_dir.mkdir(parents=True, exist_ok=True)

        (out / "metadata.json").write_text(
            json.dumps(self.metadata, indent=2, default=str) + "\n"
        )

        for table_name, frame in self.tables().items():
            for table_format in table_formats:
                _write_table(frame, tables_dir, table_name, table_format)
        return out

    def tables(self) -> dict[str, pd.DataFrame]:
        return {
            "summary": self.summary,
            "timeseries": self.timeseries,
            "metrics": self.metrics,
        }
