from __future__ import annotations

import matplotlib.pyplot as plt
import pandas as pd
from click.testing import CliRunner

from chiyoda.analysis.metrics import equity_subgroup_metrics
from chiyoda.analysis.reports import _figure_equity_subgroups
from chiyoda.cli import cli
from chiyoda.studies.models import StudyBundle


def test_equity_subgroup_metrics_break_down_agent_outcomes():
    agents = pd.DataFrame(
        [
            _agent(1, True, 10.0, 0.2, 0.2, 0.2, "senior"),
            _agent(2, True, 20.0, 0.4, 0.0, 0.8, "adult"),
            _agent(3, False, 0.0, 0.6, 0.0, 0.5, "adult"),
        ]
    )

    frame = equity_subgroup_metrics(agents)
    tags = set(frame["subgroup_tag"])

    assert {"impaired", "elderly", "low_familiarity", "high_familiarity"} <= tags
    impaired = frame[frame["subgroup_tag"] == "impaired"].iloc[0]
    assert impaired["agent_count"] == 1
    assert impaired["mean_travel_time_s"] == 10.0
    assert impaired["evacuation_rate_gap_vs_run"] > 0.0


def test_run_exports_equity_subgroups_and_loads_bundle(tmp_path):
    scenario_path = tmp_path / "scenario.yaml"
    scenario_path.write_text(
        """
name: equity_subgroups_toy
layout:
  floors:
    - id: "0"
      z: 0.0
      text: |
        XXXXXXX
        X@...EX
        XXXXXXX
population:
  cohorts:
    - name: seniors
      count: 1
      familiarity: 0.2
      age_band: senior
      spawn_cells: [{floor: "0", x: 1, y: 1}]
    - name: regulars
      count: 1
      familiarity: 0.8
      age_band: adult
      spawn_cells: [{floor: "0", x: 2, y: 1}]
simulation:
  max_steps: 6
  random_seed: 11
"""
    )
    output_dir = tmp_path / "out"

    result = CliRunner().invoke(
        cli,
        ["run", str(scenario_path), "-o", str(output_dir), "--table-format", "csv"],
    )

    assert result.exit_code == 0, result.output
    frame = pd.read_csv(output_dir / "tables" / "equity_subgroups.csv")
    assert {"elderly", "low_familiarity", "high_familiarity"} <= set(
        frame["subgroup_tag"]
    )
    agents = pd.read_csv(output_dir / "tables" / "agents.csv")
    assert {"age_band", "familiarity", "impairment"}.issubset(agents.columns)
    loaded = StudyBundle.load(output_dir)
    assert not loaded.equity_subgroups.empty


def test_equity_subgroup_report_figure_smoke():
    bundle = StudyBundle(
        metadata={},
        summary=pd.DataFrame(),
        steps=pd.DataFrame(),
        cells=pd.DataFrame(),
        agent_steps=pd.DataFrame(),
        agents=pd.DataFrame(),
        bottlenecks=pd.DataFrame(),
        dwell_samples=pd.DataFrame(),
        exits=pd.DataFrame(),
        hazards=pd.DataFrame(),
        equity_subgroups=pd.DataFrame(
            [
                {
                    "variant_name": "baseline",
                    "subgroup_tag": "elderly",
                    "evacuation_rate_gap_vs_run": -0.25,
                    "equity_time_gap_s": 4.0,
                }
            ]
        ),
    )

    fig = _figure_equity_subgroups(bundle)

    assert len(fig.axes) >= 2
    plt.close(fig)


def _agent(
    agent_id: int,
    evacuated: bool,
    travel_time_s: float,
    hazard_exposure: float,
    impairment: float,
    familiarity: float,
    age_band: str,
) -> dict[str, object]:
    return {
        "study_name": "study",
        "scenario_name": "scenario",
        "variant_name": "baseline",
        "seed": 1,
        "run_id": "run_1",
        "agent_id": agent_id,
        "evacuated": evacuated,
        "travel_time_s": travel_time_s,
        "hazard_exposure": hazard_exposure,
        "impairment": impairment,
        "familiarity": familiarity,
        "age_band": age_band,
    }
