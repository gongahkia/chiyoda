"""
Fundamental diagram validation against Weidmann (1993).

Provides the theoretical Weidmann speed-density curve and tools to fit
empirical simulation data, compute goodness-of-fit metrics, and validate
that the SFM substrate reproduces physically grounded pedestrian dynamics.

References:
    Weidmann, U. "Transporttechnik der Fußgänger." Schriftenreihe des IVT
    Nr. 90, ETH Zürich, 1993.

Canonical parameters:
    v₀     = 1.34 m/s   (free-flow speed)
    ρ_max  = 5.4  ped/m² (jam density)
    γ      = 1.913       (shape parameter)
"""

from __future__ import annotations

import json
from collections.abc import Sequence
from dataclasses import dataclass
from math import sqrt
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

from chiyoda.scenarios.manager import ScenarioManager

# canonical Weidmann parameters
V0_WEIDMANN = 1.34  # free-flow speed (m/s)
RHO_MAX_WEIDMANN = 5.4  # jam density (ped/m²)
GAMMA_WEIDMANN = 1.913  # shape parameter

JUELICH_REFERENCE = Path(
    "data/external/juelich_bottleneck_flow/specific_flow_width.csv"
)
JUELICH_SCENARIOS = tuple(
    Path(f"scenarios/validation_bottleneck_juelich_{width}.yaml")
    for width in ("080", "100", "120", "140", "160")
)


def weidmann_speed(
    density: np.ndarray,
    v0: float = V0_WEIDMANN,
    rho_max: float = RHO_MAX_WEIDMANN,
    gamma: float = GAMMA_WEIDMANN,
) -> np.ndarray:
    """
    Weidmann speed-density relationship.

    v(ρ) = v₀ · (1 - exp(-γ · (1/ρ - 1/ρ_max)))

    Returns 0 for densities >= rho_max, v0 for density -> 0.
    """
    rho = np.asarray(density, dtype=float)
    result = np.zeros_like(rho)
    valid = (rho > 0.01) & (rho < rho_max)
    inv_rho = 1.0 / rho[valid] - 1.0 / rho_max
    result[valid] = v0 * (1.0 - np.exp(-gamma * inv_rho))
    result[rho <= 0.01] = v0  # free flow at near-zero density
    return result


@dataclass
class FDFitResult:
    """Result of fitting empirical data to Weidmann form."""

    v0_fit: float
    gamma_fit: float
    rho_max_fit: float
    r_squared: float
    rmse: float
    n_points: int
    densities: np.ndarray
    speeds: np.ndarray
    fitted_speeds: np.ndarray


@dataclass
class FDValidationResult:
    """Result of validating simulation data against Weidmann curve."""

    passed: bool
    rmse: float
    max_deviation: float
    r_squared: float
    n_points: int
    fit: FDFitResult
    rmse_threshold: float
    message: str


def fit_fundamental_diagram(
    densities: np.ndarray,
    speeds: np.ndarray,
    initial_v0: float = V0_WEIDMANN,
    initial_gamma: float = GAMMA_WEIDMANN,
    initial_rho_max: float = RHO_MAX_WEIDMANN,
) -> FDFitResult:
    """
    Fit empirical (density, speed) data to Weidmann functional form.

    Uses scipy least_squares with bounds to prevent degenerate fits.
    Falls back to canonical parameters if fitting fails.
    """
    rho = np.asarray(densities, dtype=float)
    v = np.asarray(speeds, dtype=float)
    mask = (rho > 0.01) & (v > 0.01) & np.isfinite(rho) & np.isfinite(v)
    rho = rho[mask]
    v = v[mask]

    if len(rho) < 5:
        fitted = weidmann_speed(rho)
        return FDFitResult(
            v0_fit=initial_v0,
            gamma_fit=initial_gamma,
            rho_max_fit=initial_rho_max,
            r_squared=0.0,
            rmse=float(np.sqrt(np.mean((v - fitted) ** 2))) if len(v) > 0 else 0.0,
            n_points=len(rho),
            densities=rho,
            speeds=v,
            fitted_speeds=fitted,
        )

    try:
        from scipy.optimize import least_squares

        def residuals(params):
            v0, gamma, rho_max = params
            predicted = weidmann_speed(rho, v0, rho_max, gamma)
            return v - predicted

        result = least_squares(
            residuals,
            x0=[initial_v0, initial_gamma, initial_rho_max],
            bounds=([0.5, 0.1, 2.0], [3.0, 10.0, 10.0]),
            method="trf",
        )
        v0_fit, gamma_fit, rho_max_fit = result.x
    except (ImportError, Exception):
        v0_fit, gamma_fit, rho_max_fit = initial_v0, initial_gamma, initial_rho_max

    fitted = weidmann_speed(rho, v0_fit, rho_max_fit, gamma_fit)
    ss_res = float(np.sum((v - fitted) ** 2))
    ss_tot = float(np.sum((v - np.mean(v)) ** 2))
    r_sq = 1.0 - (ss_res / ss_tot) if ss_tot > 1e-10 else 0.0
    rmse = float(np.sqrt(np.mean((v - fitted) ** 2)))

    return FDFitResult(
        v0_fit=v0_fit,
        gamma_fit=gamma_fit,
        rho_max_fit=rho_max_fit,
        r_squared=r_sq,
        rmse=rmse,
        n_points=len(rho),
        densities=rho,
        speeds=v,
        fitted_speeds=fitted,
    )


