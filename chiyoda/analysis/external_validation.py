from __future__ import annotations

from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from math import sqrt
from pathlib import Path

import numpy as np
import pandas as pd

from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager

RIMEA_VALIDATION_CASES = tuple(
    Path(f"scenarios/validation_rimea_{case:02d}.yaml") for case in range(1, 11)
)
RIMEA_VALIDATION_SEEDS = (42, 43, 44, 45, 46)
DEFAULT_DENSITY_BANDS = (
    ("low", 0.0, 1.0),
    ("medium", 1.0, 2.0),
    ("high", 2.0, float("inf")),
)
KS_PVALUE_SOFT_FAIL_THRESHOLD = 0.01


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


def load_petrack_trajectory(
    path: str | Path, *, frame_rate_hz: float = 25.0
) -> pd.DataFrame:
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
        mean_flow_ped_s=(
            float(len(crossing_times) / duration) if duration > 0 else float("nan")
        ),
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
                "pct_delta": (
                    (delta / abs(ref_value) * 100.0)
                    if abs(ref_value) > 1e-12 and np.isfinite(ref_value)
                    else float("nan")
                ),
            }
        )
    return pd.DataFrame(rows)


def bottleneck_travel_times_by_density(
    frame: pd.DataFrame,
    *,
    measurement_line: Sequence[Sequence[float]] = ((0.35, 0.0), (-0.35, 0.0)),
    density_radius_m: float = 1.0,
    density_bands: Sequence[
        Sequence[object] | dict[str, object]
    ] = DEFAULT_DENSITY_BANDS,
) -> pd.DataFrame:
    crossings = bottleneck_crossing_times(frame, measurement_line=measurement_line)
    bands = _density_bands(density_bands)
    columns = [
        "agent_id",
        "entry_time_s",
        "crossing_time_s",
        "travel_time_s",
        "local_density_ped_m2",
        "density_band",
    ]
    if crossings.empty:
        return pd.DataFrame(columns=columns)

    first_times = (
        frame.groupby("agent_id", sort=False)["time_s"].min().astype(float).to_dict()
    )
    rows: list[dict[str, float | int | str]] = []
    for crossing in crossings.itertuples(index=False):
        agent_id = int(crossing.agent_id)
        crossing_time = float(crossing.crossing_time_s)
        entry_time = float(first_times.get(agent_id, crossing_time))
        density = _local_density_at_time(
            frame,
            time_s=crossing_time,
            center=(float(crossing.midpoint_x), float(crossing.midpoint_y)),
            radius_m=density_radius_m,
        )
        rows.append(
            {
                "agent_id": agent_id,
                "entry_time_s": entry_time,
                "crossing_time_s": crossing_time,
                "travel_time_s": crossing_time - entry_time,
                "local_density_ped_m2": density,
                "density_band": _density_band(density, bands),
            }
        )
    return pd.DataFrame(rows, columns=columns)


