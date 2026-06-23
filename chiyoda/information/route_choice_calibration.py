from __future__ import annotations

import csv
import json
import math
import re
import zipfile
from collections.abc import Iterable
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

TASK_ID_RE = re.compile(
    r"^(?:\d+_)?L-([WNE])([SML])([01])_R-([WNE])([SML])([01])(?:_.+)?$"
)
FEATURE_NAMES = (
    "right_width_advantage_m",
    "right_shortness_advantage_m",
    "right_stairs_advantage",
    "previous_choice_right",
    "trial_progress",
)
WIDTH_M = {"W": 4.0, "N": 2.0, "E": 2.0}
LENGTH_M = {"S": 10.0, "M": 15.0, "L": 20.0}
COUNTDOWN_MS = 8000.0


@dataclass(frozen=True)
class RouteChoiceObservation:
    user_id: str
    task_count: int
    task_id: str
    choice_right: int
    left_width_m: float
    right_width_m: float
    left_length_m: float
    right_length_m: float
    left_stairs: int
    right_stairs: int
    right_width_advantage_m: float
    right_shortness_advantage_m: float
    right_stairs_advantage: int
    previous_choice_right: float
    trial_progress: float
    time_remaining_ms: float
    decision_time_ms: float
    confidence: float | None


@dataclass(frozen=True)
class RouteChoiceFit:
    source: dict[str, Any]
    records: dict[str, Any]
    feature_names: tuple[str, ...]
    coefficients: dict[str, float]
    standardization: dict[str, dict[str, float]]
    metrics: dict[str, float]
    priors: dict[str, float]
    method: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "source": self.source,
            "records": self.records,
            "feature_names": list(self.feature_names),
            "coefficients": self.coefficients,
            "standardization": self.standardization,
            "metrics": self.metrics,
            "priors": self.priors,
            "method": self.method,
        }


@dataclass(frozen=True)
class PopulationDemandObservation:
    timestamp: str
    station_complex: str
    ridership: float


@dataclass(frozen=True)
class PopulationCalibrationFit:
    source: dict[str, Any]
    records: dict[str, Any]
    scaling: dict[str, Any]
    metrics: dict[str, float]
    hourly_profile: list[dict[str, Any]]
    scenario_population: dict[str, Any]
    method: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "source": self.source,
            "records": self.records,
            "scaling": self.scaling,
            "metrics": self.metrics,
            "hourly_profile": self.hourly_profile,
            "scenario_population": self.scenario_population,
            "method": self.method,
        }


def load_figshare_route_choice_records(
    archive_path: str | Path,
) -> list[RouteChoiceObservation]:
    archive = Path(archive_path)
    with zipfile.ZipFile(archive) as zf:
        responses = pd.read_csv(zf.open("SciData_export/responses.csv"), dtype=str)
        confidence = pd.read_csv(zf.open("SciData_export/confidence.csv"), dtype=str)

    confidence_map = _confidence_map(confidence)
    responses["task_count_numeric"] = pd.to_numeric(
        responses["taskCount"], errors="coerce"
    )
    responses["time_remaining_ms"] = responses["time"].map(_parse_float)
    responses = responses[
        responses["AOI"].isin(("corridor_left", "corridor_right"))
        & responses["task_count_numeric"].notna()
        & responses["time_remaining_ms"].notna()
    ].copy()
    responses = responses[~responses["taskId"].map(_is_training_task)]
    responses = responses.sort_values(["userId", "task_count_numeric", "taskId"])
    max_task_count = max(float(responses["task_count_numeric"].max()), 1.0)

    previous_choice: dict[str, int] = {}
    observations: list[RouteChoiceObservation] = []
    for row in responses.itertuples(index=False):
        parsed = _parse_task_id(str(row.taskId))
        if parsed is None:
            continue
        user_id = str(row.userId)
        choice_right = 1 if str(row.AOI) == "corridor_right" else 0
        previous = previous_choice.get(user_id)
        previous_feature = 0.0 if previous is None else (1.0 if previous == 1 else -1.0)
        previous_choice[user_id] = choice_right
        task_count = int(float(row.task_count_numeric))
        time_remaining = float(row.time_remaining_ms)
        confidence_value = confidence_map.get((user_id, str(row.taskId), task_count))
        observations.append(
            RouteChoiceObservation(
                user_id=user_id,
                task_count=task_count,
                task_id=str(row.taskId),
                choice_right=choice_right,
                left_width_m=parsed["left_width_m"],
                right_width_m=parsed["right_width_m"],
                left_length_m=parsed["left_length_m"],
                right_length_m=parsed["right_length_m"],
                left_stairs=parsed["left_stairs"],
                right_stairs=parsed["right_stairs"],
                right_width_advantage_m=parsed["right_width_m"]
                - parsed["left_width_m"],
                right_shortness_advantage_m=parsed["left_length_m"]
                - parsed["right_length_m"],
                right_stairs_advantage=parsed["right_stairs"] - parsed["left_stairs"],
                previous_choice_right=previous_feature,
                trial_progress=min(1.0, max(0.0, task_count / max_task_count)),
                time_remaining_ms=time_remaining,
                decision_time_ms=max(0.0, COUNTDOWN_MS - time_remaining),
                confidence=confidence_value,
            )
        )
    return observations


