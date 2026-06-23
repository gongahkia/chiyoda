from __future__ import annotations

from pathlib import Path

from chiyoda._logging import get_logger
from scripts import perf_regression_suite


def test_perf_regression_suite_defaults_exist():
    expected = {
        "scenarios/benchmark/transit_cbrn.yaml",
        "scenarios/benchmark/transit_cbrn_10k.yaml",
        "scenarios/benchmark/wildfire_wui.yaml",
    }

    assert set(perf_regression_suite.DEFAULT_SCENARIOS) == expected
    assert all(Path(path).exists() for path in perf_regression_suite.DEFAULT_SCENARIOS)


def test_perf_regression_suite_smoke_row_has_timing_and_memory():
    get_logger().setLevel("WARNING")

    row = perf_regression_suite._run_one(
        "scenarios/benchmark/transit_cbrn.yaml", seed=42
    )

    assert row["scenario"] == "scenarios/benchmark/transit_cbrn.yaml"
    assert row["seed"] == 42
    assert row["elapsed_s"] >= 0.0
    assert row["rss_delta_mib"] >= 0.0
    assert row["step_count"] > 0


def test_perf_compare_flags_elapsed_regression(tmp_path):
    baseline = tmp_path / "baseline.csv"
    current = tmp_path / "current.csv"
    perf_regression_suite.write_rows(
        baseline,
        [
            {
                "scenario": "scenario.yaml",
                "seed": 42,
                "elapsed_s": 10.0,
            }
        ],
    )
    perf_regression_suite.write_rows(
        current,
        [
            {
                "scenario": "scenario.yaml",
                "seed": 42,
                "elapsed_s": 11.2,
            }
        ],
    )

    rows = perf_regression_suite.compare_perf(baseline, current, 0.10)

    assert rows[0]["status"] == "regression"
    assert rows[0]["elapsed_delta_pct"] == 12.0


def test_perf_compare_accepts_within_threshold(tmp_path):
    baseline = tmp_path / "baseline.csv"
    current = tmp_path / "current.csv"
    perf_regression_suite.write_rows(
        baseline,
        [{"scenario": "scenario.yaml", "seed": 42, "elapsed_s": 10.0}],
    )
    perf_regression_suite.write_rows(
        current,
        [{"scenario": "scenario.yaml", "seed": 42, "elapsed_s": 10.5}],
    )

    rows = perf_regression_suite.compare_perf(baseline, current, 0.10)

    assert rows[0]["status"] == "ok"
