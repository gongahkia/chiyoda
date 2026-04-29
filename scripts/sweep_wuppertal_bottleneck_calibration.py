#!/usr/bin/env python3
"""Run a small Chiyoda/Wuppertal bottleneck-flow calibration sweep."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import sys
from typing import Iterator

import pandas as pd
import yaml

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from chiyoda.analysis.external_validation import (  # noqa: E402
    compare_bottleneck_flow,
    load_petrack_trajectory,
    summarize_bottleneck_flow,
)
from chiyoda.analysis.metrics import SimulationAnalytics  # noqa: E402
from chiyoda.scenarios.manager import ScenarioManager  # noqa: E402
from chiyoda.studies.runner import _collect_run_tables  # noqa: E402
import chiyoda.navigation.social_force as social_force  # noqa: E402


DEFAULT_SCENARIO = ROOT / "scenarios/validation_wuppertal_bottleneck.yaml"
DEFAULT_REFERENCE = ROOT / "data/external/wuppertal_bottleneck_2018/040_c_56_h-.txt"


@dataclass(frozen=True)
class Candidate:
    candidate_id: str
    bottleneck_width: int
    exit_width: int
    base_speed: float
    density_slowdown_scale: float
    min_crowd_speed_factor: float
    social_force_label: str
    a_agent: float
    b_agent: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scenario",
        default=str(DEFAULT_SCENARIO),
        help="Base Chiyoda bottleneck scenario.",
    )
    parser.add_argument(
        "--reference",
        default=str(DEFAULT_REFERENCE),
        help="Wuppertal PeTrack reference trajectory.",
    )
    parser.add_argument(
        "-o",
        "--out",
        default="out/validation_wuppertal_bottleneck_calibration",
        help="Output directory for sweep tables.",
    )
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--max-steps", type=int, default=900)
    parser.add_argument("--frame-rate-hz", type=float, default=25.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    out_dir = Path(args.out)
    candidate_dir = out_dir / "candidates"
    candidate_dir.mkdir(parents=True, exist_ok=True)

    manager = ScenarioManager()
    base_scenario = manager.load_config(args.scenario)
    reference = load_petrack_trajectory(args.reference, frame_rate_hz=args.frame_rate_hz)
    reference_summary = summarize_bottleneck_flow(
        reference,
        source="wuppertal_2018",
        measurement_line=((0.35, 0.0), (-0.35, 0.0)),
    )

    analytics = SimulationAnalytics()
    rows: list[dict[str, float | int | str]] = []
    best: tuple[float, Candidate, pd.DataFrame, pd.DataFrame, dict] | None = None

    for candidate in calibration_candidates():
        scenario = configure_scenario(
            base_scenario,
            candidate=candidate,
            seed=args.seed,
            max_steps=args.max_steps,
        )
        simulated_line = measurement_line_for_width(candidate.bottleneck_width)
        with patched_social_force(candidate):
            simulation = manager.build_simulation(scenario)
            simulation.run()
        tables = _collect_run_tables(
            simulation=simulation,
            analytics=analytics,
            study_name="wuppertal_bottleneck_calibration",
            scenario_name=scenario.get("name", "validation_wuppertal_bottleneck"),
            variant_name=candidate.candidate_id,
            seed=args.seed,
            run_id=f"{candidate.candidate_id}__seed_{args.seed}",
        )

        simulated_summary = summarize_bottleneck_flow(
            tables["agent_steps"],
            source="chiyoda",
            measurement_line=simulated_line,
        )
        summaries = pd.concat(
            [reference_summary.to_frame(), simulated_summary.to_frame()],
            ignore_index=True,
        )
        comparison = compare_bottleneck_flow(simulated_summary, reference_summary)
        score = calibration_score(comparison)

        cdir = candidate_dir / candidate.candidate_id
        cdir.mkdir(parents=True, exist_ok=True)
        summaries.to_csv(cdir / "bottleneck_flow_summary.csv", index=False)
        comparison.to_csv(cdir / "bottleneck_flow_comparison.csv", index=False)
        (cdir / "candidate_parameters.json").write_text(
            json.dumps(asdict(candidate), indent=2) + "\n"
        )

        rows.append(result_row(candidate, comparison, score))
        if best is None or score < best[0]:
            best = (score, candidate, summaries, comparison, scenario)
        print(f"{candidate.candidate_id}: score={score:.3f}")

    results = pd.DataFrame(rows).sort_values("calibration_score")
    out_dir.mkdir(parents=True, exist_ok=True)
    results.to_csv(out_dir / "calibration_sweep_results.csv", index=False)

    if best is not None:
        _, best_candidate, best_summaries, best_comparison, best_scenario = best
        best_summaries.to_csv(out_dir / "best_bottleneck_flow_summary.csv", index=False)
        best_comparison.to_csv(out_dir / "best_bottleneck_flow_comparison.csv", index=False)
        (out_dir / "best_candidate_parameters.json").write_text(
            json.dumps(asdict(best_candidate), indent=2) + "\n"
        )
        with (out_dir / "best_candidate_scenario.yaml").open("w") as handle:
            yaml.safe_dump(best_scenario, handle, sort_keys=False)

    print(f"wrote {out_dir / 'calibration_sweep_results.csv'}")
    return 0


def calibration_candidates() -> list[Candidate]:
    social_force_options = [
        ("baseline_sfm", 2.1, 0.3),
        ("soft_sfm", 1.2, 0.45),
    ]
    candidates: list[Candidate] = []
    for bottleneck_width in (3, 5, 7):
        for base_speed in (1.34, 1.60):
            for density_slowdown_scale in (1.0, 0.6):
                for sf_label, a_agent, b_agent in social_force_options:
                    candidate_id = (
                        f"w{bottleneck_width}"
                        f"__v{speed_token(base_speed)}"
                        f"__dens{speed_token(density_slowdown_scale)}"
                        f"__{sf_label}"
                    )
                    candidates.append(
                        Candidate(
                            candidate_id=candidate_id,
                            bottleneck_width=bottleneck_width,
                            exit_width=bottleneck_width,
                            base_speed=base_speed,
                            density_slowdown_scale=density_slowdown_scale,
                            min_crowd_speed_factor=0.25,
                            social_force_label=sf_label,
                            a_agent=a_agent,
                            b_agent=b_agent,
                        )
                    )
    return candidates


def configure_scenario(
    base_scenario: dict,
    *,
    candidate: Candidate,
    seed: int,
    max_steps: int,
) -> dict:
    scenario = yaml.safe_load(yaml.safe_dump(base_scenario))
    scenario["layout"] = {"grid": bottleneck_grid(candidate.bottleneck_width, candidate.exit_width)}
    scenario.setdefault("population", {})
    for cohort in scenario["population"].get("cohorts", []):
        cohort["base_speed"] = candidate.base_speed
    scenario.setdefault("simulation", {})
    scenario["simulation"]["random_seed"] = seed
    scenario["simulation"]["max_steps"] = max_steps
    scenario["simulation"]["density_slowdown_scale"] = candidate.density_slowdown_scale
    scenario["simulation"]["min_crowd_speed_factor"] = candidate.min_crowd_speed_factor
    scenario.setdefault("metadata", {})
    scenario["metadata"]["calibration_candidate"] = asdict(candidate)
    scenario["metadata"]["validation_scope"] = "wuppertal_bottleneck_flow_calibration_sweep"
    return scenario


def bottleneck_grid(bottleneck_width: int, exit_width: int) -> list[str]:
    width = 11
    center = width // 2
    approach = ["X" + "." * (width - 2) + "X" for _ in range(5)]
    throat = opening_row(width, center, bottleneck_width, ".")
    exits = opening_row(width, center, exit_width, "E")
    return ["X" * width, *approach, throat, throat, throat, throat, exits]


def opening_row(width: int, center: int, opening_width: int, token: str) -> str:
    if opening_width < 1 or opening_width > width - 2 or opening_width % 2 == 0:
        raise ValueError("opening_width must be an odd value within the walkable interior")
    left = center - opening_width // 2
    right = center + opening_width // 2
    return "".join(token if left <= x <= right else "X" for x in range(width))


def measurement_line_for_width(bottleneck_width: int) -> tuple[tuple[float, float], tuple[float, float]]:
    center = 5.0
    half = bottleneck_width / 2.0
    return ((center - half, 6.0), (center + half, 6.0))


@contextmanager
def patched_social_force(candidate: Candidate) -> Iterator[None]:
    previous = {
        "A_AGENT": social_force.A_AGENT,
        "B_AGENT": social_force.B_AGENT,
    }
    social_force.A_AGENT = candidate.a_agent
    social_force.B_AGENT = candidate.b_agent
    try:
        yield
    finally:
        social_force.A_AGENT = previous["A_AGENT"]
        social_force.B_AGENT = previous["B_AGENT"]


def calibration_score(comparison: pd.DataFrame) -> float:
    values = comparison.set_index("metric")
    flow_error = abs(float(values.loc["mean_flow_ped_s", "pct_delta"]))
    headway_error = abs(float(values.loc["mean_time_headway_s", "pct_delta"]))
    crossing_error = abs(float(values.loc["crossing_count", "pct_delta"]))
    return flow_error + 0.5 * headway_error + 0.5 * crossing_error


def result_row(candidate: Candidate, comparison: pd.DataFrame, score: float) -> dict[str, float | int | str]:
    values = comparison.set_index("metric")
    row: dict[str, float | int | str] = asdict(candidate)
    row["simulated_crossing_count"] = float(values.loc["crossing_count", "simulated"])
    row["reference_crossing_count"] = float(values.loc["crossing_count", "reference"])
    row["crossing_count_pct_delta"] = float(values.loc["crossing_count", "pct_delta"])
    row["simulated_mean_flow_ped_s"] = float(values.loc["mean_flow_ped_s", "simulated"])
    row["reference_mean_flow_ped_s"] = float(values.loc["mean_flow_ped_s", "reference"])
    row["mean_flow_pct_delta"] = float(values.loc["mean_flow_ped_s", "pct_delta"])
    row["simulated_mean_time_headway_s"] = float(values.loc["mean_time_headway_s", "simulated"])
    row["reference_mean_time_headway_s"] = float(values.loc["mean_time_headway_s", "reference"])
    row["mean_time_headway_pct_delta"] = float(values.loc["mean_time_headway_s", "pct_delta"])
    row["calibration_score"] = score
    row["claim_status"] = claim_status(row)
    return row


def claim_status(row: dict[str, float | int | str]) -> str:
    flow_error = abs(float(row["mean_flow_pct_delta"]))
    headway_error = abs(float(row["mean_time_headway_pct_delta"]))
    crossing_error = abs(float(row["crossing_count_pct_delta"]))
    if flow_error <= 15.0 and headway_error <= 25.0 and crossing_error <= 10.0:
        return "calibrated_bottleneck_flow_match"
    return "diagnostic_gap"


def speed_token(value: float) -> str:
    return f"{value:.2f}".replace(".", "p")


if __name__ == "__main__":
    raise SystemExit(main())
