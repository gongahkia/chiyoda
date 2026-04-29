from __future__ import annotations

from pathlib import Path
from typing import Iterable, Sequence

import numpy as np
import pandas as pd


REQUIRED_TRAJECTORY_COLUMNS = {"agent_id", "time_s", "x", "y"}


def load_trajectory_table(path: str | Path) -> pd.DataFrame:
    """Load an agent trajectory table from CSV, Parquet, or a study bundle."""
    source = Path(path)
    if source.is_dir():
        tables_dir = source / "tables"
        parquet = tables_dir / "agent_steps.parquet"
        csv = tables_dir / "agent_steps.csv"
        if parquet.exists():
            return pd.read_parquet(parquet)
        if csv.exists():
            return pd.read_csv(csv)
        raise FileNotFoundError(f"No agent_steps table found under {tables_dir}")
    if source.suffix.lower() == ".parquet":
        return pd.read_parquet(source)
    if source.suffix.lower() == ".csv":
        return pd.read_csv(source)
    raise ValueError(f"Unsupported trajectory table format: {source}")


def summarize_trajectory_frame(frame: pd.DataFrame) -> pd.Series:
    """Summarize first-order trajectory statistics without replacing PedPy."""
    _validate_trajectory_columns(frame)
    if frame.empty:
        return pd.Series(_empty_summary())

    ordered = frame.sort_values(["agent_id", "time_s"]).copy()
    speeds = _speed_series(ordered)
    path_lengths = _path_lengths(ordered)
    durations = ordered.groupby("agent_id")["time_s"].agg(
        lambda s: float(s.max() - s.min())
    )
    displacements = _displacements(ordered)
    densities = (
        pd.to_numeric(ordered["local_density"], errors="coerce")
        if "local_density" in ordered.columns
        else pd.Series(dtype=float)
    )

    return pd.Series(
        {
            "agent_count": int(ordered["agent_id"].nunique()),
            "sample_count": int(len(ordered)),
            "duration_s": float(ordered["time_s"].max() - ordered["time_s"].min()),
            "mean_agent_duration_s": _mean(durations),
            "mean_path_length_m": _mean(path_lengths),
            "p95_path_length_m": _quantile(path_lengths, 0.95),
            "mean_displacement_m": _mean(displacements),
            "mean_speed_mps": _mean(speeds),
            "p95_speed_mps": _quantile(speeds, 0.95),
            "mean_local_density": _mean(densities),
            "p95_local_density": _quantile(densities, 0.95),
        }
    )


def compare_trajectory_reference(
    simulated: pd.DataFrame,
    reference: pd.DataFrame,
    *,
    group_columns: Sequence[str] = ("variant_name",),
) -> pd.DataFrame:
    """Compare simulated trajectories against one reference trajectory table."""
    reference_summary = summarize_trajectory_frame(reference)
    present_groups = [column for column in group_columns if column in simulated.columns]

    rows: list[dict[str, object]] = []
    groups = (
        [((), simulated)]
        if not present_groups
        else simulated.groupby(present_groups, dropna=False, sort=False)
    )

    for group_key, group in groups:
        if present_groups and not isinstance(group_key, tuple):
            group_key = (group_key,)
        group_values = dict(zip(present_groups, group_key if present_groups else ()))
        simulated_summary = summarize_trajectory_frame(group)
        for metric, reference_value in reference_summary.items():
            simulated_value = simulated_summary[metric]
            delta = _delta(simulated_value, reference_value)
            pct_delta = _pct_delta(delta, reference_value)
            rows.append(
                {
                    **group_values,
                    "metric": metric,
                    "simulated": simulated_value,
                    "reference": reference_value,
                    "delta": delta,
                    "pct_delta": pct_delta,
                }
            )

    return pd.DataFrame(rows)


def external_tool_trajectory_frame(
    frame: pd.DataFrame,
    *,
    frame_rate_hz: float = 10.0,
) -> pd.DataFrame:
    """Normalize Chiyoda trajectories for pedestrian-analysis tool exports."""
    _validate_trajectory_columns(frame)
    if frame_rate_hz <= 0:
        raise ValueError("frame_rate_hz must be positive")
    ordered = frame.sort_values(["time_s", "agent_id"]).copy()
    return pd.DataFrame(
        {
            "agent_id": pd.to_numeric(ordered["agent_id"], errors="raise").astype(int),
            "frame": (
                pd.to_numeric(ordered["time_s"], errors="raise") * float(frame_rate_hz)
            ).round().astype(int),
            "time_s": pd.to_numeric(ordered["time_s"], errors="raise"),
            "x": pd.to_numeric(ordered["x"], errors="raise"),
            "y": pd.to_numeric(ordered["y"], errors="raise"),
            "z": 0.0,
        }
    )


