from __future__ import annotations

import json
import logging
import shutil
import subprocess

import pandas as pd
import pytest

from chiyoda.analysis.viewer import export_viewer
from chiyoda.studies.runner import run_study
from chiyoda.studies.models import StudyBundle


def _bundle() -> StudyBundle:
    return StudyBundle(
        metadata={
            "study_name": "viewer_test",
            "scenario_name": "viewer_test",
            "representative_run_id": "run_1",
            "layout_text": "XXX\nXEX\nX@X",
            "layout_floors": [{"id": "0", "z": 0.0, "text": "XXX\nXEX\nX@X"}],
            "layout_connectors": [
                {
                    "id": "stairs_a",
                    "type": "stairs",
                    "from": {"floor": "0", "x": 1, "y": 2},
                    "to": {"floor": "0", "x": 1, "y": 1},
                    "bidirectional": True,
                }
            ],
            "layout_width": 3,
            "layout_height": 3,
            "layout_cell_size": 1.0,
            "bottleneck_zones": [
                {"zone_id": "bn_1", "cells": [[1, 1]], "orientation": "vertical"}
            ],
        },
        summary=pd.DataFrame(),
        steps=pd.DataFrame(),
        cells=pd.DataFrame(
            [
                {
                    "run_id": "run_1",
                    "step": 0,
                    "time_s": 0.0,
                    "floor_id": "0",
                    "z": 0.0,
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
                    "floor_id": "0",
                    "x": 1.0,
                    "y": 2.0,
                    "z": 0.0,
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
            [
                {
                    "run_id": "run_1",
                    "step": 0,
                    "time_s": 0.0,
                    "x": 1.0,
                    "y": 1.0,
                    "z": 0.0,
                    "radius": 1.0,
                }
            ]
        ),
    )


def test_export_viewer_writes_static_threejs_artifact(tmp_path):
    exported = export_viewer(_bundle(), tmp_path)
    data = json.loads((tmp_path / "viewer_data.json").read_text())
    html = (tmp_path / "index.html").read_text()

    assert tmp_path / "index.html" in exported
    assert tmp_path / "viewer_data.json" in exported
    assert tmp_path / "js" / "sim" / "browser_sim.js" in exported
    assert "three.module.js" in html
    assert "OrbitControls" in html
    assert "runBrowserSimulation" in html
    assert "browserSim" in html
    assert "simStatus" in html
    assert "authorMode" in html
    assert "activeFloor" in html
    assert "runtimeConnectors" in html
    assert "paintToken" in html
    assert "pathUsage" in html
    assert "validationOverlay" in html
    assert "validateEditorGrid" in html
    assert "renderValidationOverlay" in html
    assert "exportScenarioYaml" in html
    assert "chiyoda_edited_scenario.yaml" in html
    assert data["metadata"]["study_name"] == "viewer_test"
    assert data["frames"][0]["agents"][0]["intent"] == "EVACUATE"
    assert data["frames"][0]["agents"][0]["cell_x"] == 1
    assert data["frames"][0]["agents"][0]["cell_y"] == 2
    assert data["browser_sim"]["enabled"] is True
    assert data["browser_sim"]["duration_s"] == 60
    assert data["layout"]
    assert data["path_usage"] == [
        {"floor_id": "0", "z": 0.0, "x": 1, "y": 1, "path_usage": 3}
    ]
    assert data["layout_grid"] == [["X", "X", "X"], ["X", "E", "X"], ["X", "@", "X"]]
    assert data["layout_floors"][0]["id"] == "0"
    assert data["layout_connectors"][0]["id"] == "stairs_a"


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
                        "geometry": {
                            "type": "LineString",
                            "coordinates": [[0, 3], [2, 3]],
                        },
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


def test_browser_sim_js_runs_exported_payload(tmp_path):
    export_viewer(_bundle(), tmp_path)

    summary = _run_browser_sim(tmp_path / "viewer_data.json")

    assert summary["initial_agents"] == 1
    assert summary["evacuated"] == 1
    assert summary["duration_s"] == 60
    assert summary["sim_steps_per_second"] >= 10


def test_browser_sim_matches_cli_egress_for_three_small_scenarios(tmp_path):
    logger = logging.getLogger("chiyoda")
    was_disabled = logger.disabled
    logger.disabled = True
    scenarios = [
        ("single_corridor", "XXXXX\nX@.EX\nXXXXX", 1),
        ("wide_corridor", "XXXXXXX\nX@...EX\nX.....X\nXXXXXXX", 2),
        ("two_starts", "XXXXXXXX\nX@....EX\nX......X\nX@....EX\nXXXXXXXX", 3),
    ]

    try:
        for name, layout, population in scenarios:
            scenario_file = _write_small_scenario(tmp_path, name, layout, population)
            bundle = run_study(str(scenario_file))
            viewer_dir = tmp_path / f"{name}_viewer"
            export_viewer(bundle, viewer_dir)
            summary = _run_browser_sim(viewer_dir / "viewer_data.json")
            cli_egress = int(bundle.summary.iloc[0]["agents_evacuated"])

            assert cli_egress > 0
            assert abs(summary["evacuated"] - cli_egress) / cli_egress <= 0.05
    finally:
        logger.disabled = was_disabled


def _run_browser_sim(data_path):
    node = shutil.which("node")
    if node is None:
        pytest.skip("node is required for browser sim module smoke test")
    script = f"""
import fs from "node:fs";
import {{ runBrowserSimulation }} from "./chiyoda/analysis/viewer_assets/js/sim/browser_sim.js";
const data = JSON.parse(fs.readFileSync({json.dumps(str(data_path))}, "utf8"));
const result = runBrowserSimulation(data, {{ durationS: 60, targetStepsPerSecond: 10 }});
if (!result.ok) throw new Error(result.reason);
console.log(JSON.stringify(result.summary));
"""
    result = subprocess.run(
        [node, "--input-type=module", "-e", script],
        cwd=".",
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def _write_small_scenario(tmp_path, name: str, layout: str, population: int):
    path = tmp_path / f"{name}.yaml"
    indented_layout = "\n".join(f"        {line}" for line in layout.splitlines())
    path.write_text(
        f"""scenario:
  name: {name}
  layout:
    cell_size: 1.0
    floors:
    - id: '0'
      z: 0.0
      text: |-
{indented_layout}
  population:
    total: {population}
  simulation:
    max_steps: 600
    dt: 0.1
    random_seed: 42
"""
    )
    return path
