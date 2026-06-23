from __future__ import annotations

import json

import pandas as pd

from chiyoda.analysis.fundamental_diagram import (
    load_specific_flow_reference,
    specific_flow_rmse,
    write_specific_flow_report,
)


def test_juelich_reference_covers_width_range():
    reference = load_specific_flow_reference()

    assert reference["width_m"].tolist() == [0.8, 1.0, 1.2, 1.4, 1.6]
    assert reference["specific_flow_ped_m_s"].min() > 1.5


def test_juelich_bottleneck_width_curve_report(tmp_path):
    summary = write_specific_flow_report(tmp_path)
    comparison = pd.read_csv(tmp_path / "juelich_specific_flow_comparison.csv")
    rmse = specific_flow_rmse(comparison)

    assert len(comparison) == 5
    assert rmse <= 0.25
    assert summary["ok"] is True
    assert (tmp_path / "juelich_specific_flow_curve.png").exists()
    persisted = json.loads(
        (tmp_path / "juelich_specific_flow_summary.json").read_text()
    )
    assert persisted["rmse_specific_flow_ped_m_s"] <= 0.25
