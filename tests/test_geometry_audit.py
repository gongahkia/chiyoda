from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.geometry_audit import build_geometry_audit


def _multifloor_scenario():
    return {
        "name": "audit_fixture",
        "metadata": {
            "station_provenance": {
                "station": "Fixture Station",
                "level": "concourse",
                "source_url": "https://example.test/station",
                "license": "fixture",
                "access_date": "2026-06-25",
                "coordinate_transform": "grid fixture",
                "manual_edits": ["synthetic corridor"],
                "known_missing_indoor_topology": ["fixture only"],
                "validation_use": "diagnostic only, not operational validation",
                "attribution": "test",
                "source_files": ["fixture.geojson"],
            }
        },
        "layout": {
            "floors": [
                {"id": "0", "z": 0.0, "text": "XXXXX\nX@.EX\nXXXXX"},
                {"id": "1", "z": 4.0, "text": "XXXXX\nX...X\nXXXXX"},
            ],
            "connectors": [
                {
                    "id": "lift",
                    "type": "elevator",
                    "from": {"floor": "0", "x": 1, "y": 1},
                    "to": {"floor": "1", "x": 1, "y": 1},
                }
            ],
        },
        "population": {"total": 1},
    }


def test_geometry_audit_reports_counts_reachability_and_provenance():
    audit = build_geometry_audit(_multifloor_scenario())

    assert audit["ok"]
    assert audit["counts"]["floors"] == 2
    assert audit["counts"]["connectors"] == 1
    assert audit["counts"]["unreachable_walkable_cells"] == 0
    assert audit["provenance"]["station"] == "Fixture Station"
    assert audit["connectors"][0]["height_delta_m"] == 4.0
    assert any(
        issue["code"] == "elevator_without_service_timing" for issue in audit["issues"]
    )


def test_geometry_audit_cli_writes_json_artifact(tmp_path):
    scenario = tmp_path / "scenario.yaml"
    output = tmp_path / "audit.json"
    scenario.write_text(
        """
scenario:
  name: cli_audit
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |
          XXXXX
          X@.EX
          XXXXX
  population:
    total: 1
"""
    )

    result = CliRunner().invoke(
        cli, ["geometry-audit", str(scenario), "-o", str(output), "--json"]
    )
    payload = json.loads(output.read_text())

    assert result.exit_code == 0, result.output
    assert payload["ok"]
    assert payload["counts"]["exit_cells"] == 1
    assert payload["scenario"] == "cli_audit"
    assert json.loads(result.output)["counts"]["walkable_cells"] == 3
