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
from dataclasses import dataclass
from typing import Optional, Tuple
import numpy as np


# canonical Weidmann parameters
V0_WEIDMANN = 1.34       # free-flow speed (m/s)
RHO_MAX_WEIDMANN = 5.4   # jam density (ped/m²)
GAMMA_WEIDMANN = 1.913   # shape parameter


def weidmann_speed(density: np.ndarray, v0: float = V0_WEIDMANN,
                   rho_max: float = RHO_MAX_WEIDMANN,
                   gamma: float = GAMMA_WEIDMANN) -> np.ndarray:
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
    result[rho <= 0.01] = v0 # free flow at near-zero density
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
            v0_fit=initial_v0, gamma_fit=initial_gamma,
            rho_max_fit=initial_rho_max,
            r_squared=0.0, rmse=float(np.sqrt(np.mean((v - fitted) ** 2))) if len(v) > 0 else 0.0,
            n_points=len(rho),
            densities=rho, speeds=v,
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
            method='trf',
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
        v0_fit=v0_fit, gamma_fit=gamma_fit, rho_max_fit=rho_max_fit,
        r_squared=r_sq, rmse=rmse, n_points=len(rho),
        densities=rho, speeds=v, fitted_speeds=fitted,
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
    rmse_vs_canonical = float(np.sqrt(np.mean((fit.speeds - theoretical) ** 2))) if len(fit.speeds) > 0 else 0.0

    passed = rmse_vs_canonical < rmse_threshold
    msg = (
        f"PASS: RMSE={rmse_vs_canonical:.3f} m/s < {rmse_threshold:.3f} m/s threshold"
        if passed
        else f"FAIL: RMSE={rmse_vs_canonical:.3f} m/s >= {rmse_threshold:.3f} m/s threshold"
    )

    return FDValidationResult(
        passed=passed, rmse=rmse_vs_canonical,
        max_deviation=max_dev, r_squared=fit.r_squared,
        n_points=fit.n_points, fit=fit,
        rmse_threshold=rmse_threshold, message=msg,
    )