def export_jupedsim_trajectory(
    frame: pd.DataFrame,
    path: str | Path,
    *,
    frame_rate_hz: float = 10.0,
) -> Path:
    """Export a JuPedSim-style plain trajectory table: id, frame, x, y, z."""
    normalized = external_tool_trajectory_frame(frame, frame_rate_hz=frame_rate_hz)
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    normalized.rename(columns={"agent_id": "id"})[
        ["id", "frame", "x", "y", "z"]
    ].to_csv(output, sep="\t", index=False)
    return output


def export_vadere_trajectory(
    frame: pd.DataFrame,
    path: str | Path,
    *,
    frame_rate_hz: float = 10.0,
) -> Path:
    """Export a compact Vadere-compatible point table."""
    normalized = external_tool_trajectory_frame(frame, frame_rate_hz=frame_rate_hz)
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    normalized.rename(
        columns={"agent_id": "pedestrianId", "frame": "timeStep", "time_s": "simTime"}
    )[["timeStep", "pedestrianId", "simTime", "x", "y"]].to_csv(output, index=False)
    return output


def _validate_trajectory_columns(frame: pd.DataFrame) -> None:
    missing = REQUIRED_TRAJECTORY_COLUMNS.difference(frame.columns)
    if missing:
        raise ValueError(
            f"Trajectory table is missing required columns: {sorted(missing)}"
        )


def _speed_series(frame: pd.DataFrame) -> pd.Series:
    if "speed" in frame.columns:
        return pd.to_numeric(frame["speed"], errors="coerce")

    pieces: list[pd.Series] = []
    for _, group in frame.groupby("agent_id", sort=False):
        dx = pd.to_numeric(group["x"], errors="coerce").diff()
        dy = pd.to_numeric(group["y"], errors="coerce").diff()
        dt = pd.to_numeric(group["time_s"], errors="coerce").diff()
        speed = np.sqrt((dx * dx) + (dy * dy)) / dt.replace(0.0, np.nan)
        pieces.append(speed)
    if not pieces:
        return pd.Series(dtype=float)
    return pd.concat(pieces, ignore_index=True)


def _path_lengths(frame: pd.DataFrame) -> pd.Series:
    values: list[float] = []
    for _, group in frame.groupby("agent_id", sort=False):
        x = pd.to_numeric(group["x"], errors="coerce")
        y = pd.to_numeric(group["y"], errors="coerce")
        distances = np.sqrt((x.diff() ** 2) + (y.diff() ** 2))
        values.append(float(distances.fillna(0.0).sum()))
    return pd.Series(values, dtype=float)


def _displacements(frame: pd.DataFrame) -> pd.Series:
    values: list[float] = []
    for _, group in frame.groupby("agent_id", sort=False):
        first = group.iloc[0]
        last = group.iloc[-1]
        values.append(
            float(
                np.hypot(
                    float(last["x"]) - float(first["x"]),
                    float(last["y"]) - float(first["y"]),
                )
            )
        )
    return pd.Series(values, dtype=float)


def _mean(values: Iterable[float] | pd.Series) -> float:
    series = pd.to_numeric(pd.Series(values), errors="coerce").dropna()
    return float(series.mean()) if not series.empty else float("nan")


def _quantile(values: Iterable[float] | pd.Series, q: float) -> float:
    series = pd.to_numeric(pd.Series(values), errors="coerce").dropna()
    return float(series.quantile(q)) if not series.empty else float("nan")


def _delta(simulated_value: object, reference_value: object) -> float:
    return float(simulated_value) - float(reference_value)


def _pct_delta(delta: float, reference_value: object) -> float:
    reference = float(reference_value)
    if abs(reference) < 1e-12 or np.isnan(reference):
        return float("nan")
    return float((delta / abs(reference)) * 100.0)


def _empty_summary() -> dict[str, float]:
    return {
        "agent_count": 0.0,
        "sample_count": 0.0,
        "duration_s": float("nan"),
        "mean_agent_duration_s": float("nan"),
        "mean_path_length_m": float("nan"),
        "p95_path_length_m": float("nan"),
        "mean_displacement_m": float("nan"),
        "mean_speed_mps": float("nan"),
        "p95_speed_mps": float("nan"),
        "mean_local_density": float("nan"),
        "p95_local_density": float("nan"),
    }
