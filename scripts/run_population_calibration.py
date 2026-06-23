#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from chiyoda.information.route_choice_calibration import (
    PopulationCalibrationFit,
    fit_population_demand_profile,
    load_mta_hourly_ridership,
    write_population_calibration_fit,
)


DEFAULT_FEED = (
    "data/calibration/population_mta_2024/times_sq_2024_12_31_hourly.csv"
)


def calibrate(
    feed: str | Path = DEFAULT_FEED,
    *,
    station_complex: str | None = None,
    target_population: int = 240,
    release_interval_steps: int = 1,
) -> PopulationCalibrationFit:
    observations = load_mta_hourly_ridership(feed, station_complex=station_complex)
    return fit_population_demand_profile(
        observations,
        target_population=target_population,
        release_interval_steps=release_interval_steps,
    )


def run(
    feed: str | Path = DEFAULT_FEED,
    *,
    station_complex: str | None = None,
    target_population: int = 240,
    release_interval_steps: int = 1,
) -> dict:
    fit = calibrate(
        feed,
        station_complex=station_complex,
        target_population=target_population,
        release_interval_steps=release_interval_steps,
    )
    return fit.to_dict()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Calibrate a Chiyoda population profile from station ridership."
    )
    parser.add_argument("--feed", default=DEFAULT_FEED)
    parser.add_argument("--station-complex", default=None)
    parser.add_argument("--target-population", type=int, default=240)
    parser.add_argument("--release-interval-steps", type=int, default=1)
    parser.add_argument(
        "-o",
        "--output",
        default="out/population_calibration/times_sq_population_calibration.json",
    )
    args = parser.parse_args()

    fit = calibrate(
        args.feed,
        station_complex=args.station_complex,
        target_population=args.target_population,
        release_interval_steps=args.release_interval_steps,
    )
    output = Path(args.output)
    write_population_calibration_fit(fit, output)
    print(json.dumps(fit.to_dict(), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
