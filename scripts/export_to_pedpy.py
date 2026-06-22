"""Export Chiyoda study trajectories to a PedPy-compatible SQLite file.

PedPy expects (id, frame, x, y) rows. We read ``out/<study>/trajectories.csv``
(or an equivalent CSV produced by ``chiyoda run``) and write a SQLite database
with a ``trajectory_data`` table indexed by ``(id, frame)``.

The script prefers ``pedpy.io.trajectory_to_sqlite`` when PedPy is
installed, and falls back to a pandas/sqlite3 path that emits the same
schema otherwise.
"""

from __future__ import annotations

import argparse
import sqlite3
from pathlib import Path

import pandas as pd


def export(csv_path: Path, sqlite_path: Path) -> Path:
    if not csv_path.exists():
        raise FileNotFoundError(csv_path)
    frame = pd.read_csv(csv_path)
    rename_map = {
        "agent_id": "id",
        "step": "frame",
        "pos_x": "x",
        "pos_y": "y",
    }
    for src, dst in rename_map.items():
        if src in frame.columns and dst not in frame.columns:
            frame = frame.rename(columns={src: dst})
    required = {"id", "frame", "x", "y"}
    missing = required - set(frame.columns)
    if missing:
        raise ValueError(
            f"Trajectories CSV missing required columns {sorted(missing)}; "
            f"have {sorted(frame.columns)}"
        )
    frame = frame[["id", "frame", "x", "y"]].astype(
        {"id": "int64", "frame": "int64", "x": "float64", "y": "float64"}
    )

    try:
        from pedpy.io import trajectory_to_sqlite

        trajectory_to_sqlite(frame, sqlite_path)
        return sqlite_path
    except Exception:
        pass

    sqlite_path.parent.mkdir(parents=True, exist_ok=True)
    if sqlite_path.exists():
        sqlite_path.unlink()
    with sqlite3.connect(sqlite_path) as conn:
        frame.to_sql("trajectory_data", conn, if_exists="replace", index=False)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS trajectory_id_frame "
            "ON trajectory_data (id, frame)"
        )
    return sqlite_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "csv_path",
        type=Path,
        help="Path to trajectories.csv (e.g. out/<study>/trajectories.csv)",
    )
    parser.add_argument(
        "sqlite_path",
        type=Path,
        help="Destination SQLite file (will be overwritten if it exists)",
    )
    args = parser.parse_args()
    written = export(args.csv_path, args.sqlite_path)
    print(f"wrote {written}")


if __name__ == "__main__":
    main()
