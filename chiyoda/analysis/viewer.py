from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any

import pandas as pd

from chiyoda.studies.models import StudyBundle


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
    data_path.write_text(json.dumps(_json_safe(payload), indent=2) + "\n")
    index_path.write_text(_viewer_html())
    return [index_path, data_path]


def _viewer_payload(bundle: StudyBundle, *, max_frames: int) -> dict[str, Any]:
    run_id = str(bundle.metadata.get("representative_run_id") or "")
    agent_steps = bundle.agent_steps.copy()
    if run_id and "run_id" in agent_steps.columns:
        agent_steps = agent_steps[agent_steps["run_id"] == run_id]
    if agent_steps.empty:
        frames: list[dict[str, Any]] = []
    else:
        steps = sorted(pd.to_numeric(agent_steps["step"], errors="coerce").dropna().unique().tolist())
        selected = set(_sample_values([int(step) for step in steps], max_frames))
        frames = []
        for step, frame in agent_steps[agent_steps["step"].isin(selected)].groupby("step", sort=True):
            agents = []
            for row in frame.itertuples(index=False):
                agents.append(
                    {
                        "id": int(getattr(row, "agent_id")),
                        "x": float(getattr(row, "x")),
                        "y": float(getattr(row, "y")),
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
            "station_provenance": bundle.metadata.get("station_provenance"),
        },
        "layout": _layout_cells(str(bundle.metadata.get("layout_text", ""))),
        "bottlenecks": bundle.metadata.get("bottleneck_zones", []),
        "hazards": _table_rows(bundle.hazards, run_id=run_id),
        "interventions": _table_rows(bundle.interventions, run_id=run_id),
        "llm_decisions": _table_rows(bundle.llm_decisions, run_id=run_id),
        "frames": frames,
    }


def _layout_cells(layout_text: str) -> list[dict[str, Any]]:
    cells = []
    for y, line in enumerate(layout_text.splitlines()):
        for x, token in enumerate(line):
            if token != ".":
                cells.append({"x": x, "y": y, "token": token})
    return cells


