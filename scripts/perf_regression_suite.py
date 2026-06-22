"""Opt-in perf regression harness.

Runs a fixed set of benchmark scenarios under timing+memory capture and
emits a CSV under ``out/perf/`` that CI can diff for regressions.

The scenarios are not exhaustive; they are a smoke set:

- ``scenarios/benchmark/transit_cbrn.yaml`` — small CBRN baseline.
- ``scenarios/benchmark/transit_cbrn_10k.yaml`` — large-population variant.
- ``scenarios/benchmark/wildfire_wui.yaml`` — wildfire with ember spotting.

If a scenario file is absent (the 10k file is optional) the harness skips
that row rather than failing.
"""

from __future__ import annotations

import argparse
import csv
import gc
import os
import resource
import time
from datetime import UTC, datetime
from pathlib import Path

from chiyoda.scenarios.manager import ScenarioManager

DEFAULT_SCENARIOS = [
    "scenarios/benchmark/transit_cbrn.yaml",
    "scenarios/benchmark/transit_cbrn_10k.yaml",
    "scenarios/benchmark/wildfire_wui.yaml",
]


def _peak_rss_bytes() -> int:
    usage = resource.getrusage(resource.RUSAGE_SELF)
    # ru_maxrss is KB on Linux, bytes on macOS
    raw = int(usage.ru_maxrss)
    if hasattr(os, "uname") and os.uname().sysname == "Darwin":
        return raw
    return raw * 1024


def _run_one(scenario_path: str, seed: int) -> dict[str, float | str | int]:
    gc.collect()
    rss_before = _peak_rss_bytes()
    manager = ScenarioManager()
    config = manager.load_config(scenario_path)
    config.setdefault("simulation", {})
    config["simulation"]["random_seed"] = seed
    start = time.perf_counter()
    sim = manager.build_simulation(config)
    sim.run()
    elapsed = time.perf_counter() - start
    rss_after = _peak_rss_bytes()
    return {
        "scenario": scenario_path,
        "seed": seed,
        "elapsed_s": round(elapsed, 4),
        "rss_delta_mib": round((rss_after - rss_before) / (1024 * 1024), 3),
        "evacuated": int(
            sum(1 for a in sim.agents if getattr(a, "has_evacuated", False))
        ),
        "step_count": int(getattr(sim, "current_step", 0)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--scenario",
        action="append",
        default=None,
        help="Scenario path. Repeatable. Defaults to the smoke set if omitted.",
    )
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("out/perf"),
        help="Output directory; receives perf_<utc>.csv",
    )
    args = parser.parse_args()
    scenarios = args.scenario or DEFAULT_SCENARIOS
    args.out.mkdir(parents=True, exist_ok=True)
    rows = []
    for scenario in scenarios:
        if not Path(scenario).exists():
            rows.append(
                {
                    "scenario": scenario,
                    "seed": args.seed,
                    "elapsed_s": -1.0,
                    "rss_delta_mib": 0.0,
                    "evacuated": 0,
                    "step_count": 0,
                    "skipped": "missing",
                }
            )
            continue
        rows.append(_run_one(scenario, args.seed))

    timestamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
    csv_path = args.out / f"perf_{timestamp}.csv"
    with csv_path.open("w", newline="") as handle:
        fieldnames = sorted({key for row in rows for key in row.keys()})
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow(row)
    print(f"wrote {csv_path}")


if __name__ == "__main__":
    main()
