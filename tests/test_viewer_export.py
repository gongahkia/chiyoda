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
        cells=pd.DataFrame(
            [
                {
                    "run_id": "run_1",
                    "step": 0,
                    "time_s": 0.0,
                    "x": 1,
                    "y": 1,
                    "occupancy": 1,
                    "density": 0.2,
                    "speed": 0.1,
                    "path_usage": 3,
                }
            ]
        ),
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
    assert "OrbitControls" in html
    assert "authorMode" in html
    assert "paintToken" in html
    assert "pathUsage" in html
    assert "validationOverlay" in html
    assert "validateEditorGrid" in html
    assert "renderValidationOverlay" in html
    assert "exportScenarioYaml" in html
    assert "chiyoda_edited_scenario.yaml" in html
    assert data["metadata"]["study_name"] == "viewer_test"
    assert data["frames"][0]["agents"][0]["intent"] == "EVACUATE"
    assert data["layout"]
    assert data["path_usage"] == [{"x": 1, "y": 1, "path_usage": 3}]
    assert data["layout_grid"] == [["X", "X", "X"], ["X", "E", "X"], ["X", "@", "X"]]


def test_export_viewer_includes_source_geojson_levels(tmp_path):
    geojson = tmp_path / "station.geojson"
    geojson.write_text(
        json.dumps(
            {
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "properties": {"indoor": "corridor", "level": "-1"},
                        "geometry": {
                            "type": "Polygon",
                            "coordinates": [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
                        },
                    },
                    {
                        "type": "Feature",
                        "properties": {"railway": "platform", "level": "-2"},
                        "geometry": {"type": "LineString", "coordinates": [[0, 3], [2, 3]]},
                    },
                ],
            }
        )
    )
    scenario = tmp_path / "scenario.yaml"
    scenario.write_text(
        """
scenario:
  name: viewer_levels
  layout:
    geojson:
      file: station.geojson
      cell_size: 1.0
      padding: 0
"""
    )
    bundle = _bundle()
    bundle.metadata["scenario_file"] = str(scenario)
    bundle.metadata["layout_origin_x"] = 0.0
    bundle.metadata["layout_origin_y"] = 0.0

    export_viewer(bundle, tmp_path / "viewer")
    data = json.loads((tmp_path / "viewer" / "viewer_data.json").read_text())

    assert [floor["level"] for floor in data["floors"]] == ["-1", "-2"]
    assert data["floors"][0]["features"][0]["role"] == "corridor"
    assert data["floors"][1]["features"][0]["role"] == "platform"