def validate_against_weidmann(
    densities: np.ndarray,
    speeds: np.ndarray,
    rmse_threshold: float = 0.20,
) -> FDValidationResult:
    """
    Validate empirical speed-density data against Weidmann curve.

    Args:
        densities: measured densities (ped/m²)
        speeds: measured speeds (m/s)
        rmse_threshold: maximum acceptable RMSE (m/s). Default 0.20 m/s.

    Returns:
        FDValidationResult with pass/fail, metrics, and fitted parameters.
    """
    fit = fit_fundamental_diagram(densities, speeds)
    theoretical = weidmann_speed(fit.densities)
    deviations = np.abs(fit.speeds - theoretical)
    max_dev = float(np.max(deviations)) if len(deviations) > 0 else 0.0

    # RMSE against canonical Weidmann (not the fit)
    rmse_vs_canonical = (
        float(np.sqrt(np.mean((fit.speeds - theoretical) ** 2)))
        if len(fit.speeds) > 0
        else 0.0
    )

    passed = rmse_vs_canonical < rmse_threshold
    msg = (
        f"PASS: RMSE={rmse_vs_canonical:.3f} m/s < {rmse_threshold:.3f} m/s threshold"
        if passed
        else f"FAIL: RMSE={rmse_vs_canonical:.3f} m/s >= {rmse_threshold:.3f} m/s threshold"
    )

    return FDValidationResult(
        passed=passed,
        rmse=rmse_vs_canonical,
        max_deviation=max_dev,
        r_squared=fit.r_squared,
        n_points=fit.n_points,
        fit=fit,
        rmse_threshold=rmse_threshold,
        message=msg,
    )


def load_specific_flow_reference(path: str | Path = JUELICH_REFERENCE) -> pd.DataFrame:
    frame = pd.read_csv(path)
    required = {"width_m", "specific_flow_ped_m_s"}
    missing = required - set(frame.columns)
    if missing:
        raise ValueError(f"specific-flow reference missing columns: {sorted(missing)}")
    frame = frame.copy()
    frame["width_m"] = pd.to_numeric(frame["width_m"], errors="raise")
    frame["specific_flow_ped_m_s"] = pd.to_numeric(
        frame["specific_flow_ped_m_s"], errors="raise"
    )
    return frame.sort_values("width_m").reset_index(drop=True)


def run_bottleneck_width_curve(
    scenario_files: Sequence[str | Path] = JUELICH_SCENARIOS,
) -> pd.DataFrame:
    rows = []
    manager = ScenarioManager()
    for scenario_file in scenario_files:
        path = Path(scenario_file)
        scenario = manager.load_config(str(path))
        simulation = manager.build_simulation(scenario)
        simulation.run()
        rows.append(_scenario_specific_flow(path, scenario, simulation))
    return pd.DataFrame(rows).sort_values("width_m").reset_index(drop=True)


def compare_specific_flow_curve(
    simulated: pd.DataFrame,
    reference: pd.DataFrame,
) -> pd.DataFrame:
    sim = simulated.rename(
        columns={"specific_flow_ped_m_s": "simulated_specific_flow_ped_m_s"}
    )
    ref = reference.rename(
        columns={"specific_flow_ped_m_s": "reference_specific_flow_ped_m_s"}
    )
    comparison = ref.merge(
        sim[
            [
                "width_m",
                "scenario",
                "crossing_count",
                "flow_ped_s",
                "simulated_specific_flow_ped_m_s",
            ]
        ],
        on="width_m",
        how="inner",
    )
    comparison["delta_specific_flow_ped_m_s"] = (
        comparison["simulated_specific_flow_ped_m_s"]
        - comparison["reference_specific_flow_ped_m_s"]
    )
    comparison["squared_error"] = comparison["delta_specific_flow_ped_m_s"] ** 2
    return comparison.sort_values("width_m").reset_index(drop=True)


