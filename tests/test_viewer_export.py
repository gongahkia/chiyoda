from __future__ import annotations

import json

import pandas as pd

from chiyoda.analysis.viewer import export_viewer
from chiyoda.studies.models import StudyBundle


def _bundle() -> StudyBundle:
    return StudyBundle(
        metadata={
            "study_name": "viewer_test",
            "scenario_name": "viewer_test",
            "representative_run_id": "run_1",
            "layout_text": "XXX\nXEX\nX@X",
            "layout_width": 3,
            "layout_height": 3,
            "layout_cell_size": 1.0,
            "bottleneck_zones": [{"zone_id": "bn_1", "cells": [[1, 1]], "orientation": "vertical"}],
        },
        summary=pd.DataFrame(),
        steps=pd.DataFrame(),
        cells=pd.DataFrame(),
        agent_steps=pd.DataFrame(
            [
                {
                    "run_id": "run_1",
                    "step": 0,
                    "agent_id": 1,
                    "x": 1.0,
                    "y": 2.0,
                    "speed": 0.0,
                    "entropy": 0.5,
                    "state": "CALM",
                    "decision_mode": "EVACUATE",
                }
            ]
        ),
        agents=pd.DataFrame(),
        bottlenecks=pd.DataFrame(),
        dwell_samples=pd.DataFrame(),
        exits=pd.DataFrame(),
        hazards=pd.DataFrame(
            [{"run_id": "run_1", "step": 0, "time_s": 0.0, "x": 1.0, "y": 1.0, "radius": 1.0}]
        ),
    )


def test_export_viewer_writes_static_threejs_artifact(tmp_path):
    exported = export_viewer(_bundle(), tmp_path)
    data = json.loads((tmp_path / "viewer_data.json").read_text())
    html = (tmp_path / "index.html").read_text()

    assert tmp_path / "index.html" in exported
    assert tmp_path / "viewer_data.json" in exported
    assert "three.module.js" in html
    assert data["metadata"]["study_name"] == "viewer_test"
    assert data["frames"][0]["agents"][0]["intent"] == "EVACUATE"
    assert data["layout"]
