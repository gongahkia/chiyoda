from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
from typing import Any

import pandas as pd
import yaml

from chiyoda.studies.models import StudyBundle

_VIEWER_SIM_ASSET = (
    Path(__file__).with_name("viewer_assets") / "js" / "sim" / "browser_sim.js"
)


def export_viewer(
    bundle: StudyBundle,
    output_dir: str | Path,
    *,
    max_frames: int = 500,
) -> list[Path]:
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    payload = _viewer_payload(bundle, max_frames=max_frames)
    data_path = out / "viewer_data.json"
    index_path = out / "index.html"
    sim_asset_path = out / "js" / "sim" / "browser_sim.js"
    data_path.write_text(json.dumps(_json_safe(payload), indent=2) + "\n")
    index_path.write_text(_viewer_html())
    sim_asset_path.parent.mkdir(parents=True, exist_ok=True)
    sim_asset_path.write_text(_VIEWER_SIM_ASSET.read_text())
    return [index_path, data_path, sim_asset_path]


def _viewer_payload(bundle: StudyBundle, *, max_frames: int) -> dict[str, Any]:
    run_id = str(bundle.metadata.get("representative_run_id") or "")
    floors = _source_floors(bundle) + _source_ifc_floors(bundle)
    layout_floors = _layout_floors(bundle.metadata)
    origin, source_scenario = _scenario_origin(bundle.metadata)
    agent_steps = bundle.agent_steps.copy()
    if run_id and "run_id" in agent_steps.columns:
        agent_steps = agent_steps[agent_steps["run_id"] == run_id]
    if agent_steps.empty:
        frames: list[dict[str, Any]] = []
    else:
        steps = sorted(
            pd.to_numeric(agent_steps["step"], errors="coerce")
            .dropna()
            .unique()
            .tolist()
        )
        selected = set(_sample_values([int(step) for step in steps], max_frames))
        frames = []
        for step, frame in agent_steps[agent_steps["step"].isin(selected)].groupby(
            "step", sort=True
        ):
            agents = []
            for row in frame.itertuples(index=False):
                agents.append(
                    {
                        "id": int(row.agent_id),
                        "x": float(row.x),
                        "y": float(row.y),
                        "z": float(getattr(row, "z", 0.0)),
                        "floor_id": str(getattr(row, "floor_id", "0")),
                        "cell_x": int(getattr(row, "cell_x", math.floor(row.x))),
                        "cell_y": int(getattr(row, "cell_y", math.floor(row.y))),
                        "speed": float(getattr(row, "speed", 0.0)),
                        "entropy": float(getattr(row, "entropy", 0.0)),
                        "state": str(getattr(row, "state", "")),
                        "intent": str(getattr(row, "decision_mode", "")),
                    }
                )
            frames.append({"step": int(step), "agents": agents})

    return {
        "metadata": {
            "study_name": bundle.metadata.get("study_name"),
            "scenario_name": bundle.metadata.get("scenario_name"),
            "run_id": run_id,
            "layout_width": bundle.metadata.get("layout_width", 0),
            "layout_height": bundle.metadata.get("layout_height", 0),
            "layout_cell_size": bundle.metadata.get("layout_cell_size", 1.0),
            "layout_origin_x": bundle.metadata.get("layout_origin_x", 0.0),
            "layout_origin_y": bundle.metadata.get("layout_origin_y", 0.0),
            "station_provenance": bundle.metadata.get("station_provenance"),
        },
        "origin": origin,
        "source_scenario": source_scenario,
        "layout": _layout_cells(str(bundle.metadata.get("layout_text", ""))),
        "layout_grid": _layout_grid(str(bundle.metadata.get("layout_text", ""))),
        "layout_floors": layout_floors,
        "layout_connectors": bundle.metadata.get("layout_connectors", []),
        "browser_sim": _browser_sim_payload(layout_floors, frames),
        "floors": floors,
        "bottlenecks": bundle.metadata.get("bottleneck_zones", []),
        "path_usage": _path_usage_cells(bundle.cells, run_id=run_id),
        "hazards": _table_rows(bundle.hazards, run_id=run_id),
        "interventions": _table_rows(bundle.interventions, run_id=run_id),
        "llm_decisions": _table_rows(bundle.llm_decisions, run_id=run_id),
        "frames": frames,
    }


def _scenario_origin(metadata: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    raw_path = metadata.get("scenario_file")
    origin = {"path": "", "sha256": ""}
    if not raw_path:
        return origin, {}
    path = Path(str(raw_path)).resolve()
    if not path.exists():
        origin["path"] = str(path)
        return origin, {}
    content = path.read_bytes()
    origin = {"path": str(path), "sha256": hashlib.sha256(content).hexdigest()}
    try:
        payload = yaml.safe_load(content.decode()) or {}
    except Exception:
        return origin, {}
    scenario = payload.get("scenario", payload) if isinstance(payload, dict) else {}
    return origin, scenario if isinstance(scenario, dict) else {}


def _browser_sim_payload(
    layout_floors: list[dict[str, Any]], frames: list[dict[str, Any]]
) -> dict[str, Any]:
    initial_agents = len(frames[0]["agents"]) if frames else 0
    floor_count = len(layout_floors)
    has_exit = any(
        token == "E"
        for floor in layout_floors[:1]
        for row in floor.get("grid", [])
        for token in row
    )
    enabled = floor_count == 1 and 0 < initial_agents <= 200 and has_exit
    return {
        "enabled": enabled,
        "scope": "single_floor_200_agents_no_llm",
        "duration_s": 60,
        "target_steps_per_second": 10,
        "agent_limit": 200,
        "initial_agent_count": initial_agents,
        "floor_count": floor_count,
        "has_exit": has_exit,
    }


def _layout_cells(layout_text: str) -> list[dict[str, Any]]:
    cells = []
    for y, line in enumerate(layout_text.splitlines()):
        for x, token in enumerate(line):
            if token != ".":
                cells.append({"x": x, "y": y, "token": token})
    return cells


def _layout_grid(layout_text: str) -> list[list[str]]:
    return [list(line) for line in layout_text.splitlines()]


def _layout_floors(metadata: dict[str, Any]) -> list[dict[str, Any]]:
    floors = metadata.get("layout_floors")
    if isinstance(floors, list) and floors:
        return [
            {
                "id": str(floor.get("id", index)),
                "z": float(floor.get("z", 0.0) or 0.0),
                "grid": _layout_grid(str(floor.get("text", ""))),
            }
            for index, floor in enumerate(floors)
            if isinstance(floor, dict)
        ]
    return [
        {
            "id": "0",
            "z": 0.0,
            "grid": _layout_grid(str(metadata.get("layout_text", ""))),
        }
    ]


def _source_floors(bundle: StudyBundle) -> list[dict[str, Any]]:
    scenario_file = bundle.metadata.get("scenario_file")
    if not scenario_file:
        return []
    scenario_path = Path(str(scenario_file))
    if not scenario_path.exists():
        return []
    try:
        payload = yaml.safe_load(scenario_path.read_text())
    except Exception:
        return []
    scenario = payload.get("scenario", payload) if isinstance(payload, dict) else {}
    layout = scenario.get("layout", {}) if isinstance(scenario, dict) else {}
    geojson_cfg = layout.get("geojson") if isinstance(layout, dict) else None
    if geojson_cfg is None:
        return []
    if isinstance(geojson_cfg, str):
        geojson_cfg = {"file": geojson_cfg}
    if not isinstance(geojson_cfg, dict) or "file" not in geojson_cfg:
        return []
    source = Path(str(geojson_cfg["file"]))
    if not source.is_absolute():
        source = scenario_path.parent / source
    if not source.exists():
        return []
    try:
        geojson = json.loads(source.read_text())
    except Exception:
        return []

    origin_x = float(bundle.metadata.get("layout_origin_x", 0.0) or 0.0)
    origin_y = float(bundle.metadata.get("layout_origin_y", 0.0) or 0.0)
    cell_size = float(bundle.metadata.get("layout_cell_size", 1.0) or 1.0)
    by_level: dict[str, list[dict[str, Any]]] = {}
    for feature in geojson.get("features", []):
        if not isinstance(feature, dict):
            continue
        properties = dict(feature.get("properties", {}) or {})
        level = str(properties.get("level", properties.get("level_id", "unassigned")))
        geometry = feature.get("geometry", {}) or {}
        converted = _viewer_geometry(
            geometry, origin_x=origin_x, origin_y=origin_y, cell_size=cell_size
        )
        if converted is None:
            continue
        by_level.setdefault(level, []).append(
            {
                "role": _feature_role(properties),
                "name": str(properties.get("name", properties.get("ref", ""))),
                "geometry": converted,
            }
        )
    return [
        {"level": level, "features": features}
        for level, features in sorted(by_level.items(), key=lambda item: item[0])
    ]


def _source_ifc_floors(bundle: StudyBundle) -> list[dict[str, Any]]:
    metadata = bundle.metadata.get("scenario_metadata", {}) or {}
    ifc = metadata.get("ifc_import", {}) if isinstance(metadata, dict) else {}
    elements = ifc.get("elements", []) if isinstance(ifc, dict) else []
    by_floor: dict[str, list[dict[str, Any]]] = {}
    for element in elements:
        if not isinstance(element, dict):
            continue
        span = element.get("cell_span", {}) or {}
        try:
            x0 = float(span["x0"])
            y0 = float(span["y0"])
            x1 = float(span["x1"]) + 1.0
            y1 = float(span["y1"]) + 1.0
        except (KeyError, TypeError, ValueError):
            continue
        floor = str(element.get("floor", "0"))
        by_floor.setdefault(floor, []).append(
            {
                "role": str(element.get("role", "ifc")),
                "name": str(element.get("name", element.get("ifc_type", ""))),
                "source": "ifc",
                "ifc_type": str(element.get("ifc_type", "")),
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [
                        [[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]]
                    ],
                },
            }
        )
    return [
        {"level": f"ifc:{floor}", "features": features}
        for floor, features in sorted(by_floor.items(), key=lambda item: item[0])
    ]