def load_mta_hourly_ridership(
    path: str | Path,
    *,
    station_complex: str | None = None,
) -> list[PopulationDemandObservation]:
    source = Path(path)
    if source.suffix.lower() == ".json":
        frame = pd.read_json(source)
    else:
        frame = pd.read_csv(source)
    if "sum_ridership" in frame.columns and "ridership" not in frame.columns:
        frame = frame.rename(columns={"sum_ridership": "ridership"})
    required = {"transit_timestamp", "station_complex", "ridership"}
    missing = required.difference(frame.columns)
    if missing:
        raise ValueError(f"MTA hourly ridership feed missing columns: {sorted(missing)}")
    if station_complex is not None:
        frame = frame[frame["station_complex"] == station_complex].copy()
    frame["transit_timestamp"] = pd.to_datetime(
        frame["transit_timestamp"], errors="raise"
    )
    frame["ridership"] = pd.to_numeric(frame["ridership"], errors="raise")
    frame = frame[frame["ridership"].notna()].copy()
    if (frame["ridership"] < 0).any():
        raise ValueError("MTA hourly ridership feed contains negative ridership")
    grouped = (
        frame.groupby(["transit_timestamp", "station_complex"], as_index=False)[
            "ridership"
        ]
        .sum()
        .sort_values(["station_complex", "transit_timestamp"])
    )
    return [
        PopulationDemandObservation(
            timestamp=row.transit_timestamp.isoformat(),
            station_complex=str(row.station_complex),
            ridership=float(row.ridership),
        )
        for row in grouped.itertuples(index=False)
    ]


