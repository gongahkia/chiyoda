from __future__ import annotations

import pandas as pd
import pytest

from chiyoda.analysis.trajectory_reference import (
    compare_trajectory_reference,
    summarize_trajectory_frame,
)


def test_summarize_trajectory_frame_computes_basic_motion_metrics():
    frame = pd.DataFrame(
        [
            {"agent_id": 1, "time_s": 0.0, "x": 0.0, "y": 0.0, "local_density": 0.2},
            {"agent_id": 1, "time_s": 1.0, "x": 3.0, "y": 4.0, "local_density": 0.4},
            {"agent_id": 2, "time_s": 0.0, "x": 1.0, "y": 1.0, "local_density": 0.3},
            {"agent_id": 2, "time_s": 1.0, "x": 1.0, "y": 2.0, "local_density": 0.5},
        ]
    )

    summary = summarize_trajectory_frame(frame)

    assert summary["agent_count"] == 2
    assert summary["sample_count"] == 4
    assert summary["duration_s"] == pytest.approx(1.0)
    assert summary["mean_path_length_m"] == pytest.approx(3.0)
    assert summary["mean_speed_mps"] == pytest.approx(3.0)
    assert summary["mean_local_density"] == pytest.approx(0.35)


def test_compare_trajectory_reference_groups_simulation_variants():
    simulated = pd.DataFrame(
        [
            {
                "variant_name": "slow",
                "agent_id": 1,
                "time_s": 0.0,
                "x": 0.0,
                "y": 0.0,
                "speed": 0.0,
            },
            {
                "variant_name": "slow",
                "agent_id": 1,
                "time_s": 1.0,
                "x": 1.0,
                "y": 0.0,
                "speed": 1.0,
            },
            {
                "variant_name": "fast",
                "agent_id": 1,
                "time_s": 0.0,
                "x": 0.0,
                "y": 0.0,
                "speed": 0.0,
            },
            {
                "variant_name": "fast",
                "agent_id": 1,
                "time_s": 1.0,
                "x": 2.0,
                "y": 0.0,
                "speed": 2.0,
            },
        ]
    )
    reference = pd.DataFrame(
        [
            {"agent_id": 99, "time_s": 0.0, "x": 0.0, "y": 0.0, "speed": 0.0},
            {"agent_id": 99, "time_s": 1.0, "x": 1.5, "y": 0.0, "speed": 1.5},
        ]
    )

    comparison = compare_trajectory_reference(simulated, reference)
    path_rows = comparison[comparison["metric"] == "mean_path_length_m"].set_index(
        "variant_name"
    )

    assert path_rows.loc["slow", "simulated"] == pytest.approx(1.0)
    assert path_rows.loc["slow", "reference"] == pytest.approx(1.5)
    assert path_rows.loc["slow", "delta"] == pytest.approx(-0.5)
    assert path_rows.loc["fast", "simulated"] == pytest.approx(2.0)
    assert path_rows.loc["fast", "delta"] == pytest.approx(0.5)