def compare_density_band_travel_times(
    simulated: pd.DataFrame,
    reference: pd.DataFrame,
    *,
    simulated_line: Sequence[Sequence[float]] = ((4.0, 6.0), (6.0, 6.0)),
    reference_line: Sequence[Sequence[float]] = ((0.35, 0.0), (-0.35, 0.0)),
    density_radius_m: float = 1.0,
    density_bands: Sequence[
        Sequence[object] | dict[str, object]
    ] = DEFAULT_DENSITY_BANDS,
    pvalue_threshold: float = KS_PVALUE_SOFT_FAIL_THRESHOLD,
) -> pd.DataFrame:
    bands = _density_bands(density_bands)
    simulated_distribution = bottleneck_travel_times_by_density(
        simulated,
        measurement_line=simulated_line,
        density_radius_m=density_radius_m,
        density_bands=bands,
    )
    reference_distribution = bottleneck_travel_times_by_density(
        reference,
        measurement_line=reference_line,
        density_radius_m=density_radius_m,
        density_bands=bands,
    )
    rows: list[dict[str, float | int | str | bool]] = []
    for name, lower, upper in bands:
        sim_values = _band_values(simulated_distribution, name)
        ref_values = _band_values(reference_distribution, name)
        statistic, pvalue = _ks_2samp(sim_values, ref_values)
        finite_pvalue = bool(np.isfinite(pvalue))
        rows.append(
            {
                "density_band": name,
                "density_min_ped_m2": lower,
                "density_max_ped_m2": upper,
                "simulated_count": int(len(sim_values)),
                "reference_count": int(len(ref_values)),
                "simulated_mean_travel_time_s": _mean(sim_values),
                "reference_mean_travel_time_s": _mean(ref_values),
                "ks_statistic": statistic,
                "ks_pvalue": pvalue,
                "pvalue_soft_fail_threshold": float(pvalue_threshold),
                "soft_fail": bool(finite_pvalue and pvalue < pvalue_threshold),
                "status": "ok" if finite_pvalue else "insufficient_samples",
            }
        )
    return pd.DataFrame(rows)


def run_rimea_validation_scenarios(
    scenario_files: Sequence[str | Path] = RIMEA_VALIDATION_CASES,
    *,
    seeds: Sequence[int] = RIMEA_VALIDATION_SEEDS,
) -> pd.DataFrame:
    rows: list[dict[str, object]] = []
    manager = ScenarioManager()
    for scenario_file in scenario_files:
        path = Path(scenario_file)
        for seed in seeds:
            scenario = manager.load_config(str(path))
            scenario.setdefault("simulation", {})
            scenario["simulation"]["random_seed"] = int(seed)
            simulation = manager.build_simulation(scenario)
            simulation.run()
            assertions = evaluate_scenario_assertions(scenario, simulation)
            travel_times = [
                float(value) for value in getattr(simulation, "travel_times_s", [])
            ]
            rows.append(
                {
                    "case": _rimea_case_id(path),
                    "scenario": str(scenario.get("name", path.stem)),
                    "seed": int(seed),
                    "ok": bool(assertions.ok),
                    "assertion_issue_count": len(assertions.issues),
                    "evacuated": int(len(simulation.completed_agents)),
                    "remaining": int(
                        len(
                            [
                                agent
                                for agent in simulation.agents
                                if not getattr(agent, "has_evacuated", False)
                                and not getattr(agent, "is_responder", False)
                            ]
                        )
                    ),
                    "evacuation_time_s": (
                        float(max(travel_times)) if travel_times else float("nan")
                    ),
                    "mean_travel_time_s": _mean(travel_times),
                }
            )
    return pd.DataFrame(rows)


def summarize_rimea_validation_runs(runs: pd.DataFrame) -> pd.DataFrame:
    rows: list[dict[str, object]] = []
    if runs.empty:
        return pd.DataFrame()
    for (case, scenario), group in runs.groupby(["case", "scenario"], sort=True):
        evacuation_time = pd.to_numeric(
            group["evacuation_time_s"], errors="coerce"
        ).dropna()
        mean_travel_time = pd.to_numeric(
            group["mean_travel_time_s"], errors="coerce"
        ).dropna()
        rows.append(
            {
                "case": int(case),
                "scenario": str(scenario),
                "seed_count": int(group["seed"].nunique()),
                "pass_count": int(group["ok"].sum()),
                "run_count": int(len(group)),
                "evacuated_min": int(group["evacuated"].min()),
                "evacuated_max": int(group["evacuated"].max()),
                "remaining_max": int(group["remaining"].max()),
                "evacuation_time_mean_s": _mean(evacuation_time),
                "evacuation_time_ci95_s": _ci95(evacuation_time),
                "mean_travel_time_mean_s": _mean(mean_travel_time),
                "mean_travel_time_ci95_s": _ci95(mean_travel_time),
            }
        )
    return pd.DataFrame(rows)


