from __future__ import annotations

import json

import pandas as pd
from click.testing import CliRunner

from chiyoda.analysis.viewer import export_viewer
from chiyoda.cli import cli
from chiyoda.studies.models import StudyBundle


def test_run_per_step_intent_writes_sparse_tensor(tmp_path):
    scenario_path = tmp_path / "scenario.yaml"
    scenario_path.write_text(
        """
name: intent_path_usage_toy
layout:
  floors:
    - id: "0"
      z: 0.0
      text: |
        XXXXXX
        X@..EX
        XXXXXX
population:
  total: 1
simulation:
  max_steps: 3
  random_seed: 9
"""
    )
    output_dir = tmp_path / "out"

    result = CliRunner().invoke(
        cli,
        [
            "run",
            str(scenario_path),
            "-o",
            str(output_dir),
            "--per-step-intent",
            "--table-format",
            "csv",
        ],
    )

    assert result.exit_code == 0, result.output
    frame = pd.read_csv(output_dir / "tables" / "intent_path_usage.csv")
    assert not frame.empty
    assert {"step", "floor_id", "x", "y", "intent", "count"}.issubset(frame.columns)
    assert (
        frame["count"].sum()
        <= pd.read_csv(output_dir / "tables" / "agent_steps.csv").shape[0]
    )


def test_viewer_payload_loads_intent_path_usage(tmp_path):
    bundle = StudyBundle(
        metadata={
            "study_name": "intent_viewer",
            "scenario_name": "intent_viewer",
            "representative_run_id": "run_1",
            "layout_text": "XXX\nXEX\nX@X",
            "layout_floors": [{"id": "0", "z": 0.0, "text": "XXX\nXEX\nX@X"}],
            "layout_width": 3,
            "layout_height": 3,
        },
        summary=pd.DataFrame(),
        steps=pd.DataFrame(),
        cells=pd.DataFrame(),
        agent_steps=pd.DataFrame(),
        agents=pd.DataFrame(),
        bottlenecks=pd.DataFrame(),
        dwell_samples=pd.DataFrame(),
        exits=pd.DataFrame(),
        hazards=pd.DataFrame(),
        intent_path_usage=pd.DataFrame(
            [
                {
                    "run_id": "run_1",
                    "step": 0,
                    "time_s": 0.0,
                    "floor_id": "0",
                    "z": 0.0,
                    "x": 1,
                    "y": 2,
                    "intent": "EVACUATE",
                    "count": 1,
                }
            ]
        ),
    )

    export_viewer(bundle, tmp_path)
    data = json.loads((tmp_path / "viewer_data.json").read_text())

    assert data["intent_path_usage"][0]["intent"] == "EVACUATE"
