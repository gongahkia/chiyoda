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
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from chiyoda._logging import get_logger
from chiyoda.scenarios.manager import ScenarioManager

DEFAULT_SCENARIOS = [
    "scenarios/benchmark/transit_cbrn.yaml",
    "scenarios/benchmark/transit_cbrn_10k.yaml",
    "scenarios/benchmark/wildfire_wui.yaml",
]
DEFAULT_MAX_REGRESSION = 0.10


def _peak_rss_bytes() -> int:
    usage = resource.getrusage(resource.RUSAGE_SELF)
    # ru_maxrss is KB on Linux, bytes on macOS
    raw = int(usage.ru_maxrss)
    if hasattr(os, "uname") and os.uname().sysname == "Darwin":
        return raw
    return raw * 1024


def _run_one(
    scenario_path: str, seed: int, *, label: str = ""
) -> dict[str, float | str | int]:
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
        "label": label,
        "scenario": scenario_path,
        "seed": seed,
        "elapsed_s": round(elapsed, 4),
        "rss_delta_mib": round((rss_after - rss_before) / (1024 * 1024), 3),
        "evacuated": int(
            sum(1 for a in sim.agents if getattr(a, "has_evacuated", False))
        ),
        "step_count": int(getattr(sim, "current_step", 0)),
    }


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv[:1] == ["compare"]:
        return _compare_main(argv[1:])
    return _run_main(argv)


def _run_main(argv: list[str]) -> int:
    get_logger().setLevel("WARNING")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--scenario",
        action="append",
        default=None,
        help="Scenario path. Repeatable. Defaults to the smoke set if omitted.",
    )
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--label", default="")
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("out/perf"),
        help="Output directory; receives perf_<utc>.csv",
    )
    parser.add_argument("--output-file", type=Path, default=None)
    args = parser.parse_args(argv)
    scenarios = args.scenario or DEFAULT_SCENARIOS
    args.out.mkdir(parents=True, exist_ok=True)
    rows = []
    for scenario in scenarios:
        if not Path(scenario).exists():
            rows.append(
                {
                    "scenario": scenario,
                    "seed": args.seed,
                    "label": args.label,
                    "elapsed_s": -1.0,
                    "rss_delta_mib": 0.0,
                    "evacuated": 0,
                    "step_count": 0,
                    "skipped": "missing",
                }
            )
            continue
        rows.append(_run_one(scenario, args.seed, label=args.label))

    timestamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")
    csv_path = args.output_file or args.out / f"perf_{timestamp}.csv"
    write_rows(csv_path, rows)
    print(f"wrote {csv_path}")
    return 0


def _compare_main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Compare perf CSV files.")
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument(
        "--max-regression",
        type=float,
        default=DEFAULT_MAX_REGRESSION,
        help="Allowed elapsed_s regression fraction, e.g. 0.10 for 10%%.",
    )
    parser.add_argument("--out", type=Path, default=Path("out/perf/perf_delta.csv"))
    parser.add_argument(
        "--summary", type=Path, default=Path("out/perf/perf_delta.md")
    )
    args = parser.parse_args(argv)
    rows = compare_perf(args.baseline, args.current, args.max_regression)
    write_rows(args.out, rows)
    markdown = comparison_markdown(rows, args.max_regression)
    args.summary.parent.mkdir(parents=True, exist_ok=True)
    args.summary.write_text(markdown)
    step_summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if step_summary:
        with Path(step_summary).open("a") as handle:
            handle.write(markdown)
    print(markdown)
    return 1 if any(row["status"] == "regression" for row in rows) else 0


def compare_perf(
    baseline_path: Path, current_path: Path, max_regression: float
) -> list[dict[str, float | str | int]]:
    baseline = _rows_by_key(read_rows(baseline_path))
    current = read_rows(current_path)
    rows: list[dict[str, float | str | int]] = []
    for row in current:
        key = (row["scenario"], row["seed"])
        base = baseline.get(key)
        if base is None:
            rows.append(_comparison_row(row, None, max_regression, "missing_baseline"))
            continue
        rows.append(_comparison_row(row, base, max_regression, ""))
    return rows


def _comparison_row(
    current: dict[str, str],
    baseline: dict[str, str] | None,
    max_regression: float,
    status: str,
) -> dict[str, float | str | int]:
    current_elapsed = _float(current.get("elapsed_s"))
    baseline_elapsed = _float(baseline.get("elapsed_s")) if baseline else 0.0
    delta = current_elapsed - baseline_elapsed
    delta_pct = delta / baseline_elapsed if baseline_elapsed > 0 else 0.0
    if not status:
        status = "regression" if delta_pct > max_regression else "ok"
    return {
        "scenario": current.get("scenario", ""),
        "seed": int(_float(current.get("seed"))),
        "baseline_elapsed_s": round(baseline_elapsed, 4),
        "current_elapsed_s": round(current_elapsed, 4),
        "elapsed_delta_s": round(delta, 4),
        "elapsed_delta_pct": round(delta_pct * 100.0, 2),
        "threshold_pct": round(max_regression * 100.0, 2),
        "status": status,
    }


def comparison_markdown(
    rows: list[dict[str, float | str | int]], max_regression: float
) -> str:
    lines = [
        "### Perf regression delta",
        "",
        f"Threshold: `{max_regression * 100.0:.1f}%` elapsed-time regression.",
        "",
        "| scenario | seed | baseline s | current s | delta % | status |",
        "|---|---:|---:|---:|---:|---|",
    ]
    for row in rows:
        lines.append(
            "| {scenario} | {seed} | {baseline_elapsed_s:.4f} | "
            "{current_elapsed_s:.4f} | {elapsed_delta_pct:.2f} | {status} |".format(
                **row
            )
        )
    lines.append("")
    return "\n".join(lines)


def read_rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="") as handle:
        return list(csv.DictReader(handle))


def write_rows(path: Path, rows: list[dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as handle:
        fieldnames = sorted({key for row in rows for key in row.keys()})
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def _rows_by_key(rows: list[dict[str, str]]) -> dict[tuple[str, str], dict[str, str]]:
    return {(row["scenario"], row["seed"]): row for row in rows}


def _float(value: object) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


if __name__ == "__main__":
    raise SystemExit(main())