def specific_flow_rmse(comparison: pd.DataFrame) -> float:
    if comparison.empty:
        return float("nan")
    return float(sqrt(float(comparison["squared_error"].mean())))


def write_specific_flow_report(
    output_dir: str | Path,
    *,
    scenario_files: Sequence[str | Path] = JUELICH_SCENARIOS,
    reference_path: str | Path = JUELICH_REFERENCE,
    rmse_threshold: float = 0.25,
) -> dict[str, Any]:
    output = Path(output_dir)
    output.mkdir(parents=True, exist_ok=True)
    reference = load_specific_flow_reference(reference_path)
    simulated = run_bottleneck_width_curve(scenario_files)
    comparison = compare_specific_flow_curve(simulated, reference)
    rmse = specific_flow_rmse(comparison)
    ok = bool(rmse <= float(rmse_threshold))

    simulated.to_csv(output / "juelich_specific_flow_simulated.csv", index=False)
    comparison.to_csv(output / "juelich_specific_flow_comparison.csv", index=False)
    figure_path = output / "juelich_specific_flow_curve.png"
    plot_specific_flow_curve(comparison, figure_path)

    summary: dict[str, Any] = {
        "rmse_specific_flow_ped_m_s": rmse,
        "rmse_threshold_ped_m_s": float(rmse_threshold),
        "ok": ok,
        "width_count": int(len(comparison)),
        "figure": str(figure_path),
    }
    (output / "juelich_specific_flow_summary.json").write_text(
        json.dumps(summary, indent=2) + "\n"
    )
    return summary


def plot_specific_flow_curve(
    comparison: pd.DataFrame,
    output_path: str | Path,
) -> Path:
    import matplotlib.pyplot as plt

    output = Path(output_path)
    output.parent.mkdir(parents=True, exist_ok=True)
    fig, ax = plt.subplots(figsize=(6, 4))
    ax.plot(
        comparison["width_m"],
        comparison["reference_specific_flow_ped_m_s"],
        marker="o",
        label="Juelich reference",
    )
    ax.plot(
        comparison["width_m"],
        comparison["simulated_specific_flow_ped_m_s"],
        marker="s",
        label="Chiyoda proxy",
    )
    ax.set_xlabel("Bottleneck width (m)")
    ax.set_ylabel("Specific flow (ped/(m*s))")
    ax.grid(True, alpha=0.25)
    ax.legend()
    fig.tight_layout()
    fig.savefig(output, dpi=160)
    plt.close(fig)
    return output


def _scenario_specific_flow(
    scenario_file: Path,
    scenario: dict[str, Any],
    simulation,
) -> dict[str, Any]:
    metadata = scenario.get("metadata", {}) or {}
    width_m = float(metadata["bottleneck_width_m"])
    connector_ids = set(
        str(value)
        for value in metadata.get(
            "bottleneck_connector_ids",
            list(getattr(simulation, "connector_usage_cumulative", {}).keys()),
        )
    )
    crossing_times = sorted(
        float(event["time_s"])
        for event in getattr(simulation, "connector_events", [])
        if event.get("phase") == "finish"
        and str(event.get("connector_id")) in connector_ids
    )
    if len(crossing_times) < 2:
        raise ValueError(f"{scenario_file} recorded fewer than two crossings")
    duration_s = crossing_times[-1] - crossing_times[0]
    if duration_s <= 0:
        raise ValueError(f"{scenario_file} recorded non-positive crossing duration")
    flow_ped_s = len(crossing_times) / duration_s
    return {
        "scenario": str(scenario.get("name", scenario_file.stem)),
        "scenario_file": str(scenario_file),
        "width_m": width_m,
        "connector_ids": ",".join(sorted(connector_ids)),
        "crossing_count": int(len(crossing_times)),
        "first_crossing_s": float(crossing_times[0]),
        "last_crossing_s": float(crossing_times[-1]),
        "duration_s": float(duration_s),
        "flow_ped_s": float(flow_ped_s),
        "specific_flow_ped_m_s": float(flow_ped_s / width_m),
    }