def fit_population_demand_profile(
    observations: Iterable[PopulationDemandObservation],
    *,
    target_population: int = 240,
    release_interval_steps: int = 1,
    source_name: str = "MTA Subway Hourly Ridership: 2020-2024",
) -> PopulationCalibrationFit:
    records = list(observations)
    if not records:
        raise ValueError("no population-demand observations")
    if target_population <= 0:
        raise ValueError("target_population must be positive")
    if release_interval_steps <= 0:
        raise ValueError("release_interval_steps must be positive")
    stations = sorted({record.station_complex for record in records})
    if len(stations) != 1:
        raise ValueError("population calibration requires exactly one station_complex")
    records = sorted(records, key=lambda record: record.timestamp)
    observed = np.array([record.ridership for record in records], dtype=float)
    observed_total = float(observed.sum())
    if observed_total <= 0.0:
        raise ValueError("population-demand observations have zero total ridership")
    scale = float(target_population / observed_total)
    raw_agents = observed * scale
    calibrated_agents = _round_counts_to_total(raw_agents, int(target_population))
    predicted_ridership = calibrated_agents.astype(float) / scale
    residuals = observed - predicted_ridership
    hourly_profile = []
    cohorts = []
    for index, record in enumerate(records):
        release_step = int(index * release_interval_steps)
        profile_row = {
            "timestamp": record.timestamp,
            "station_complex": record.station_complex,
            "observed_ridership": float(record.ridership),
            "raw_scaled_agents": float(raw_agents[index]),
            "calibrated_agents": int(calibrated_agents[index]),
            "predicted_ridership": float(predicted_ridership[index]),
            "residual_ridership": float(residuals[index]),
            "release_step": release_step,
        }
        hourly_profile.append(profile_row)
        cohorts.append(
            {
                "name": f"mta_hour_{index:02d}",
                "count": int(calibrated_agents[index]),
                "release_step": release_step,
                "source_timestamp": record.timestamp,
                "source_ridership": float(record.ridership),
            }
        )
    return PopulationCalibrationFit(
        source={
            "dataset": source_name,
            "api_id": "wujg-7c2s",
            "url": "https://data.ny.gov/Transportation/MTA-Subway-Hourly-Ridership-2020-2024/wujg-7c2s",
            "station_complex": stations[0],
            "start_timestamp": records[0].timestamp,
            "end_timestamp": records[-1].timestamp,
        },
        records={
            "total": len(records),
            "station_complexes": len(stations),
        },
        scaling={
            "target_population": int(target_population),
            "observed_total_ridership": observed_total,
            "agents_per_observed_rider": scale,
            "release_interval_steps": int(release_interval_steps),
        },
        metrics={
            "mean_absolute_residual_ridership": float(np.mean(np.abs(residuals))),
            "rmse_residual_ridership": float(
                np.sqrt(np.mean(residuals * residuals))
            ),
            "max_abs_residual_ridership": float(np.max(np.abs(residuals))),
            "total_residual_ridership": float(residuals.sum()),
        },
        hourly_profile=hourly_profile,
        scenario_population={
            "total": int(target_population),
            "cohorts": cohorts,
        },
        method={
            "model": "hourly ridership proportional integer cohort scaling",
            "residual_definition": "observed_ridership - calibrated_agents / agents_per_observed_rider",
            "rounding": "largest remainder with exact target_population sum",
        },
    )