def _mean(values: Iterable[float] | pd.Series) -> float:
    series = pd.to_numeric(pd.Series(values), errors="coerce").dropna()
    return float(series.mean()) if not series.empty else float("nan")


def _ci95(values: Iterable[float] | pd.Series) -> float:
    series = pd.to_numeric(pd.Series(values), errors="coerce").dropna()
    if len(series) < 2:
        return 0.0
    return float(1.96 * series.std(ddof=1) / sqrt(len(series)))


def _density_bands(
    bands: Sequence[Sequence[object] | dict[str, object]],
) -> tuple[tuple[str, float, float], ...]:
    parsed: list[tuple[str, float, float]] = []
    for band in bands:
        if isinstance(band, dict):
            name = str(band["name"])
            lower = float(band.get("min_ped_m2", band.get("min", 0.0)))
            upper_value = band.get("max_ped_m2", band.get("max", float("inf")))
            upper = float(upper_value) if upper_value is not None else float("inf")
        else:
            name = str(band[0])
            lower = float(band[1])
            upper = float(band[2])
        parsed.append((name, lower, upper))
    return tuple(parsed)


def _density_band(density: float, bands: Sequence[tuple[str, float, float]]) -> str:
    for name, lower, upper in bands:
        if lower <= density < upper:
            return name
    return "out_of_range"


def _local_density_at_time(
    frame: pd.DataFrame,
    *,
    time_s: float,
    center: Sequence[float],
    radius_m: float,
) -> float:
    if frame.empty:
        return 0.0
    radius = max(float(radius_m), 1e-9)
    times = pd.to_numeric(frame["time_s"], errors="coerce").dropna().unique()
    if len(times) == 0:
        return 0.0
    nearest = float(times[int(np.argmin(np.abs(times - float(time_s))))])
    sample = frame[np.isclose(frame["time_s"].astype(float), nearest)]
    if sample.empty:
        return 0.0
    positions = sample[["x", "y"]].to_numpy(dtype=float)
    distances = np.linalg.norm(positions - np.array(center, dtype=float), axis=1)
    return float(np.count_nonzero(distances <= radius) / (np.pi * radius * radius))


def _band_values(distribution: pd.DataFrame, band: str) -> np.ndarray:
    if distribution.empty:
        return np.array([], dtype=float)
    values = distribution.loc[
        distribution["density_band"] == band, "travel_time_s"
    ].to_numpy(dtype=float)
    return values[np.isfinite(values)]


def _ks_2samp(
    sample_a: Sequence[float], sample_b: Sequence[float]
) -> tuple[float, float]:
    a = np.sort(np.array(sample_a, dtype=float))
    b = np.sort(np.array(sample_b, dtype=float))
    a = a[np.isfinite(a)]
    b = b[np.isfinite(b)]
    if len(a) < 2 or len(b) < 2:
        return (float("nan"), float("nan"))
    points = np.sort(np.concatenate([a, b]))
    cdf_a = np.searchsorted(a, points, side="right") / len(a)
    cdf_b = np.searchsorted(b, points, side="right") / len(b)
    statistic = float(np.max(np.abs(cdf_a - cdf_b)))
    effective_n = sqrt(len(a) * len(b) / (len(a) + len(b)))
    scaled = (effective_n + 0.12 + 0.11 / max(effective_n, 1e-9)) * statistic
    return (statistic, _kolmogorov_pvalue(scaled))


def _kolmogorov_pvalue(value: float) -> float:
    if not np.isfinite(value) or value < 0.0:
        return float("nan")
    total = 0.0
    for index in range(1, 101):
        term = (
            2.0 * ((-1.0) ** (index - 1)) * np.exp(-2.0 * index * index * value * value)
        )
        total += term
        if abs(term) < 1e-12:
            break
    return float(np.clip(total, 0.0, 1.0))


def _rimea_case_id(path: Path) -> int:
    suffix = path.stem.rsplit("_", maxsplit=1)[-1]
    return int(suffix)
