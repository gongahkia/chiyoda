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