def fit_route_choice_priors(
    observations: Iterable[RouteChoiceObservation],
    *,
    holdout_stride: int = 5,
    l2: float = 1e-3,
    learning_rate: float = 0.1,
    max_iterations: int = 10_000,
    tolerance: float = 1e-9,
) -> RouteChoiceFit:
    records = list(observations)
    if not records:
        raise ValueError("no route-choice observations")
    users = sorted({record.user_id for record in records})
    if len(users) < 2:
        raise ValueError("calibration requires at least two participants")
    if holdout_stride < 2:
        raise ValueError("holdout_stride must be >= 2")
    holdout_users = set(users[::holdout_stride])
    train = [record for record in records if record.user_id not in holdout_users]
    test = [record for record in records if record.user_id in holdout_users]
    if not train or not test:
        raise ValueError("participant split produced empty train or test set")

    x_train_raw, y_train = _feature_matrix(train)
    x_test_raw, y_test = _feature_matrix(test)
    means = x_train_raw.mean(axis=0)
    scales = x_train_raw.std(axis=0)
    scales[scales < 1e-9] = 1.0
    x_train = _with_intercept((x_train_raw - means) / scales)
    x_test = _with_intercept((x_test_raw - means) / scales)
    weights, iterations = _fit_logistic(
        x_train,
        y_train,
        l2=l2,
        learning_rate=learning_rate,
        max_iterations=max_iterations,
        tolerance=tolerance,
    )
    train_probability = float(np.clip(y_train.mean(), 1e-9, 1.0 - 1e-9))
    test_probability = _predict_probability(x_test, weights)
    baseline_probability = np.full_like(y_test, train_probability, dtype=float)
    train_probability_model = _predict_probability(x_train, weights)
    coefficients = {"intercept": float(weights[0])}
    coefficients.update(
        {
            name: float(value)
            for name, value in zip(FEATURE_NAMES, weights[1:], strict=False)
        }
    )
    fit = RouteChoiceFit(
        source={
            "dataset": "Predictors of evacuation behavior: dataset on respondents' route choice and web interaction",
            "article_doi": "10.1038/s41597-025-04440-y",
            "figshare_doi": "10.6084/m9.figshare.27705402.v1",
            "archive": "Snopkova_Isovists.zip",
        },
        records={
            "total": len(records),
            "train": len(train),
            "test": len(test),
            "participants": len(users),
            "holdout_participants": len(holdout_users),
        },
        feature_names=FEATURE_NAMES,
        coefficients=coefficients,
        standardization={
            "mean": {
                name: float(value)
                for name, value in zip(FEATURE_NAMES, means, strict=False)
            },
            "scale": {
                name: float(value)
                for name, value in zip(FEATURE_NAMES, scales, strict=False)
            },
        },
        metrics={
            "train_log_loss": _log_loss(y_train, train_probability_model),
            "test_log_loss": _log_loss(y_test, test_probability),
            "test_baseline_log_loss": _log_loss(y_test, baseline_probability),
            "test_log_loss_improvement": _log_loss(y_test, baseline_probability)
            - _log_loss(y_test, test_probability),
            "test_accuracy": _accuracy(y_test, test_probability),
            "test_baseline_accuracy": _accuracy(y_test, baseline_probability),
        },
        priors=_derive_priors(records, weights),
        method={
            "model": "participant-held-out l2 logistic regression",
            "holdout_stride": holdout_stride,
            "l2": l2,
            "learning_rate": learning_rate,
            "iterations": iterations,
            "excluded": "training tasks, background clicks, missing AOI, missing timer rows",
        },
    )
    return fit


def write_normalized_records(
    observations: Iterable[RouteChoiceObservation], output_path: str | Path
) -> None:
    records = [asdict(record) for record in observations]
    output = Path(output_path)
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(records[0].keys()))
        writer.writeheader()
        writer.writerows(records)


def write_route_choice_fit(fit: RouteChoiceFit, output_path: str | Path) -> None:
    output = Path(output_path)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(fit.to_dict(), indent=2, sort_keys=True) + "\n")


def write_population_calibration_fit(
    fit: PopulationCalibrationFit, output_path: str | Path
) -> None:
    output = Path(output_path)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(fit.to_dict(), indent=2, sort_keys=True) + "\n")


def _round_counts_to_total(values: np.ndarray, total: int) -> np.ndarray:
    floors = np.floor(values).astype(int)
    remainder = int(total - floors.sum())
    if remainder < 0:
        raise ValueError("rounded population exceeds target")
    if remainder:
        order = np.argsort(-(values - floors))
        floors[order[:remainder]] += 1
    return floors


def _confidence_map(frame: pd.DataFrame) -> dict[tuple[str, str, int], float]:
    frame = frame.copy()
    frame["task_count_numeric"] = pd.to_numeric(frame["taskCount"], errors="coerce")
    frame["confidence_value"] = pd.to_numeric(frame["confidence"], errors="coerce")
    frame = frame[
        frame["task_count_numeric"].notna() & frame["confidence_value"].notna()
    ]
    result: dict[tuple[str, str, int], float] = {}
    for row in frame.itertuples(index=False):
        result[
            (str(row.userId), str(row.taskId), int(float(row.task_count_numeric)))
        ] = float(row.confidence_value)
    return result