def _viewer_geometry(
    geometry: dict[str, Any],
    *,
    origin_x: float,
    origin_y: float,
    cell_size: float,
) -> dict[str, Any] | None:
    kind = geometry.get("type")
    coords = geometry.get("coordinates")
    if kind == "Point":
        return {
            "type": kind,
            "coordinates": _project_point(coords, origin_x, origin_y, cell_size),
        }
    if kind == "LineString":
        return {
            "type": kind,
            "coordinates": [
                _project_point(point, origin_x, origin_y, cell_size) for point in coords
            ],
        }
    if kind == "Polygon":
        return {
            "type": kind,
            "coordinates": [
                [_project_point(point, origin_x, origin_y, cell_size) for point in ring]
                for ring in coords
            ],
        }
    if kind == "MultiLineString":
        return {
            "type": kind,
            "coordinates": [
                [_project_point(point, origin_x, origin_y, cell_size) for point in line]
                for line in coords
            ],
        }
    if kind == "MultiPolygon":
        return {
            "type": kind,
            "coordinates": [
                [
                    [
                        _project_point(point, origin_x, origin_y, cell_size)
                        for point in ring
                    ]
                    for ring in polygon
                ]
                for polygon in coords
            ],
        }
    return None


def _project_point(
    point: Any, origin_x: float, origin_y: float, cell_size: float
) -> list[float]:
    return [
        (float(point[0]) - origin_x) / cell_size,
        (float(point[1]) - origin_y) / cell_size,
    ]


def _feature_role(properties: dict[str, Any]) -> str:
    if properties.get("role"):
        return str(properties["role"])
    if properties.get("chiyoda_role"):
        return str(properties["chiyoda_role"])
    if str(properties.get("indoor", "")).lower() in {"wall", "column"}:
        return "wall"
    if properties.get("entrance") or str(properties.get("railway", "")).endswith(
        "entrance"
    ):
        return "exit"
    if str(properties.get("location_type", "")) == "2":
        return "exit"
    if (
        properties.get("public_transport") == "platform"
        or properties.get("railway") == "platform"
    ):
        return "platform"
    if properties.get("pathway_mode"):
        return "pathway"
    if properties.get("indoor"):
        return str(properties["indoor"])
    return "feature"


def _table_rows(frame: pd.DataFrame, *, run_id: str) -> list[dict[str, Any]]:
    if frame.empty:
        return []
    current = frame.copy()
    if run_id and "run_id" in current.columns:
        current = current[current["run_id"] == run_id]
    return current.to_dict(orient="records")


def _path_usage_cells(frame: pd.DataFrame, *, run_id: str) -> list[dict[str, Any]]:
    required = {"x", "y", "path_usage"}
    if frame.empty or not required.issubset(frame.columns):
        return []
    current = frame.copy()
    if run_id and "run_id" in current.columns:
        current = current[current["run_id"] == run_id]
    if current.empty:
        return []
    current["path_usage"] = pd.to_numeric(
        current["path_usage"], errors="coerce"
    ).fillna(0)
    grouped = current.groupby(["x", "y"], as_index=False)["path_usage"].max()
    if "floor_id" in current.columns:
        grouped = current.groupby(["floor_id", "x", "y"], as_index=False)[
            "path_usage"
        ].max()
    if "z" in current.columns:
        z_by_cell = (
            current.groupby(["floor_id", "x", "y"], as_index=False)["z"].first()
            if "floor_id" in current.columns
            else current.groupby(["x", "y"], as_index=False)["z"].first()
        )
        grouped = grouped.merge(z_by_cell, how="left")
    grouped = grouped[grouped["path_usage"] > 0]
    return grouped.to_dict(orient="records")


def _sample_values(values: list[int], max_count: int) -> list[int]:
    if len(values) <= max_count:
        return values
    stride = max(1, math.ceil(len(values) / max_count))
    sampled = values[::stride]
    if values[-1] not in sampled:
        sampled.append(values[-1])
    return sampled


def _json_safe(value: Any) -> Any:
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_json_safe(item) for item in value]
    if isinstance(value, tuple):
        return [_json_safe(item) for item in value]
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if pd.isna(value) if not isinstance(value, (dict, list, tuple)) else False:
        return None
    return value


