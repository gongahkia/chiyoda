from __future__ import annotations

import pandas as pd
import pytest

from chiyoda.analysis.external_validation import (
    bottleneck_travel_times_by_density,
    compare_bottleneck_flow,
    compare_density_band_travel_times,
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


def test_wuppertal_reference_reports_density_banded_travel_times():
    frame = load_petrack_trajectory(REFERENCE)

    distribution = bottleneck_travel_times_by_density(frame)

    assert len(distribution) == 75
    assert distribution["travel_time_s"].min() > 0.0
    assert set(distribution["density_band"]).issubset({"low", "medium", "high"})
    assert distribution["local_density_ped_m2"].max() > 0.0


def test_density_band_travel_time_ks_reports_soft_fail():
    line = ((0.35, 0.0), (-0.35, 0.0))
    reference = _synthetic_crossing_frame([1.0 + index * 0.05 for index in range(40)])
    simulated = _synthetic_crossing_frame([8.0 + index * 0.05 for index in range(40)])

    comparison = compare_density_band_travel_times(
        simulated,
        reference,
        simulated_line=line,
        reference_line=line,
    )

    low = comparison[comparison["density_band"] == "low"].iloc[0]
    assert low["reference_count"] == 40
    assert low["simulated_count"] == 40
    assert low["ks_pvalue"] < 0.01
    assert bool(low["soft_fail"]) is True


def _synthetic_crossing_frame(crossing_times: list[float]) -> pd.DataFrame:
    rows = []
    for agent_id, crossing_time in enumerate(crossing_times, start=1):
        rows.append({"agent_id": agent_id, "time_s": 0.0, "x": 0.0, "y": 1.0})
        rows.append(
            {
                "agent_id": agent_id,
                "time_s": float(crossing_time),
                "x": 0.0,
                "y": -1.0,
            }
        )
    return pd.DataFrame(rows)
