from __future__ import annotations

import pytest

import pandas as pd

from chiyoda.analysis.external_validation import (
    compare_bottleneck_flow,
    load_petrack_trajectory,
    summarize_bottleneck_flow,
)


REFERENCE = "data/external/wuppertal_bottleneck_2018/040_c_56_h-.txt"


def test_loads_wuppertal_petrack_reference_and_computes_flow_summary():
    frame = load_petrack_trajectory(REFERENCE)

    summary = summarize_bottleneck_flow(frame, source="wuppertal_2018")

    assert summary.agent_count == 75
    assert summary.crossing_count == 75
    assert summary.sample_count == 63110
    assert summary.first_crossing_s == pytest.approx(0.52)
    assert summary.last_crossing_s == pytest.approx(65.0)
    assert summary.mean_flow_ped_s == pytest.approx(1.1631513648)
    assert summary.mean_time_headway_s > 0.0


def test_bottleneck_flow_comparison_reports_deltas():
    reference_frame = pd.DataFrame(
        [
            {"agent_id": 1, "time_s": 0.0, "x": 0.0, "y": 1.0},
            {"agent_id": 1, "time_s": 1.0, "x": 0.0, "y": -1.0},
            {"agent_id": 2, "time_s": 0.0, "x": 0.0, "y": 1.0},
            {"agent_id": 2, "time_s": 2.0, "x": 0.0, "y": -1.0},
        ]
    )
    simulated_frame = pd.DataFrame(
        [
            {"agent_id": 1, "time_s": 0.0, "x": 0.0, "y": 1.0},
            {"agent_id": 1, "time_s": 2.0, "x": 0.0, "y": -1.0},
            {"agent_id": 2, "time_s": 0.0, "x": 0.0, "y": 1.0},
            {"agent_id": 2, "time_s": 4.0, "x": 0.0, "y": -1.0},
        ]
    )

    comparison = compare_bottleneck_flow(
        summarize_bottleneck_flow(simulated_frame, source="simulated"),
        summarize_bottleneck_flow(reference_frame, source="reference"),
    )

    row = comparison[comparison["metric"] == "mean_flow_ped_s"].iloc[0]
    assert row["reference"] == pytest.approx(2.0)
    assert row["simulated"] == pytest.approx(1.0)
    assert row["delta"] == pytest.approx(-1.0)
