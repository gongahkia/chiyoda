from __future__ import annotations

import sqlite3
from pathlib import Path

import pandas as pd
import pytest

from scripts.export_to_pedpy import export


def _write_trajectories_csv(path: Path) -> None:
    frame = pd.DataFrame(
        {
            "id": [1, 1, 2, 2, 3],
            "frame": [0, 1, 0, 1, 0],
            "x": [0.0, 1.0, 2.0, 3.0, 4.0],
            "y": [0.0, 0.5, 1.0, 1.5, 2.0],
        }
    )
    frame.to_csv(path, index=False)


def test_export_to_pedpy_roundtrip_pandas_fallback(tmp_path):
    csv_path = tmp_path / "trajectories.csv"
    sqlite_path = tmp_path / "trajectories.sqlite"
    _write_trajectories_csv(csv_path)
    export(csv_path, sqlite_path)
    assert sqlite_path.exists()
    with sqlite3.connect(sqlite_path) as conn:
        read = pd.read_sql_query(
            "SELECT id, frame, x, y FROM trajectory_data ORDER BY id, frame",
            conn,
        )
    assert list(read.columns) == ["id", "frame", "x", "y"]
    assert len(read) == 5
    assert read.iloc[0]["x"] == 0.0
    assert read.iloc[-1]["y"] == 2.0


def test_export_to_pedpy_renames_chiyoda_columns(tmp_path):
    csv_path = tmp_path / "trajectories.csv"
    sqlite_path = tmp_path / "trajectories.sqlite"
    pd.DataFrame(
        {
            "agent_id": [1, 1],
            "step": [0, 1],
            "pos_x": [0.0, 1.0],
            "pos_y": [0.0, 0.5],
        }
    ).to_csv(csv_path, index=False)
    export(csv_path, sqlite_path)
    with sqlite3.connect(sqlite_path) as conn:
        read = pd.read_sql_query("SELECT * FROM trajectory_data", conn)
    assert set(read.columns) == {"id", "frame", "x", "y"}


def test_export_to_pedpy_with_pedpy_when_available(tmp_path):
    pedpy = pytest.importorskip("pedpy")  # noqa: F841
    csv_path = tmp_path / "trajectories.csv"
    sqlite_path = tmp_path / "trajectories.sqlite"
    _write_trajectories_csv(csv_path)
    export(csv_path, sqlite_path)
    assert sqlite_path.exists()