def _parse_float(value: Any) -> float | None:
    if value is None:
        return None
    text = str(value).strip().strip('"').replace(",", ".")
    if not text or text.lower() in {"null", "nan", "na"}:
        return None
    try:
        return float(text)
    except ValueError:
        return None


def _is_training_task(task_id: Any) -> bool:
    text = str(task_id)
    return text.endswith("_tr") or text.endswith("_in_tr") or "_tr_" in text


def _parse_task_id(task_id: str) -> dict[str, Any] | None:
    match = TASK_ID_RE.match(task_id)
    if match is None:
        return None
    left_width, left_length, left_stairs, right_width, right_length, right_stairs = (
        match.groups()
    )
    return {
        "left_width_m": WIDTH_M[left_width],
        "right_width_m": WIDTH_M[right_width],
        "left_length_m": LENGTH_M[left_length],
        "right_length_m": LENGTH_M[right_length],
        "left_stairs": int(left_stairs),
        "right_stairs": int(right_stairs),
    }


def _feature_matrix(
    records: list[RouteChoiceObservation],
) -> tuple[np.ndarray, np.ndarray]:
    x = np.array(
        [
            [
                record.right_width_advantage_m,
                record.right_shortness_advantage_m,
                float(record.right_stairs_advantage),
                record.previous_choice_right,
                record.trial_progress,
            ]
            for record in records
        ],
        dtype=float,
    )
    y = np.array([record.choice_right for record in records], dtype=float)
    return x, y


def _with_intercept(x: np.ndarray) -> np.ndarray:
    return np.column_stack([np.ones(x.shape[0], dtype=float), x])


def _fit_logistic(
    x: np.ndarray,
    y: np.ndarray,
    *,
    l2: float,
    learning_rate: float,
    max_iterations: int,
    tolerance: float,
) -> tuple[np.ndarray, int]:
    weights = np.zeros(x.shape[1], dtype=float)
    penalty_mask = np.ones_like(weights)
    penalty_mask[0] = 0.0
    for iteration in range(1, max_iterations + 1):
        probabilities = _predict_probability(x, weights)
        gradient = x.T @ (probabilities - y) / len(y) + l2 * penalty_mask * weights
        step = learning_rate * gradient
        weights -= step
        if float(np.linalg.norm(step)) < tolerance:
            return weights, iteration
    return weights, max_iterations


def _predict_probability(x: np.ndarray, weights: np.ndarray) -> np.ndarray:
    logits = np.clip(x @ weights, -50.0, 50.0)
    return 1.0 / (1.0 + np.exp(-logits))


def _log_loss(y: np.ndarray, probabilities: np.ndarray) -> float:
    clipped = np.clip(probabilities, 1e-9, 1.0 - 1e-9)
    return float(-np.mean(y * np.log(clipped) + (1.0 - y) * np.log(1.0 - clipped)))


def _accuracy(y: np.ndarray, probabilities: np.ndarray) -> float:
    return float(np.mean((probabilities >= 0.5) == y))


def _derive_priors(
    records: list[RouteChoiceObservation], weights: np.ndarray
) -> dict[str, float]:
    confidence_values = [
        record.confidence for record in records if record.confidence is not None
    ]
    confidence_prior = (
        (float(np.mean(confidence_values)) / 5.0) if confidence_values else 0.5
    )
    decision_times = np.array(
        [record.decision_time_ms for record in records], dtype=float
    )
    speed_prior = 1.0 - float(np.median(decision_times) / COUNTDOWN_MS)
    familiarity = _clamp(0.65 * confidence_prior + 0.35 * speed_prior)
    geometry_strength = float(np.linalg.norm(weights[1:4]) / math.sqrt(3.0))
    return {
        "familiarity": familiarity,
        "herding": _sigmoid(float(weights[4])),
        "exit_affinity": _sigmoid(geometry_strength),
    }


def _sigmoid(value: float) -> float:
    return float(1.0 / (1.0 + math.exp(-value)))


def _clamp(value: float) -> float:
    return float(max(0.0, min(1.0, value)))
