from __future__ import annotations

import pandas as pd

from chiyoda.analysis.statistics import compare_variants
from chiyoda.studies.models import StudyBundle


def _empty_frame() -> pd.DataFrame:
    return pd.DataFrame()


def test_compare_variants_uses_run_level_rows_only():
    summary = pd.DataFrame(
        [
            {
                "record_type": "run",
                "variant_name": "baseline",
                "mean_travel_time_s": 10.0,
                "information_safety_efficiency": 0.01,
            },
            {
                "record_type": "run",
                "variant_name": "baseline",
                "mean_travel_time_s": 12.0,
                "information_safety_efficiency": 0.02,
            },
            {
                "record_type": "variant_aggregate",
                "variant_name": "baseline",
                "mean_travel_time_s": 999.0,
                "information_safety_efficiency": 9.99,
            },
            {
                "record_type": "run",
                "variant_name": "test",
                "mean_travel_time_s": 8.0,
                "information_safety_efficiency": 0.04,
            },
            {
                "record_type": "run",
                "variant_name": "test",
                "mean_travel_time_s": 9.0,
                "information_safety_efficiency": 0.05,
            },
            {
                "record_type": "variant_aggregate",
                "variant_name": "test",
                "mean_travel_time_s": -999.0,
                "information_safety_efficiency": -9.99,
            },
        ]
    )
    bundle = StudyBundle(
        metadata={},
        summary=summary,
        steps=_empty_frame(),
        cells=_empty_frame(),
        agent_steps=_empty_frame(),
        agents=_empty_frame(),
        bottlenecks=_empty_frame(),
        dwell_samples=_empty_frame(),
        exits=_empty_frame(),
        hazards=_empty_frame(),
    )

    result = compare_variants(
        bundle,
        "baseline",
        "test",
        metrics=["mean_travel_time_s", "information_safety_efficiency"],
    ).set_index("metric")

    assert result.loc["mean_travel_time_s", "baseline_mean"] == 11.0
    assert result.loc["mean_travel_time_s", "test_mean"] == 8.5
    assert result.loc["information_safety_efficiency", "baseline_mean"] == 0.015
    assert result.loc["information_safety_efficiency", "test_mean"] == 0.045
    assert result.loc["mean_travel_time_s", "n_baseline"] == 2
    assert result.loc["mean_travel_time_s", "n_test"] == 2
