from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.validation import validate_scenario_config


def _layout(text: str) -> dict:
    return {"floors": [{"id": "0", "z": 0.0, "text": text}]}


def test_validate_scenario_accepts_reachable_spawn():
    result = validate_scenario_config(
        {
            "name": "valid",
            "layout": _layout("XXXXXX\nX@..EX\nXXXXXX"),
            "population": {"total": 1},
        }
    )

    assert not result.has_errors
    assert result.exits == [("0", 4, 1)]
    assert result.paths["spawn_0"] == [
        ("0", 1, 1),
        ("0", 2, 1),
        ("0", 3, 1),
        ("0", 4, 1),
    ]


def test_validate_scenario_rejects_spawn_cut_off_from_exit():
    result = validate_scenario_config(
        {
            "name": "invalid",
            "layout": _layout("XXXXXX\nX@X.EX\nXXXXXX"),
            "population": {"total": 1},
        }
    )

    assert result.has_errors
    assert any(
        issue.code == "start_unreachable" and issue.cell == ("0", 1, 1)
        for issue in result.issues
    )
    assert any(issue.code == "unreachable_walkable_cells" for issue in result.issues)


def test_validate_scenario_catches_explicit_spawn_on_wall():
    result = validate_scenario_config(
        {
            "name": "wall_spawn",
            "layout": _layout("XXXXXX\nX...EX\nXXXXXX"),
            "population": {
                "total": 1,
                "cohorts": [
                    {
                        "name": "bad",
                        "count": 1,
                        "spawn_cells": [{"floor": "0", "x": 0, "y": 0}],
                    }
                ],
            },
        }
    )

    assert result.has_errors
    assert any(
        issue.code == "start_on_wall"
        and issue.source == "population.cohorts.bad.spawn_cells"
        for issue in result.issues
    )


def test_validate_scenario_cli_emits_json_and_fails_on_errors(tmp_path):
    scenario = tmp_path / "invalid.yaml"
    scenario.write_text(
        """
scenario:
  name: invalid
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |
          XXXXXX
          X@X.EX
          XXXXXX
  population:
    total: 1
"""
    )

    result = CliRunner().invoke(cli, ["validate-scenario", str(scenario), "--json"])
    payload = json.loads(result.output)

    assert result.exit_code == 1
    assert payload["ok"] is False
    assert any(issue["code"] == "start_unreachable" for issue in payload["issues"])
