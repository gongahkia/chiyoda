from __future__ import annotations

import hashlib
import json
import logging
import shutil
import subprocess

import pandas as pd
import pytest

from chiyoda.analysis.viewer import export_viewer
from chiyoda.scenarios.validation import validate_scenario_file
from chiyoda.studies.models import StudyBundle
from chiyoda.studies.runner import run_study


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
            "requested_pathfinding_strategy": "auto",
            "effective_pathfinding_strategy": "reverse_dijkstra",
            "runs": [
                {
                    "run_id": "run_1",
                    "requested_pathfinding_strategy": "auto",
                    "effective_pathfinding_strategy": "reverse_dijkstra",
                    "last_effective_pathfinding_strategy": "reverse_dijkstra",
                    "route_cache_hits": 3,
                    "route_cache_misses": 2,
                    "path_computations": 2,
                    "pathfinding_fallback_count": 0,
                    "routing_wall_time_s": 0.125,
                    "pathfinding_strategy_counts": {"reverse_dijkstra": 5},
                }
            ],
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
    assert tmp_path / "img" / "opengameart_player_41.png" in exported
    assert tmp_path / "img" / "README.md" in exported
    assert (tmp_path / "img" / "opengameart_player_41.png").exists()
    assert "three.module.js" in html
    assert "OrbitControls" in html
    assert "opengameart_player_41.png" in html
    assert "THREE.SpriteMaterial" in html
    assert "setTimeout" not in html
    assert "runBrowserSimulation" in html
    assert "browserSim" in html
    assert "simStatus" in html
    assert "authorTool" in html
    assert "connectorType" in html
    assert "connectorToFloor" in html
    assert "hostileObjective" in html
    assert "authoredHostileChannels" in html
    assert "hostile_channels" in html
    assert "dispatcherPanel" in html
    assert "routingPanel" in html
    assert "routingStatus" in html
    assert "routingPathUsage" in html
    assert "dispatchMessageType" in html
    assert "projectDispatchMessage" in html
    assert "commitDispatchMarker" in html
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
    assert data["origin"] == {"path": "", "sha256": ""}
    assert data["source_scenario"] == {}
    assert data["frames"][0]["agents"][0]["intent"] == "EVACUATE"
    assert data["frames"][0]["agents"][0]["cell_x"] == 1
    assert data["frames"][0]["agents"][0]["cell_y"] == 2
    assert data["browser_sim"]["enabled"] is True
    assert data["browser_sim"]["duration_s"] == 60
    assert data["pathfinding"]["requested_strategy"] == "auto"
    assert data["pathfinding"]["effective_strategy"] == "reverse_dijkstra"
    assert data["pathfinding"]["route_cache_hits"] == 3
    assert data["layout"]
    assert data["path_usage"] == [
        {"floor_id": "0", "z": 0.0, "x": 1, "y": 1, "path_usage": 3}
    ]
    assert data["layout_grid"] == [["X", "X", "X"], ["X", "E", "X"], ["X", "@", "X"]]
    assert data["layout_floors"][0]["id"] == "0"
    assert data["layout_connectors"][0]["id"] == "stairs_a"
    assert data["qa"]["ok"] is True
    assert data["qa"]["layout_floor_count"] == 1
    assert data["qa"]["frame_count"] == 1
    assert data["qa"]["agent_sample_count"] == 1
    assert data["qa"]["exit_count"] == 1


def test_export_viewer_qa_flags_empty_replay(tmp_path):
    bundle = _bundle()
    bundle.agent_steps = pd.DataFrame()

    export_viewer(bundle, tmp_path)
    data = json.loads((tmp_path / "viewer_data.json").read_text())

    assert data["qa"]["ok"] is False
    assert "no_frames" in data["qa"]["warnings"]
    assert "no_agent_samples" in data["qa"]["warnings"]


def test_viewer_authored_connector_and_hostile_yaml_round_trips(tmp_path):
    scenario = tmp_path / "authored.yaml"
    scenario.write_text(
        """
scenario:
  name: viewer_authored
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |-
          XXX
          X@X
          X.X
          XXX
      - id: "1"
        z: 3.0
        text: |-
          XXX
          XEX
          XXX
    connectors:
      - id: "viewer_connector_1"
        type: "stairs"
        from: {floor: "0", x: 1, y: 2}
        to: {floor: "1", x: 1, y: 1}
        bidirectional: true
        capacity: 30
  population:
    total: 1
    cohorts:
      - name: baseline
        count: 1
        spawn_cells:
          - {floor: "0", x: 1, y: 1}
  hostile_channels:
    - id: "viewer_hostile_1"
      channel_type: "gossip"
      objective: "false-protective-action"
      budget: 1
      start_step: 0
      interval_steps: 5
      plausibility: 0.7
      radius: 6
      source_id: "viewer_hostile_1"
      target_cohort: "baseline"
      claimed_exit:
        floor: "0"
        x: 1
        y: 2
  simulation:
    max_steps: 40
    dt: 0.1
    random_seed: 42
"""
    )

    result = validate_scenario_file(scenario)

    assert not result.has_errors


def test_export_viewer_records_source_origin_hash(tmp_path):
    scenario = _write_small_scenario(tmp_path, "origin_hash", "XXXXX\nX@.EX\nXXXXX", 1)
    bundle = run_study(str(scenario))

    export_viewer(bundle, tmp_path / "viewer")
    data = json.loads((tmp_path / "viewer" / "viewer_data.json").read_text())

    assert data["origin"]["path"] == str(scenario.resolve())
    assert data["origin"]["sha256"] == hashlib.sha256(scenario.read_bytes()).hexdigest()
    assert data["source_scenario"]["name"] == "origin_hash"


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


def test_browser_sim_supports_multifloor_connectors(tmp_path):
    bundle = run_study("scenarios/validation_multifloor_connectors.yaml")
    viewer_dir = tmp_path / "multifloor_viewer"
    export_viewer(bundle, viewer_dir)

    data = json.loads((viewer_dir / "viewer_data.json").read_text())
    summary = _run_browser_sim(viewer_dir / "viewer_data.json")

    assert data["browser_sim"]["enabled"] is True
    assert data["browser_sim"]["scope"] == "multi_floor_200_agents_no_llm"
    assert summary["floor_count"] == 3
    assert summary["connector_count"] == 4
    assert summary["evacuated"] == 4
    assert summary["connector_usage"]["stairs_main"] >= 1
    assert summary["connector_usage"]["ramp_main"] >= 1
    assert summary["connector_usage"]["escalator_main"] >= 1
    assert summary["connector_usage"]["elevator_main"] >= 1


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