def _viewer_html() -> str:
    return """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="icon" href="data:,">
  <title>Chiyoda 3D Viewer</title>
  <style>
    html, body { margin: 0; height: 100%; font-family: system-ui, sans-serif; background: #111; color: #f5f5f5; }
    #app { display: grid; grid-template-rows: auto 1fr; height: 100%; }
    #toolbar { display: flex; flex-wrap: wrap; gap: 10px 12px; align-items: center; padding: 10px 12px; background: #1d1d1d; border-bottom: 1px solid #333; }
    button, input, select, label { font: inherit; }
    button { background: #e6e6e6; border: 0; padding: 5px 10px; border-radius: 4px; cursor: pointer; }
    select { background: #e6e6e6; border: 0; padding: 5px 8px; border-radius: 4px; }
    input[type="range"] { width: min(55vw, 720px); }
    canvas { display: block; width: 100%; height: 100%; }
    .metric { color: #cfcfcf; min-width: 170px; }
    .dispatch { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; padding: 6px 8px; background: #282828; border-radius: 4px; }
  </style>
</head>
<body>
<div id="app">
  <div id="toolbar">
    <button id="play">Play</button>
    <input id="scrub" type="range" min="0" max="0" value="0">
    <span class="metric" id="step">step 0</span>
    <button id="resetCamera">Reset camera</button>
    <button id="browserSim">Run browser sim</button>
    <button id="resetReplay">Reset replay</button>
    <span class="metric" id="simStatus">browser sim idle</span>
    <div class="dispatch" id="dispatcherPanel">
      <select id="dispatchMessageType" aria-label="dispatch message type">
        <option value="route_guidance">route</option>
        <option value="avoid_hazard">avoid hazard</option>
        <option value="shelter_hold">hold</option>
        <option value="all_clear">all clear</option>
      </select>
      <label>radius <input id="dispatchRadius" type="number" min="0.1" step="0.5" value="8" style="width:4.5rem"></label>
      <label>cred <input id="dispatchCredibility" type="number" min="0" max="1" step="0.05" value="0.90" style="width:4.5rem"></label>
      <label>horizon <input id="dispatchHorizon" type="number" min="1" max="240" step="1" value="30" style="width:4.5rem"></label>
      <button id="projectDispatch">Project</button>
      <button id="commitDispatch" disabled>Commit marker</button>
      <span class="metric" id="dispatchStatus">projection idle</span>
    </div>
    <label><input id="sourceFloors" type="checkbox" checked> source floors</label>
    <label><input id="hazards" type="checkbox" checked> hazards</label>
    <label><input id="bottlenecks" type="checkbox" checked> bottlenecks</label>
    <label><input id="pathUsage" type="checkbox"> path usage</label>
    <label><input id="validationOverlay" type="checkbox" checked> validation</label>
    <label><input id="messages" type="checkbox" checked> messages</label>
    <label>floor gap <input id="floorGap" type="range" min="0" max="8" step="0.5" value="2.5"></label>
    <label>edit floor <select id="activeFloor" aria-label="edit floor"></select></label>
    <label><input id="connectors" type="checkbox" checked> connectors</label>
    <label><input id="authorMode" type="checkbox"> author</label>
    <select id="authorTool" aria-label="author tool">
      <option value="paint">paint</option>
      <option value="connector">connector</option>
      <option value="hostile">hostile</option>
      <option value="dispatch">dispatch</option>
    </select>
    <select id="paintToken" aria-label="paint token">
      <option value=".">floor</option>
      <option value="X">wall</option>
      <option value="E">exit</option>
      <option value="@">spawn</option>
      <option value="S">signage</option>
      <option value="R">responder</option>
    </select>
    <select id="connectorType" aria-label="connector type">
      <option value="stairs">stairs</option>
      <option value="ramp">ramp</option>
      <option value="escalator">escalator</option>
      <option value="elevator">elevator</option>
    </select>
    <label>to floor <select id="connectorToFloor" aria-label="connector target floor"></select></label>
    <label>capacity <input id="connectorCapacity" type="number" min="1" value="30" style="width:5rem"></label>
    <select id="hostileObjective" aria-label="hostile objective">
      <option value="false-protective-action">false action</option>
      <option value="threat-amplification">amplify threat</option>
      <option value="authority-confusion">authority</option>
      <option value="social-proof-poisoning">social proof</option>
    </select>
    <label>target <input id="hostileTarget" value="baseline" style="width:7rem"></label>
    <label>cred <input id="hostileCredibility" type="number" min="0.01" max="0.99" step="0.01" value="0.70" style="width:5rem"></label>
    <button id="exportScenario">Export YAML</button>
    <span class="metric" id="editorStatus">author off</span>
  </div>
  <canvas id="scene"></canvas>
</div>
<script type="importmap">
{
  "imports": {
    "three": "https://unpkg.com/three@0.160.0/build/three.module.js",
    "three/addons/": "https://unpkg.com/three@0.160.0/examples/jsm/"
  }
}
</script>
<script type="module">
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { browserSimSupport, runBrowserSimulation } from "./js/sim/browser_sim.js";

const data = await fetch("./viewer_data.json").then(r => r.json());
const canvas = document.querySelector("#scene");
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x111111);
const width = Number(data.metadata.layout_width || 20);
const height = Number(data.metadata.layout_height || 20);
const camera = new THREE.PerspectiveCamera(55, 1, 0.1, 2000);
camera.position.set(width * 0.55, Math.max(width, height) * 0.9, height * 1.1);
camera.lookAt(width / 2, 0, height / 2);
scene.add(new THREE.HemisphereLight(0xffffff, 0x202020, 2.2));
const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;
controls.dampingFactor = 0.08;
controls.enableRotate = false;
controls.target.set(width / 2, 0, height / 2);
controls.update();
const authorMode = document.querySelector("#authorMode");
const authorTool = document.querySelector("#authorTool");
const paintToken = document.querySelector("#paintToken");
const connectorType = document.querySelector("#connectorType");
const connectorToFloorSelect = document.querySelector("#connectorToFloor");
const connectorCapacity = document.querySelector("#connectorCapacity");
const hostileObjective = document.querySelector("#hostileObjective");
const hostileTarget = document.querySelector("#hostileTarget");
const hostileCredibility = document.querySelector("#hostileCredibility");
const dispatchMessageType = document.querySelector("#dispatchMessageType");
const dispatchRadius = document.querySelector("#dispatchRadius");
const dispatchCredibility = document.querySelector("#dispatchCredibility");
const dispatchHorizon = document.querySelector("#dispatchHorizon");
const dispatchStatus = document.querySelector("#dispatchStatus");
const projectDispatch = document.querySelector("#projectDispatch");
const commitDispatch = document.querySelector("#commitDispatch");
const editorStatus = document.querySelector("#editorStatus");
const validationOverlay = document.querySelector("#validationOverlay");
const runtimeFloors = normalizeRuntimeFloors(data.layout_floors || []);
const runtimeConnectors = normalizeRuntimeConnectors(data.layout_connectors || []);
const authoredHostileChannels = [];
const authoredDispatchMessages = [];
let dispatchTarget = null;
let lastDispatchProjection = null;
const activeFloorSelect = document.querySelector("#activeFloor");
const editorFloors = runtimeFloors.map(floor => ({ id: floor.id, z: floor.z, grid: floor.grid.map(row => row.slice()) }));
const editorGrid = editorFloors[0]?.grid || normalizeLayoutGrid(data.layout_grid && data.layout_grid.length ? data.layout_grid : layoutGridFromCells());
populateActiveFloorSelect();
window.chiyodaViewer = { camera, controls, renderer, scene, data, editorGrid, editorFloors, runtimeConnectors, authoredHostileChannels, authoredDispatchMessages, browserSimSupport, runBrowserSimulation };

const root = new THREE.Group();
scene.add(root);
const layoutGroup = new THREE.Group();
const agentGroup = new THREE.Group();
const hazardGroup = new THREE.Group();
const bottleneckGroup = new THREE.Group();
const pathUsageGroup = new THREE.Group();
const validationGroup = new THREE.Group();
const messageGroup = new THREE.Group();
const dispatchGroup = new THREE.Group();
const hostileGroup = new THREE.Group();
const connectorGroup = new THREE.Group();
const sourceFloorGroup = new THREE.Group();
scene.add(sourceFloorGroup, connectorGroup, hostileGroup, agentGroup, hazardGroup, bottleneckGroup, pathUsageGroup, validationGroup, messageGroup, dispatchGroup);

function box(x, z, sx, sz, color, y = 0.05, h = 0.1) {
  const geo = new THREE.BoxGeometry(sx, h, sz);
  const mat = new THREE.MeshStandardMaterial({ color });
  const mesh = new THREE.Mesh(geo, mat);
  mesh.position.set(x, y, z);
  return mesh;
}

function overlayBox(x, z, sx, sz, color, opacity, y = 0.08, h = 0.04) {
  const mesh = box(x, z, sx, sz, color, y, h);
  mesh.material.transparent = true;
  mesh.material.opacity = opacity;
  return mesh;
}

for (const floor of runtimeFloors) {
  root.add(box(width / 2, height / 2, width, height, 0x2b2b2b, floor.z - 0.04, 0.04));
}
root.add(layoutGroup);

function layoutGridFromCells() {
  const grid = Array.from({ length: height }, () => Array.from({ length: width }, () => "."));
  for (const cell of data.layout || []) {
    if (cell.y >= 0 && cell.y < height && cell.x >= 0 && cell.x < width) {
      grid[cell.y][cell.x] = String(cell.token || ".").slice(0, 1);
    }
  }
  return grid;
}

function normalizeLayoutGrid(rawGrid) {
  const grid = [];
  for (let y = 0; y < height; y += 1) {
    const source = rawGrid[y] || [];
    const row = Array.isArray(source) ? source.map(token => String(token || ".").slice(0, 1)) : String(source).split("");
    while (row.length < width) row.push(".");
    grid.push(row.slice(0, width));
  }
  return grid;
}

function normalizeRuntimeFloors(rawFloors) {
  if (!rawFloors.length) return [{ id: "0", z: 0, grid: normalizeLayoutGrid(data.layout_grid || []) }];
  return rawFloors.map((floor, index) => ({
    id: String(floor.id ?? index),
    z: Number(floor.z || 0),
    grid: normalizeLayoutGrid(floor.grid || []),
  }));
}

function normalizeRuntimeConnectors(rawConnectors) {
  return rawConnectors.map((connector, index) => ({
    id: String(connector.id ?? `connector_${index + 1}`),
    type: String(connector.type || "stairs"),
    from: normalizeConnectorEndpoint(connector.from || connector.from_cell),
    to: normalizeConnectorEndpoint(connector.to || connector.to_cell),
    bidirectional: connector.bidirectional !== false,
    width: Number(connector.width ?? 1),
    speed_multiplier: Number(connector.speed_multiplier ?? 1),
    capacity: connector.capacity == null ? null : Number(connector.capacity),
    dwell_s: Number(connector.dwell_s ?? 0),
    travel_s: Number(connector.travel_s ?? 0),
  })).filter(connector => connector.from && connector.to);
}

function normalizeConnectorEndpoint(value) {
  if (!value) return null;
  if (Array.isArray(value) && value.length >= 3) return { floor: String(value[0]), x: Number(value[1]), y: Number(value[2]) };
  return { floor: String(value.floor), x: Number(value.x), y: Number(value.y) };
}

function populateActiveFloorSelect() {
  activeFloorSelect.innerHTML = "";
  connectorToFloorSelect.innerHTML = "";
  editorFloors.forEach((floor, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = `${floor.id} z=${floor.z}`;
    activeFloorSelect.appendChild(option);
    const connectorOption = option.cloneNode(true);
    connectorOption.value = floor.id;
    connectorToFloorSelect.appendChild(connectorOption);
  });
  if (editorFloors.length > 1) connectorToFloorSelect.value = editorFloors[1].id;
}

function activeFloorIndex() {
  return Math.max(0, Math.min(editorFloors.length - 1, Number(activeFloorSelect.value || 0)));
}

function activeFloor() {
  return editorFloors[activeFloorIndex()] || editorFloors[0];
}

function activeEditorGrid() {
  return activeFloor().grid;
}

function floorZ(floorId) {
  const floor = runtimeFloors.find(item => item.id === String(floorId));
  return floor ? floor.z : 0;
}

function cellColor(token) {
  if (token === "X") return 0x656565;
  if (token === "E") return 0x3eb36f;
  if (token === "@") return 0x4f83ff;
  if (token === "R") return 0x39c3a0;
  if (token === "S") return 0xd4b24c;
  return 0x252525;
}

function cellHeight(token) {
  if (token === "X") return 0.9;
  if (token === ".") return 0.025;
  return 0.12;
}

function renderLayoutGrid() {
  layoutGroup.clear();
  const showFloorCells = authorMode.checked;
  for (const floor of editorFloors) {
    const grid = floor.grid;
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const token = grid[y][x] || ".";
        if (token === "." && !showFloorCells) continue;
        const h = cellHeight(token);
        const mesh = box(x + 0.5, y + 0.5, 0.95, 0.95, cellColor(token), floor.z + h / 2, h);
        if (token === ".") {
          mesh.material.transparent = true;
          mesh.material.opacity = 0.28;
        }
        layoutGroup.add(mesh);
      }
    }
  }
}
renderLayoutGrid();

function roleColor(role) {
  if (role === "wall") return 0x777777;
  if (role === "exit") return 0x39b36b;
  if (role === "spawn") return 0x4f83ff;
  if (role === "platform") return 0x8ec4ff;
  if (role === "pathway") return 0xd8b545;
  if (role === "corridor" || role === "area" || role === "room") return 0x5f6f7a;
  return 0x708090;
}

function polygonMesh(rings, y, color, opacity = 0.24) {
  if (!rings.length || rings[0].length < 3) return null;
  const contour = rings[0].map(p => new THREE.Vector2(Number(p[0]), Number(p[1])));
  const triangles = THREE.ShapeUtils.triangulateShape(contour, []);
  const positions = [];
  for (const tri of triangles) {
    for (const idx of tri) {
      const p = contour[idx];
      positions.push(p.x, y, p.y);
    }
  }
  const geo = new THREE.BufferGeometry();
  geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geo.computeVertexNormals();
  return new THREE.Mesh(
    geo,
    new THREE.MeshStandardMaterial({ color, transparent: true, opacity, side: THREE.DoubleSide })
  );
}

function lineObject(points, y, color) {
  const geo = new THREE.BufferGeometry().setFromPoints(points.map(p => new THREE.Vector3(Number(p[0]), y + 0.05, Number(p[1]))));
  return new THREE.Line(geo, new THREE.LineBasicMaterial({ color, linewidth: 2 }));
}

function connectorColor(type) {
  if (type === "elevator") return 0x43d9ff;
  if (type === "ramp") return 0x8fd16a;
  if (type === "escalator") return 0xd4b24c;
  return 0xffffff;
}

function renderConnectors() {
  connectorGroup.clear();
  for (const connector of runtimeConnectors) {
    const from = connector.from;
    const to = connector.to;
    const color = connectorColor(connector.type);
    const points = [
      new THREE.Vector3(from.x + 0.5, floorZ(from.floor) + 0.7, from.y + 0.5),
      new THREE.Vector3(to.x + 0.5, floorZ(to.floor) + 0.7, to.y + 0.5),
    ];
    connectorGroup.add(new THREE.Line(
      new THREE.BufferGeometry().setFromPoints(points),
      new THREE.LineBasicMaterial({ color })
    ));
    connectorGroup.add(overlayBox(from.x + 0.5, from.y + 0.5, 0.55, 0.55, color, 0.6, floorZ(from.floor) + 0.2, 0.16));
    connectorGroup.add(overlayBox(to.x + 0.5, to.y + 0.5, 0.55, 0.55, color, 0.6, floorZ(to.floor) + 0.2, 0.16));
  }
}
renderConnectors();

function renderHostileChannels() {
  hostileGroup.clear();
  for (const channel of authoredHostileChannels) {
    const color = channel.objective === "threat-amplification" ? 0xe05d44 : 0xc44cff;
    hostileGroup.add(overlayBox(channel.x + 0.5, channel.y + 0.5, 0.7, 0.7, color, 0.68, floorZ(channel.floor) + 0.35, 0.22));
  }
}

function addAuthoredConnector(from, to) {
  if (!from || !to) return null;
  if (from.floor === to.floor && from.x === to.x && from.y === to.y) return null;
  const connector = {
    id: `viewer_connector_${runtimeConnectors.length + 1}`,
    type: String(connectorType.value || "stairs"),
    from,
    to,
    bidirectional: true,
    width: 1,
    speed_multiplier: 1,
    capacity: Math.max(1, Math.round(Number(connectorCapacity.value || 30))),
    dwell_s: 0,
    travel_s: 0,
  };
  runtimeConnectors.push(connector);
  renderConnectors();
  const result = renderValidationOverlay();
  updateEditorStatus(`connector ${connector.type} ${from.floor}:${from.x},${from.y} -> ${to.floor}:${to.x},${to.y} | ${validationSummary(result)}`);
  return connector;
}

function addHostileChannel(cell) {
  const objective = String(hostileObjective.value || "false-protective-action");
  const target = String(hostileTarget.value || "baseline").trim() || "baseline";
  const plausibility = Math.max(0.01, Math.min(0.99, Number(hostileCredibility.value || 0.7)));
  const channel = {
    id: `viewer_hostile_${authoredHostileChannels.length + 1}`,
    channel_type: "gossip",
    objective,
    budget: 1,
    start_step: 0,
    interval_steps: 5,
    plausibility,
    radius: 6,
    source_id: `viewer_hostile_${authoredHostileChannels.length + 1}`,
    target_cohort: target,
    floor: activeFloor().id,
    x: cell.x,
    y: cell.y,
  };
  authoredHostileChannels.push(channel);
  renderHostileChannels();
  updateEditorStatus(`hostile ${objective} ${channel.floor}:${channel.x},${channel.y}`);
  return channel;
}

function hostileClaimKey(channel) {
  return channel.objective === "threat-amplification" ? "claimed_hazard" : "claimed_exit";
}

function setDispatchTarget(cell) {
  dispatchTarget = { floor: activeFloor().id, x: cell.x, y: cell.y, z: activeFloor().z };
  commitDispatch.disabled = true;
  updateDispatchStatus(`target ${dispatchTarget.floor}:${cell.x},${cell.y}`);
}

function currentDispatchTarget() {
  if (dispatchTarget) return dispatchTarget;
  const floor = activeFloor();
  return { floor: floor.id, x: Math.floor(width / 2), y: Math.floor(height / 2), z: floor.z };
}

function projectDispatchMessage() {
  const started = performance.now();
  const frame = playbackFrames[frameIndex] || { agents: [] };
  const agents = frame.agents || [];
  const target = currentDispatchTarget();
  const radius = Math.max(0.1, Number(dispatchRadius.value || 8));
  const credibility = Math.max(0, Math.min(1, Number(dispatchCredibility.value || 0.9)));
  const horizonSteps = Math.max(1, Math.round(Number(dispatchHorizon.value || 30)));
  const horizonScale = Math.min(1, horizonSteps / 60);
  const recipients = agents.filter(agent => distance3(agentPoint(agent), [target.x + 0.5, target.y + 0.5, target.z]) <= radius);
  const pressure = recipients.map(agent => hazardPressure(agent, data.hazards || []));
  const meanPressure = pressure.length ? pressure.reduce((sum, value) => sum + value, 0) / pressure.length : 0;
  const recipientShare = recipients.length / Math.max(1, agents.length);
  const uncertainty = recipients.length ? recipients.reduce((sum, agent) => sum + Number(agent.entropy || 0), 0) / recipients.length : 0;
  const coeffs = dispatchCoefficients(dispatchMessageType.value, (data.hazards || []).length > 0);
  const effect = credibility * horizonScale * recipientShare;
  const result = {
    target,
    message_type: dispatchMessageType.value,
    recipients: recipients.length,
    affected_agents: recipients.length,
    mean_belief_delta: round6(coeffs.belief * credibility * horizonScale * (0.5 + uncertainty)),
    harmful_convergence_delta: round6(coeffs.hci * effect * (1 + meanPressure)),
    exposure_delta: round6(coeffs.exposure * effect * (0.25 + meanPressure)),
    latency_ms: Math.round((performance.now() - started) * 1000) / 1000,
    horizon_steps: horizonSteps,
  };
  lastDispatchProjection = result;
  commitDispatch.disabled = false;
  updateDispatchStatus(`recipients ${result.recipients} | belief ${signed(result.mean_belief_delta)} | HCI ${signed(result.harmful_convergence_delta)} | exposure ${signed(result.exposure_delta)} | ${result.latency_ms.toFixed(2)}ms`);
  return result;
}

function commitDispatchMarker() {
  if (!lastDispatchProjection) return null;
  const frame = playbackFrames[frameIndex] || { step: 0 };
  const message = { ...lastDispatchProjection, step: Number(frame.step || 0) };
  authoredDispatchMessages.push(message);
  renderDispatchMarkers();
  commitDispatch.disabled = true;
  updateDispatchStatus(`committed ${message.message_type} @ ${message.target.floor}:${message.target.x},${message.target.y}`);
  return message;
}

function renderDispatchMarkers() {
  dispatchGroup.clear();
  for (const message of authoredDispatchMessages) {
    const target = message.target || {};
    const color = message.exposure_delta <= 0 ? 0x56c7ff : 0xff8a4c;
    const marker = new THREE.Mesh(
      new THREE.SphereGeometry(0.28, 16, 12),
      new THREE.MeshStandardMaterial({ color, emissive: 0x123344 })
    );
    marker.position.set(Number(target.x || 0) + 0.5, floorZ(target.floor || "0") + 0.72, Number(target.y || 0) + 0.5);
    dispatchGroup.add(marker);
  }
}

function dispatchCoefficients(messageType, hasHazard) {
  if (messageType === "avoid_hazard") return { belief: 0.14, hci: -0.06, exposure: -0.22 };
  if (messageType === "shelter_hold") return { belief: 0.05, hci: 0.05, exposure: 0.10 };
  if (messageType === "all_clear") return { belief: 0.08, hci: hasHazard ? 0.02 : -0.02, exposure: hasHazard ? 0.08 : -0.03 };
  return { belief: 0.18, hci: -0.10, exposure: -0.12 };
}

function hazardPressure(agent, hazards) {
  const point = agentPoint(agent);
  let pressure = 0;
  for (const hazard of hazards) {
    const radius = Math.max(0.1, Number(hazard.radius || 1));
    const severity = Number(hazard.severity || 1);
    const center = [Number(hazard.x || 0), Number(hazard.y || 0), Number(hazard.z || 0)];
    pressure = Math.max(pressure, Math.max(0, 1 - (distance3(point, center) / radius)) * severity);
  }
  return pressure;
}

function agentPoint(agent) {
  return [Number(agent.x || 0), Number(agent.y || 0), Number(agent.z || 0)];
}

function distance3(a, b) {
  const dx = a[0] - b[0];
  const dy = a[1] - b[1];
  const dz = a[2] - b[2];
  return Math.sqrt(dx * dx + dy * dy + dz * dz);
}

function round6(value) {
  return Math.round(Number(value || 0) * 1000000) / 1000000;
}

function signed(value) {
  return value >= 0 ? `+${value}` : String(value);
}

function updateDispatchStatus(text) {
  dispatchStatus.textContent = text;
}

function renderSourceFloors() {
  sourceFloorGroup.clear();
  const floors = data.floors || [];
  const gap = Number(document.querySelector("#floorGap").value || 0);
  floors.forEach((floor, index) => {
    const y = gap > 0 ? index * gap : 0;
    const group = new THREE.Group();
    group.userData.level = floor.level;
    for (const feature of floor.features || []) {
      const color = roleColor(feature.role);
      const geometry = feature.geometry || {};
      if (geometry.type === "Polygon") {
        const mesh = polygonMesh(geometry.coordinates || [], y, color);
        if (mesh) group.add(mesh);
      } else if (geometry.type === "MultiPolygon") {
        for (const polygon of geometry.coordinates || []) {
          const mesh = polygonMesh(polygon, y, color);
          if (mesh) group.add(mesh);
        }
      } else if (geometry.type === "LineString") {
        group.add(lineObject(geometry.coordinates || [], y, color));
      } else if (geometry.type === "MultiLineString") {
        for (const line of geometry.coordinates || []) group.add(lineObject(line, y, color));
      } else if (geometry.type === "Point") {
        const p = geometry.coordinates || [0, 0];
        group.add(box(Number(p[0]), Number(p[1]), 0.35, 0.35, color, y + 0.18, 0.35));
      }
    }
    if (floors.length > 1) {
      const label = box(-0.6, 0.5 + index * 1.2, 0.25, 0.9, 0xffffff, y + 0.08, 0.16);
      group.add(label);
    }
    sourceFloorGroup.add(group);
  });
}
renderSourceFloors();

for (const hz of data.hazards) {
  const mesh = new THREE.Mesh(
    new THREE.CylinderGeometry(Math.max(0.2, Number(hz.radius || 0.2)), Math.max(0.2, Number(hz.radius || 0.2)), 0.08, 32),
    new THREE.MeshStandardMaterial({ color: 0xe05d44, transparent: true, opacity: 0.45 })
  );
  mesh.position.set(Number(hz.x || 0), Number(hz.z || 0) + 0.08, Number(hz.y || 0));
  hazardGroup.add(mesh);
}

for (const zone of data.bottlenecks || []) {
  for (const cell of zone.cells || []) {
    const offset = typeof cell[0] === "string" ? 1 : 0;
    const z = typeof cell[0] === "string" ? floorZ(cell[0]) : 0;
    bottleneckGroup.add(box(Number(cell[offset]) + 0.5, Number(cell[offset + 1]) + 0.5, 0.9, 0.9, 0xf0c84b, z + 0.03, 0.06));
  }
}

function renderPathUsage() {
  pathUsageGroup.clear();
  const cells = data.path_usage || [];
  const maxUsage = Math.max(1, ...cells.map(cell => Number(cell.path_usage || 0)));
  for (const cell of cells) {
    const usage = Number(cell.path_usage || 0);
    if (usage <= 0) continue;
    const strength = Math.min(1, usage / maxUsage);
    pathUsageGroup.add(overlayBox(
      Number(cell.x) + 0.5,
      Number(cell.y) + 0.5,
      0.9,
      0.9,
      0x56c7ff,
      0.18 + strength * 0.45,
      Number(cell.z || floorZ(cell.floor_id || "0")) + 0.11,
      0.05
    ));
  }
}
renderPathUsage();
pathUsageGroup.visible = false;

for (const event of data.interventions || []) {
  const mesh = new THREE.Mesh(
    new THREE.SphereGeometry(0.22, 16, 12),
    new THREE.MeshStandardMaterial({ color: 0x56c7ff, emissive: 0x123344 })
  );
  mesh.position.set(Number(event.target_x || 0), Number(event.target_z || 0) + 0.55, Number(event.target_y || 0));
  messageGroup.add(mesh);
}

const frames = data.frames || [];
let playbackFrames = frames;
const scrub = document.querySelector("#scrub");
const stepLabel = document.querySelector("#step");
const simStatus = document.querySelector("#simStatus");
scrub.max = Math.max(0, playbackFrames.length - 1);
let frameIndex = 0;
let playing = false;

function drawFrame(index) {
  agentGroup.clear();
  const frame = playbackFrames[index] || { step: 0, agents: [] };
  for (const agent of frame.agents) {
    const entropy = Math.max(0, Math.min(1, Number(agent.entropy || 0)));
    const color = new THREE.Color().setHSL(0.58 - entropy * 0.45, 0.75, 0.55);
    const mesh = new THREE.Mesh(
      new THREE.SphereGeometry(0.18, 16, 12),
      new THREE.MeshStandardMaterial({ color })
    );
    mesh.position.set(Number(agent.x), Number(agent.z || 0) + 0.32, Number(agent.y));
    agentGroup.add(mesh);
  }
  stepLabel.textContent = `step ${frame.step} | agents ${frame.agents.length}`;
}

function setPlaybackFrames(nextFrames, statusText) {
  playbackFrames = nextFrames && nextFrames.length ? nextFrames : [];
  frameIndex = 0;
  scrub.max = Math.max(0, playbackFrames.length - 1);
  scrub.value = "0";
  drawFrame(0);
  simStatus.textContent = statusText;
}

document.querySelector("#play").addEventListener("click", event => {
  playing = !playing;
  event.target.textContent = playing ? "Pause" : "Play";
});
scrub.addEventListener("input", () => {
  frameIndex = Number(scrub.value);
  drawFrame(frameIndex);
});
document.querySelector("#hazards").addEventListener("change", event => hazardGroup.visible = event.target.checked);
document.querySelector("#bottlenecks").addEventListener("change", event => bottleneckGroup.visible = event.target.checked);
document.querySelector("#pathUsage").addEventListener("change", event => pathUsageGroup.visible = event.target.checked);
validationOverlay.addEventListener("change", renderValidationOverlay);
document.querySelector("#messages").addEventListener("change", event => {
  messageGroup.visible = event.target.checked;
  hostileGroup.visible = event.target.checked;
  dispatchGroup.visible = event.target.checked;
});
document.querySelector("#connectors").addEventListener("change", event => connectorGroup.visible = event.target.checked);
document.querySelector("#sourceFloors").addEventListener("change", event => sourceFloorGroup.visible = event.target.checked);
document.querySelector("#floorGap").addEventListener("input", renderSourceFloors);
activeFloorSelect.addEventListener("change", () => {
  renderValidationOverlay();
  updateEditorStatus(`floor ${activeFloor().id} | ${authorActionLabel()} | ${validationSummary(validateEditorGrid())}`);
});
document.querySelector("#resetCamera").addEventListener("click", () => {
  camera.position.set(width * 0.55, Math.max(width, height) * 0.9, height * 1.1);
  controls.target.set(width / 2, 0, height / 2);
  controls.update();
});
document.querySelector("#browserSim").addEventListener("click", () => {
  const result = runBrowserSimulation(data, { durationS: 60, targetStepsPerSecond: 10, maxAgents: 200 });
  window.chiyodaViewer.browserSimResult = result;
  if (!result.ok) {
    simStatus.textContent = result.reason;
    return;
  }
  const summary = result.summary;
  setPlaybackFrames(result.frames, `browser ${summary.evacuated}/${summary.initial_agents} | ${summary.sim_steps_per_second.toFixed(0)} steps/s`);
});
document.querySelector("#resetReplay").addEventListener("click", () => {
  setPlaybackFrames(frames, "replay");
});
projectDispatch.addEventListener("click", () => {
  playing = false;
  document.querySelector("#play").textContent = "Play";
  projectDispatchMessage();
});
commitDispatch.addEventListener("click", commitDispatchMarker);
const browserSupport = browserSimSupport(data, { maxAgents: 200 });
if (!browserSupport.ok) {
  document.querySelector("#browserSim").disabled = true;
  simStatus.textContent = browserSupport.reason;
}
authorMode.addEventListener("change", () => {
  if (authorMode.checked) {
    playing = false;
    document.querySelector("#play").textContent = "Play";
  }
  renderLayoutGrid();
  const result = renderValidationOverlay();
  updateEditorStatus(authorMode.checked ? `author ${authorActionLabel()} | ${validationSummary(result)}` : validationSummary(result));
});
authorTool.addEventListener("change", () => {
  if (authorMode.checked) updateEditorStatus(`author ${authorTool.value}`);
});
paintToken.addEventListener("change", () => {
  if (authorMode.checked) updateEditorStatus(`author ${tokenLabel(paintToken.value)} | ${validationSummary(validateEditorGrid())}`);
});
document.querySelector("#exportScenario").addEventListener("click", downloadScenario);
window.chiyodaViewer.exportScenarioYaml = exportScenarioYaml;
window.chiyodaViewer.renderLayoutGrid = renderLayoutGrid;
window.chiyodaViewer.validateEditorGrid = validateEditorGrid;
window.chiyodaViewer.renderValidationOverlay = renderValidationOverlay;
window.chiyodaViewer.addAuthoredConnector = addAuthoredConnector;
window.chiyodaViewer.addHostileChannel = addHostileChannel;
window.chiyodaViewer.projectDispatchMessage = projectDispatchMessage;
window.chiyodaViewer.commitDispatchMarker = commitDispatchMarker;
window.chiyodaViewer.setDispatchTarget = setDispatchTarget;

function tokenLabel(token) {
  if (token === ".") return "floor";
  if (token === "X") return "wall";
  if (token === "E") return "exit";
  if (token === "@") return "spawn";
  if (token === "S") return "signage";
  if (token === "R") return "responder";
  return token;
}

function authorActionLabel() {
  if (authorTool.value === "connector") return `${connectorType.value} -> ${connectorToFloorSelect.value}`;
  if (authorTool.value === "hostile") return `${hostileObjective.value} -> ${hostileTarget.value}`;
  if (authorTool.value === "dispatch") return `${dispatchMessageType.value} target`;
  return tokenLabel(paintToken.value);
}

function updateEditorStatus(text) {
  editorStatus.textContent = text;
}

function layoutText() {
  return floorText(activeEditorGrid());
}

function floorText(grid) {
  return grid.map(row => row.join("")).join("\\n");
}

function exportedFloors() {
  return editorFloors.map((floor, index) => ({
    id: String(floor.id ?? index),
    z: Number(floor.z ?? index * 3),
    grid: floor.grid,
  }));
}

function collectTokenCells(token) {
  const cells = [];
  const grid = activeEditorGrid();
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (grid[y][x] === token) cells.push([x, y]);
    }
  }
  return cells;
}

function collectFloorTokenCells(token) {
  const cells = [];
  for (const floor of exportedFloors()) {
    for (let y = 0; y < floor.grid.length; y += 1) {
      for (let x = 0; x < floor.grid[y].length; x += 1) {
        if (floor.grid[y][x] === token) cells.push({ floor_id: floor.id, x, y });
      }
    }
  }
  return cells;
}

function cellKey(floor, x, y) {
  return `${floor},${x},${y}`;
}

function floorById(floorId) {
  return editorFloors.find(floor => floor.id === String(floorId));
}

function inBounds(floorId, x, y) {
  const floor = floorById(floorId);
  return !!floor && y >= 0 && y < floor.grid.length && x >= 0 && x < floor.grid[y].length;
}

function isWalkable(floorId, x, y) {
  const floor = floorById(floorId);
  return inBounds(floorId, x, y) && floor.grid[y][x] !== "X";
}

function connectorEdges() {
  const edges = [];
  for (const connector of runtimeConnectors) {
    edges.push([connector.from, connector.to]);
    if (connector.bidirectional) edges.push([connector.to, connector.from]);
  }
  return edges;
}

function neighbors(floorId, x, y) {
  const cells = [
    { floor: floorId, x: x + 1, y },
    { floor: floorId, x: x - 1, y },
    { floor: floorId, x, y: y + 1 },
    { floor: floorId, x, y: y - 1 },
  ].filter(cell => isWalkable(cell.floor, cell.x, cell.y));
  for (const [source, target] of connectorEdges()) {
    if (source.floor === String(floorId) && source.x === x && source.y === y && isWalkable(target.floor, target.x, target.y)) {
      cells.push(target);
    }
  }
  return cells;
}

function validateEditorGrid() {
  const exits = collectFloorTokenCells("E").map(cell => ({ floor: cell.floor_id, x: cell.x, y: cell.y }));
  const starts = [
    ...collectFloorTokenCells("@").map(cell => ({ kind: "spawn", label: "layout spawn", source: "layout.@", cell: { floor: cell.floor_id, x: cell.x, y: cell.y } })),
    ...collectFloorTokenCells("R").map(cell => ({ kind: "responder", label: "layout responder entry", source: "layout.R", cell: { floor: cell.floor_id, x: cell.x, y: cell.y } })),
  ];
  const issues = [];
  const reachable = new Set();
  const parent = new Map();
  const queue = [];
  for (const exit of exits) {
    if (!isWalkable(exit.floor, exit.x, exit.y)) continue;
    const key = cellKey(exit.floor, exit.x, exit.y);
    reachable.add(key);
    parent.set(key, null);
    queue.push(exit);
  }
  while (queue.length) {
    const cell = queue.shift();
    for (const next of neighbors(cell.floor, cell.x, cell.y)) {
      const key = cellKey(next.floor, next.x, next.y);
      if (reachable.has(key)) continue;
      reachable.add(key);
      parent.set(key, cell);
      queue.push(next);
    }
  }
  if (!exits.length) {
    issues.push({ severity: "error", code: "no_exits", message: "layout has no exit cells" });
  }
  if (Number(frames[0]?.agents?.length || 0) > 0 && !collectFloorTokenCells("@").length) {
    issues.push({ severity: "warning", code: "implicit_population_spawn", message: "no @ spawn cells; exported run will use random walkable cells" });
  }
  for (const start of starts) {
    const { floor, x, y } = start.cell;
    if (!isWalkable(floor, x, y)) {
      issues.push({ severity: "error", code: "start_on_wall", message: `${start.label} is on a wall cell`, cell: start.cell, source: start.source });
      continue;
    }
    if (exits.length && !reachable.has(cellKey(floor, x, y))) {
      issues.push({ severity: "error", code: "start_unreachable", message: `${start.label} cannot reach any exit`, cell: start.cell, source: start.source });
    }
    if (floorById(floor).grid[y][x] === "E") {
      issues.push({ severity: "warning", code: "start_on_exit", message: `${start.label} is already on an exit cell`, cell: start.cell, source: start.source });
    }
  }
  const walkableCells = [];
  const unreachableCells = [];
  for (const floor of editorFloors) {
    for (let y = 0; y < floor.grid.length; y += 1) {
      for (let x = 0; x < floor.grid[y].length; x += 1) {
        if (!isWalkable(floor.id, x, y)) continue;
        const cell = { floor: floor.id, x, y };
        walkableCells.push(cell);
        if (exits.length && !reachable.has(cellKey(floor.id, x, y))) unreachableCells.push(cell);
      }
    }
  }
  if (!walkableCells.length) {
    issues.push({ severity: "error", code: "no_walkable_cells", message: "layout has no walkable cells" });
  }
  if (exits.length && unreachableCells.length) {
    issues.push({ severity: "warning", code: "unreachable_walkable_cells", message: `${unreachableCells.length} walkable cells cannot reach any exit` });
  }
  const paths = {};
  starts.forEach((start, index) => {
    const { floor, x, y } = start.cell;
    const key = cellKey(floor, x, y);
    if (!reachable.has(key)) return;
    paths[`${start.kind}_${index}`] = pathToExit(start.cell, parent);
  });
  return {
    ok: !issues.some(issue => issue.severity === "error"),
    exits: exits.map(cell => [cell.floor, cell.x, cell.y]),
    starts,
    reachableCells: Array.from(reachable).map(parseCellKey),
    unreachableWalkableCells: unreachableCells,
    paths,
    issues,
  };
}

function parseCellKey(key) {
  const [floor, x, y] = key.split(",");
  return [floor, Number(x), Number(y)];
}

function pathToExit(start, parent) {
  const path = [start];
  let current = start;
  while (parent.get(cellKey(current.floor, current.x, current.y))) {
    current = parent.get(cellKey(current.floor, current.x, current.y));
    path.push(current);
  }
  return path.map(cell => [cell.floor, cell.x, cell.y]);
}

function validationSummary(result) {
  const errors = result.issues.filter(issue => issue.severity === "error").length;
  const warnings = result.issues.filter(issue => issue.severity === "warning").length;
  if (errors) return `errors ${errors} warnings ${warnings}`;
  if (warnings) return `warnings ${warnings}`;
  return "valid";
}

function renderValidationOverlay() {
  validationGroup.clear();
  const result = validateEditorGrid();
  window.chiyodaViewer.validation = result;
  validationGroup.visible = validationOverlay.checked;
  if (validationOverlay.checked) {
    const seenPathCells = new Set();
    for (const path of Object.values(result.paths)) {
      for (const [floor, x, y] of path) {
        const key = cellKey(floor, x, y);
        if (seenPathCells.has(key)) continue;
        seenPathCells.add(key);
        validationGroup.add(overlayBox(x + 0.5, y + 0.5, 0.52, 0.52, 0x46d983, 0.55, floorZ(floor) + 0.16, 0.05));
      }
    }
    for (const cell of result.unreachableWalkableCells) {
      validationGroup.add(overlayBox(cell.x + 0.5, cell.y + 0.5, 0.82, 0.82, 0xc44cff, 0.42, floorZ(cell.floor) + 0.14, 0.05));
    }
    for (const issue of result.issues) {
      if (issue.severity !== "error" || !issue.cell) continue;
      const cell = issue.cell;
      validationGroup.add(overlayBox(cell.x + 0.5, cell.y + 0.5, 0.95, 0.95, 0xff4f4f, 0.7, floorZ(cell.floor) + 0.22, 0.08));
    }
  }
  return result;
}

function yamlQuote(value) {
  return JSON.stringify(String(value ?? ""));
}

function yamlNumber(value, fallback = 0) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return String(fallback);
  return String(Math.round(parsed * 10000) / 10000);
}

function initialHazards() {
  const rows = (data.hazards || []).filter(row => Number.isFinite(Number(row.x)) && Number.isFinite(Number(row.y)));
  if (!rows.length) return [];
  const firstStep = Math.min(...rows.map(row => Number(row.step ?? 0)).filter(Number.isFinite));
  const seen = new Set();
  const hazards = [];
  for (const row of rows) {
    const step = Number(row.step ?? 0);
    if (Number.isFinite(firstStep) && step !== firstStep) continue;
    const key = [row.kind || "GAS", yamlNumber(row.x), yamlNumber(row.y), yamlNumber(row.z), yamlNumber(row.radius), yamlNumber(row.severity)].join("|");
    if (seen.has(key)) continue;
    seen.add(key);
    hazards.push(row);
  }
  return hazards;
}

function scenarioExportSections() {
  const scenarioName = `${String(data.metadata.scenario_name || "chiyoda_viewer")}_edited`;
  const spawns = collectFloorTokenCells("@");
  const responders = collectFloorTokenCells("R");
  const floors = exportedFloors();
  const populationTotal = Math.max(spawns.length, Number(frames[0]?.agents?.length || 0), 1);
  const lastStep = Number(frames[frames.length - 1]?.step || 0);
  const maxSteps = Math.max(1, Math.ceil(lastStep || 400));
  const sections = [
    { key: "name", scalar: yamlQuote(scenarioName) },
    { key: "description", scalar: yamlQuote("Edited from Chiyoda static viewer export.") },
  ];
  const layoutLines = [
    `cell_size: ${yamlNumber(data.metadata.layout_cell_size || 1, 1)}`,
    "floors:",
  ];
  for (const floor of floors) {
    layoutLines.push(
      `  - id: ${yamlQuote(floor.id)}`,
      `    z: ${yamlNumber(floor.z)}`,
      "    text: |"
    );
    for (const line of floorText(floor.grid).split("\\n")) layoutLines.push(`      ${line}`);
  }
  if (runtimeConnectors.length) {
    layoutLines.push("connectors:");
    for (const connector of runtimeConnectors) {
      layoutLines.push(
        `  - id: ${yamlQuote(connector.id)}`,
        `    type: ${yamlQuote(connector.type)}`,
        `    from: {floor: ${yamlQuote(connector.from.floor)}, x: ${connector.from.x}, y: ${connector.from.y}}`,
        `    to: {floor: ${yamlQuote(connector.to.floor)}, x: ${connector.to.x}, y: ${connector.to.y}}`,
        `    bidirectional: ${connector.bidirectional ? "true" : "false"}`,
        `    width: ${yamlNumber(connector.width, 1)}`,
        `    speed_multiplier: ${yamlNumber(connector.speed_multiplier, 1)}`
      );
      if (connector.capacity !== null) layoutLines.push(`    capacity: ${Math.max(1, Math.round(connector.capacity))}`);
      if (connector.dwell_s) layoutLines.push(`    dwell_s: ${yamlNumber(connector.dwell_s)}`);
      if (connector.travel_s) layoutLines.push(`    travel_s: ${yamlNumber(connector.travel_s)}`);
    }
  }
  sections.push({ key: "layout", lines: layoutLines });
  const populationLines = [`total: ${populationTotal}`];
  if (spawns.length) {
    populationLines.push("cohorts:", "  - name: baseline", `    count: ${populationTotal}`, "    spawn_cells:");
    for (const cell of spawns) populationLines.push(`      - {floor: ${yamlQuote(cell.floor_id)}, x: ${cell.x}, y: ${cell.y}}`);
  }
  sections.push({ key: "population", lines: populationLines });
  if (responders.length) {
    const responderLines = [`- count: ${responders.length}`, "  spawn_cells:"];
    for (const cell of responders) responderLines.push(`    - {floor: ${yamlQuote(cell.floor_id)}, x: ${cell.x}, y: ${cell.y}}`);
    sections.push({ key: "responders", lines: responderLines });
  }
  const hazards = initialHazards();
  if (hazards.length) {
    const hazardLines = [];
    for (const hazard of hazards) {
      hazardLines.push(
        `- type: ${yamlQuote(hazard.kind || "GAS")}`,
        `  location: [${yamlNumber(hazard.x)}, ${yamlNumber(hazard.y)}, ${yamlNumber(hazard.z)}]`,
        `  radius: ${yamlNumber(hazard.radius)}`,
        `  severity: ${yamlNumber(hazard.severity)}`
      );
    }
    sections.push({ key: "hazards", lines: hazardLines });
  }
  if (authoredHostileChannels.length) {
    const hostileLines = [];
    for (const channel of authoredHostileChannels) {
      const claimKey = hostileClaimKey(channel);
      hostileLines.push(
        `- id: ${yamlQuote(channel.id)}`,
        `  channel_type: ${yamlQuote(channel.channel_type)}`,
        `  objective: ${yamlQuote(channel.objective)}`,
        `  budget: ${Math.max(1, Math.round(channel.budget))}`,
        `  start_step: ${Math.max(0, Math.round(channel.start_step))}`,
        `  interval_steps: ${Math.max(1, Math.round(channel.interval_steps))}`,
        `  plausibility: ${yamlNumber(channel.plausibility, 0.7)}`,
        `  radius: ${yamlNumber(channel.radius, 6)}`,
        `  source_id: ${yamlQuote(channel.source_id)}`,
        `  target_cohort: ${yamlQuote(channel.target_cohort)}`,
        `  ${claimKey}:`,
        `    floor: ${yamlQuote(channel.floor)}`,
        `    x: ${channel.x}`,
        `    y: ${channel.y}`
      );
    }
    sections.push({ key: "hostile_channels", lines: hostileLines });
  }
  sections.push({ key: "simulation", lines: [`max_steps: ${maxSteps}`, "dt: 0.1", "random_seed: 42"] });
  sections.push({ key: "information", lines: ["mode: asymmetric"] });
  return sections;
}

function sourceScenarioKeys() {
  const source = data.source_scenario || {};
  return source && typeof source === "object" && !Array.isArray(source) ? Object.keys(source) : [];
}

function jsonPointerKey(key) {
  return String(key).replace(/~/g, "~0").replace(/\\//g, "~1");
}

function pushIndented(target, source, spaces) {
  const pad = " ".repeat(spaces);
  for (const line of source) target.push(`${pad}${line}`);
}

function pushSection(target, section, spaces) {
  const pad = " ".repeat(spaces);
  if (section.scalar !== undefined) {
    target.push(`${pad}${section.key}: ${section.scalar}`);
    return;
  }
  target.push(`${pad}${section.key}:`);
  pushIndented(target, section.lines, spaces + 2);
}

function pushPatchValue(target, section) {
  if (section.scalar !== undefined) {
    target.push(`      value: ${section.scalar}`);
    return;
  }
  target.push("      value:");
  pushIndented(target, section.lines, 8);
}

function exportScenarioYaml() {
  const origin = data.origin || {};
  const sections = scenarioExportSections();
  const exportedKeys = new Set(sections.map(section => section.key));
  const sourceKeys = sourceScenarioKeys();
  const lines = [
    "origin:",
    `  path: ${yamlQuote(origin.path || "")}`,
    `  sha256: ${yamlQuote(origin.sha256 || "")}`,
    "patch:",
    '  format: "RFC6902"',
    "  ops:",
  ];
  for (const key of sourceKeys) {
    if (exportedKeys.has(key)) continue;
    lines.push(
      '    - op: "remove"',
      `      path: ${yamlQuote(`/${jsonPointerKey(key)}`)}`
    );
  }
  for (const section of sections) {
    const op = sourceKeys.includes(section.key) ? "replace" : "add";
    lines.push(
      `    - op: ${yamlQuote(op)}`,
      `      path: ${yamlQuote(`/${jsonPointerKey(section.key)}`)}`
    );
    pushPatchValue(lines, section);
  }
  lines.push("scenario:");
  for (const section of sections) pushSection(lines, section, 2);
  lines.push("");
  return lines.join("\\n");
}

function downloadScenario() {
  const result = renderValidationOverlay();
  const yaml = exportScenarioYaml();
  const blob = new Blob([yaml], { type: "text/yaml" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "chiyoda_edited_scenario.yaml";
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
  updateEditorStatus(`exported YAML | ${validationSummary(result)}`);
}

let dragStart = null;
let paintPointerId = null;
let connectorDraft = null;
const raycaster = new THREE.Raycaster();
const pointer = new THREE.Vector2();
const paintPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);

function gridCellFromEvent(event) {
  const rect = canvas.getBoundingClientRect();
  pointer.x = ((event.clientX - rect.left) / Math.max(rect.width, 1)) * 2 - 1;
  pointer.y = -(((event.clientY - rect.top) / Math.max(rect.height, 1)) * 2 - 1);
  raycaster.setFromCamera(pointer, camera);
  const point = new THREE.Vector3();
  paintPlane.constant = -activeFloor().z;
  if (!raycaster.ray.intersectPlane(paintPlane, point)) return null;
  const x = Math.floor(point.x);
  const y = Math.floor(point.z);
  if (x < 0 || x >= width || y < 0 || y >= height) return null;
  return { x, y };
}

function paintCellFromEvent(event) {
  const cell = gridCellFromEvent(event);
  if (!cell) return false;
  const token = String(paintToken.value || ".").slice(0, 1);
  activeEditorGrid()[cell.y][cell.x] = token;
  if (activeFloorIndex() === 0) editorGrid[cell.y][cell.x] = token;
  renderLayoutGrid();
  const result = renderValidationOverlay();
  updateEditorStatus(`${activeFloor().id}:${cell.x},${cell.y} ${tokenLabel(token)} | ${validationSummary(result)}`);
  return true;
}

canvas.addEventListener("pointerdown", event => {
  if (authorMode.checked && event.button === 0) {
    event.preventDefault();
    const cell = gridCellFromEvent(event);
    if (!cell) return;
    const tool = authorTool.value;
    if (tool === "paint") {
      paintPointerId = event.pointerId;
      canvas.setPointerCapture(event.pointerId);
      paintCellFromEvent(event);
    } else if (tool === "connector") {
      connectorDraft = {
        pointerId: event.pointerId,
        from: { floor: activeFloor().id, x: cell.x, y: cell.y },
      };
      canvas.setPointerCapture(event.pointerId);
      updateEditorStatus(`connector start ${connectorDraft.from.floor}:${cell.x},${cell.y}`);
    } else if (tool === "hostile") {
      addHostileChannel(cell);
      renderValidationOverlay();
    } else if (tool === "dispatch") {
      setDispatchTarget(cell);
    }
    return;
  }
  if (event.button !== 0) return;
  dragStart = { x: event.clientX, y: event.clientY, pointerId: event.pointerId };
  canvas.setPointerCapture(event.pointerId);
});
canvas.addEventListener("pointermove", event => {
  if (paintPointerId === event.pointerId && authorMode.checked) {
    event.preventDefault();
    paintCellFromEvent(event);
    return;
  }
  if (connectorDraft && event.pointerId === connectorDraft.pointerId) {
    const cell = gridCellFromEvent(event);
    if (cell) updateEditorStatus(`connector ${connectorDraft.from.floor}:${connectorDraft.from.x},${connectorDraft.from.y} -> ${connectorToFloorSelect.value}:${cell.x},${cell.y}`);
    return;
  }
  if (authorMode.checked && paintPointerId === null) {
    const cell = gridCellFromEvent(event);
    if (cell) updateEditorStatus(`${activeFloor().id}:${cell.x},${cell.y} ${authorActionLabel()}`);
  }
  if (!dragStart || event.pointerId !== dragStart.pointerId) return;
  const dx = event.clientX - dragStart.x;
  const dy = event.clientY - dragStart.y;
  rotateCamera(dx * 0.006, dy * 0.004);
  dragStart = { x: event.clientX, y: event.clientY, pointerId: event.pointerId };
});
canvas.addEventListener("pointerup", event => {
  if (connectorDraft && event.pointerId === connectorDraft.pointerId) {
    const cell = gridCellFromEvent(event);
    if (cell) {
      addAuthoredConnector(connectorDraft.from, {
        floor: String(connectorToFloorSelect.value || activeFloor().id),
        x: cell.x,
        y: cell.y,
      });
    }
    connectorDraft = null;
    return;
  }
  if (paintPointerId === event.pointerId) {
    paintPointerId = null;
    return;
  }
  if (dragStart && event.pointerId === dragStart.pointerId) dragStart = null;
});
canvas.addEventListener("pointercancel", () => {
  dragStart = null;
  paintPointerId = null;
  connectorDraft = null;
});

function rotateCamera(deltaTheta, deltaPhi) {
  const offset = camera.position.clone().sub(controls.target);
  const spherical = new THREE.Spherical().setFromVector3(offset);
  spherical.theta -= deltaTheta;
  spherical.phi = Math.max(0.2, Math.min(Math.PI * 0.48, spherical.phi + deltaPhi));
  camera.position.copy(controls.target).add(new THREE.Vector3().setFromSpherical(spherical));
  controls.update();
}

function resize() {
  const { clientWidth, clientHeight } = canvas;
  renderer.setSize(clientWidth, clientHeight, false);
  camera.aspect = clientWidth / Math.max(clientHeight, 1);
  camera.updateProjectionMatrix();
}

function animate() {
  resize();
  controls.update();
  if (playing && playbackFrames.length) {
    frameIndex = (frameIndex + 1) % playbackFrames.length;
    scrub.value = String(frameIndex);
    drawFrame(frameIndex);
  }
  renderer.render(scene, camera);
  setTimeout(() => requestAnimationFrame(animate), playing ? 120 : 250);
}

updateEditorStatus(validationSummary(renderValidationOverlay()));
drawFrame(0);
animate();
</script>
</body>
</html>
"""
