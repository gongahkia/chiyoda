from __future__ import annotations

from dataclasses import dataclass, field
import json
from pathlib import Path
from typing import Dict

import pandas as pd


def _empty_frame() -> pd.DataFrame:
    return pd.DataFrame()


def _write_table(frame: pd.DataFrame, directory: Path, name: str, table_format: str) -> None:
    target = directory / f"{name}.{table_format}"
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
        return pd.read_csv(csv)
    return pd.DataFrame()


@dataclass
class StudyBundle:
    metadata: Dict[str, object]
    summary: pd.DataFrame
    steps: pd.DataFrame
    cells: pd.DataFrame
    agent_steps: pd.DataFrame
    agents: pd.DataFrame
    bottlenecks: pd.DataFrame
    dwell_samples: pd.DataFrame
    exits: pd.DataFrame
    hazards: pd.DataFrame
    measurements: pd.DataFrame = field(default_factory=_empty_frame)
    gossip: pd.DataFrame = field(default_factory=_empty_frame)

    def export(self, output_dir: str | Path, table_formats: tuple[str, ...] = ("parquet", "csv")) -> Path:
        out = Path(output_dir)
        tables_dir = out / "tables"
        out.mkdir(parents=True, exist_ok=True)
        tables_dir.mkdir(parents=True, exist_ok=True)

        (out / "metadata.json").write_text(
            json.dumps(self.metadata, indent=2, default=str) + "\n"
        )

        tables = self.tables()
        for table_name, frame in tables.items():
            for table_format in table_formats:
                _write_table(frame, tables_dir, table_name, table_format)
        return out

    def tables(self) -> Dict[str, pd.DataFrame]:
        return {
            "summary": self.summary,
            "steps": self.steps,
            "cells": self.cells,
            "agent_steps": self.agent_steps,
            "agents": self.agents,
            "bottlenecks": self.bottlenecks,
            "dwell_samples": self.dwell_samples,
            "exits": self.exits,
            "hazards": self.hazards,
            "measurements": self.measurements,
            "gossip": self.gossip,
        }

    @classmethod
    def load(cls, output_dir: str | Path) -> "StudyBundle":
        root = Path(output_dir)
        tables_dir = root / "tables"
        metadata = json.loads((root / "metadata.json").read_text())
        return cls(
            metadata=metadata,
            summary=_read_table(tables_dir, "summary"),
            steps=_read_table(tables_dir, "steps"),
            cells=_read_table(tables_dir, "cells"),
            agent_steps=_read_table(tables_dir, "agent_steps"),
            agents=_read_table(tables_dir, "agents"),
            bottlenecks=_read_table(tables_dir, "bottlenecks"),
            dwell_samples=_read_table(tables_dir, "dwell_samples"),
            exits=_read_table(tables_dir, "exits"),
            hazards=_read_table(tables_dir, "hazards"),
            measurements=_read_table(tables_dir, "measurements"),
            gossip=_read_table(tables_dir, "gossip"),
        )


@dataclass
class ComparisonResult:
    metadata: Dict[str, object]
    summary: pd.DataFrame
    timeseries: pd.DataFrame
    metrics: pd.DataFrame

    def export(self, output_dir: str | Path, table_formats: tuple[str, ...] = ("parquet", "csv")) -> Path:
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

    def tables(self) -> Dict[str, pd.DataFrame]:
        return {
            "summary": self.summary,
            "timeseries": self.timeseries,
            "metrics": self.metrics,
        }
