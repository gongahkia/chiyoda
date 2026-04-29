from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

import numpy as np
import pandas as pd


@dataclass(frozen=True)
class BottleneckFlowSummary:
    source: str
    agent_count: int
    sample_count: int
    crossing_count: int
    first_crossing_s: float
    last_crossing_s: float
    mean_flow_ped_s: float
    mean_time_headway_s: float

    def to_frame(self) -> pd.DataFrame:
        return pd.DataFrame([self.__dict__])


def load_petrack_trajectory(path: str | Path, *, frame_rate_hz: float = 25.0) -> pd.DataFrame:
    """Load a PeTrack text trajectory file into Chiyoda's trajectory schema."""
    rows: list[dict[str, float | int]] = []
    source = Path(path)
    with source.open("r") as handle:
        for line in handle:
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            parts = stripped.split()
            if len(parts) < 4:
                continue
            agent_id = int(parts[0])
            frame = int(parts[1])
            rows.append(
                {
                    "agent_id": agent_id,
                    "frame": frame,
                    "time_s": frame / float(frame_rate_hz),
                    "x": float(parts[2]),
                    "y": float(parts[3]),
                    "z": float(parts[4]) if len(parts) > 4 else 0.0,
                }
            )
    if not rows:
        raise ValueError(f"No trajectory rows found in {source}")
    return pd.DataFrame(rows)


def bottleneck_crossing_times(
    frame: pd.DataFrame,
    *,
    measurement_line: Sequence[Sequence[float]] = ((0.35, 0.0), (-0.35, 0.0)),
) -> pd.DataFrame:
    """Compute first crossing time per agent through a line segment."""
    p1 = np.array(measurement_line[0], dtype=float)
    p2 = np.array(measurement_line[1], dtype=float)
    line_vec = p2 - p1
    length = float(np.linalg.norm(line_vec))
    if length <= 1e-9:
        raise ValueError("measurement_line endpoints must be distinct")
    unit = line_vec / length
    normal = np.array([-unit[1], unit[0]])

    rows: list[dict[str, float | int]] = []
    ordered = frame.sort_values(["agent_id", "time_s"])
    for agent_id, group in ordered.groupby("agent_id", sort=False):
        previous = None
        for current in group.itertuples(index=False):
            pos = np.array([float(current.x), float(current.y)])
            side = float(np.dot(pos - p1, normal))
            if previous is not None:
                prev_pos, prev_side, prev_time = previous
                if prev_side * side < 0:
                    midpoint = (prev_pos + pos) / 2.0
                    along = float(np.dot(midpoint - p1, unit))
                    if -0.5 <= along <= length + 0.5:
                        rows.append(
                            {
                                "agent_id": int(agent_id),
                                "crossing_time_s": float(current.time_s),
                                "previous_time_s": float(prev_time),
                                "midpoint_x": float(midpoint[0]),
                                "midpoint_y": float(midpoint[1]),
                            }
                        )
                        break
            previous = (pos, side, float(current.time_s))
    return pd.DataFrame(rows)


def summarize_bottleneck_flow(
    frame: pd.DataFrame,
    *,
    source: str,
    measurement_line: Sequence[Sequence[float]] = ((0.35, 0.0), (-0.35, 0.0)),
) -> BottleneckFlowSummary:
    crossings = bottleneck_crossing_times(frame, measurement_line=measurement_line)
    crossing_times = (
        pd.to_numeric(crossings["crossing_time_s"], errors="coerce").sort_values()
        if not crossings.empty
        else pd.Series(dtype=float)
    )
    first = float(crossing_times.min()) if not crossing_times.empty else float("nan")
    last = float(crossing_times.max()) if not crossing_times.empty else float("nan")
    duration = last - first
    headways = crossing_times.diff().dropna()
    return BottleneckFlowSummary(
        source=source,
        agent_count=int(frame["agent_id"].nunique()),
        sample_count=int(len(frame)),
        crossing_count=int(len(crossing_times)),
        first_crossing_s=first,
        last_crossing_s=last,
        mean_flow_ped_s=float(len(crossing_times) / duration) if duration > 0 else float("nan"),
        mean_time_headway_s=_mean(headways),
    )


def compare_bottleneck_flow(
    simulated: BottleneckFlowSummary,
    reference: BottleneckFlowSummary,
) -> pd.DataFrame:
    rows = []
    for metric in (
        "crossing_count",
        "first_crossing_s",
        "last_crossing_s",
        "mean_flow_ped_s",
        "mean_time_headway_s",
    ):
        sim_value = float(getattr(simulated, metric))
        ref_value = float(getattr(reference, metric))
        delta = sim_value - ref_value
        rows.append(
            {
                "metric": metric,
                "simulated": sim_value,
                "reference": ref_value,
                "delta": delta,
                "pct_delta": (delta / abs(ref_value) * 100.0)
                if abs(ref_value) > 1e-12 and np.isfinite(ref_value)
                else float("nan"),
            }
        )
    return pd.DataFrame(rows)


def _mean(values: Iterable[float] | pd.Series) -> float:
    series = pd.to_numeric(pd.Series(values), errors="coerce").dropna()
    return float(series.mean()) if not series.empty else float("nan")