def _table_rows(frame: pd.DataFrame, *, run_id: str) -> list[dict[str, Any]]:
    if frame.empty:
        return []
    current = frame.copy()
    if run_id and "run_id" in current.columns:
        current = current[current["run_id"] == run_id]
    return current.to_dict(orient="records")


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
  <title>Chiyoda 3D Viewer</title>
  <style>
    html, body { margin: 0; height: 100%; font-family: system-ui, sans-serif; background: #111; color: #f5f5f5; }
    #app { display: grid; grid-template-rows: auto 1fr; height: 100%; }
    #toolbar { display: flex; gap: 12px; align-items: center; padding: 10px 12px; background: #1d1d1d; border-bottom: 1px solid #333; }
    button, input, select, label { font: inherit; }
    button { background: #e6e6e6; border: 0; padding: 5px 10px; border-radius: 4px; cursor: pointer; }
    input[type="range"] { width: min(55vw, 720px); }
    canvas { display: block; width: 100%; height: 100%; }
    .metric { color: #cfcfcf; min-width: 170px; }
  </style>
</head>
<body>
<div id="app">
  <div id="toolbar">
    <button id="play">Play</button>
    <input id="scrub" type="range" min="0" max="0" value="0">
    <span class="metric" id="step">step 0</span>
    <label><input id="hazards" type="checkbox" checked> hazards</label>
    <label><input id="bottlenecks" type="checkbox" checked> bottlenecks</label>
    <label><input id="messages" type="checkbox" checked> messages</label>
  </div>
  <canvas id="scene"></canvas>
</div>
<script type="module">
import * as THREE from "https://unpkg.com/three@0.160.0/build/three.module.js";

const data = await fetch("./viewer_data.json").then(r => r.json());
const canvas = document.querySelector("#scene");
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x111111);
const width = Number(data.metadata.layout_width || 20);
const height = Number(data.metadata.layout_height || 20);
const camera = new THREE.PerspectiveCamera(55, 1, 0.1, 2000);
camera.position.set(width * 0.55, Math.max(width, height) * 0.9, height * 1.1);
camera.lookAt(width / 2, 0, height / 2);
scene.add(new THREE.HemisphereLight(0xffffff, 0x202020, 2.2));

const root = new THREE.Group();
scene.add(root);
const agentGroup = new THREE.Group();
const hazardGroup = new THREE.Group();
const bottleneckGroup = new THREE.Group();
const messageGroup = new THREE.Group();
scene.add(agentGroup, hazardGroup, bottleneckGroup, messageGroup);

function box(x, z, sx, sz, color, y = 0.05, h = 0.1) {
  const geo = new THREE.BoxGeometry(sx, h, sz);
  const mat = new THREE.MeshStandardMaterial({ color });
  const mesh = new THREE.Mesh(geo, mat);
  mesh.position.set(x, y, z);
  return mesh;
}

root.add(box(width / 2, height / 2, width, height, 0x2b2b2b, -0.04, 0.04));
for (const cell of data.layout) {
  const color = cell.token === "X" ? 0x656565 : cell.token === "E" ? 0x3eb36f : cell.token === "@" ? 0x4f83ff : 0xd4b24c;
  const h = cell.token === "X" ? 0.9 : 0.12;
  root.add(box(cell.x + 0.5, cell.y + 0.5, 0.95, 0.95, color, h / 2, h));
}

for (const hz of data.hazards) {
  const mesh = new THREE.Mesh(
    new THREE.CylinderGeometry(Math.max(0.2, Number(hz.radius || 0.2)), Math.max(0.2, Number(hz.radius || 0.2)), 0.08, 32),
    new THREE.MeshStandardMaterial({ color: 0xe05d44, transparent: true, opacity: 0.45 })
  );
  mesh.position.set(Number(hz.x || 0), 0.08, Number(hz.y || 0));
  hazardGroup.add(mesh);
}

for (const zone of data.bottlenecks || []) {
  for (const cell of zone.cells || []) {
    bottleneckGroup.add(box(Number(cell[0]) + 0.5, Number(cell[1]) + 0.5, 0.9, 0.9, 0xf0c84b, 0.03, 0.06));
  }
}

for (const event of data.interventions || []) {
  const mesh = new THREE.Mesh(
    new THREE.SphereGeometry(0.22, 16, 12),
    new THREE.MeshStandardMaterial({ color: 0x56c7ff, emissive: 0x123344 })
  );
  mesh.position.set(Number(event.target_x || 0), 0.55, Number(event.target_y || 0));
  messageGroup.add(mesh);
}

const frames = data.frames || [];
const scrub = document.querySelector("#scrub");
const stepLabel = document.querySelector("#step");
scrub.max = Math.max(0, frames.length - 1);
let frameIndex = 0;
let playing = false;

function drawFrame(index) {
  agentGroup.clear();
  const frame = frames[index] || { step: 0, agents: [] };
  for (const agent of frame.agents) {
    const entropy = Math.max(0, Math.min(1, Number(agent.entropy || 0)));
    const color = new THREE.Color().setHSL(0.58 - entropy * 0.45, 0.75, 0.55);
    const mesh = new THREE.Mesh(
      new THREE.SphereGeometry(0.18, 16, 12),
      new THREE.MeshStandardMaterial({ color })
    );
    mesh.position.set(Number(agent.x), 0.32, Number(agent.y));
    agentGroup.add(mesh);
  }
  stepLabel.textContent = `step ${frame.step} | agents ${frame.agents.length}`;
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
document.querySelector("#messages").addEventListener("change", event => messageGroup.visible = event.target.checked);

function resize() {
  const { clientWidth, clientHeight } = canvas;
  renderer.setSize(clientWidth, clientHeight, false);
  camera.aspect = clientWidth / Math.max(clientHeight, 1);
  camera.updateProjectionMatrix();
}

function animate() {
  resize();
  if (playing && frames.length) {
    frameIndex = (frameIndex + 1) % frames.length;
    scrub.value = String(frameIndex);
    drawFrame(frameIndex);
  }
  renderer.render(scene, camera);
  setTimeout(() => requestAnimationFrame(animate), playing ? 120 : 250);
}

drawFrame(0);
animate();
</script>
</body>
</html>
"""
