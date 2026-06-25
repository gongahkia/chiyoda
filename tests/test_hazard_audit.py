from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.hazard_audit import build_hazard_audit


def test_hazard_audit_distinguishes_imported_and_stylized_fields():
    audit = build_hazard_audit(
        {
            "name": "hazard_fixture",
            "metadata": {
                "external_reference": "reference.csv",
                "validation_scope": "scalar import agreement",
            },
            "hazards": [
                {"type": "SMOKE", "field": {"file": "reference.csv"}},
                {"type": "GAS", "location": [1, 1, 0], "radius": 2, "severity": 0.5},
            ],
        }
    )

    assert audit["ok"]
    assert audit["counts"]["hazards"] == 2
    assert audit["counts"]["imported_fields"] == 1
    assert audit["counts"]["stylized"] == 1
    assert any(issue["code"] == "stylized_hazard" for issue in audit["issues"])


def test_hazard_audit_cli_writes_json_artifact(tmp_path):
    scenario = tmp_path / "scenario.yaml"
    output = tmp_path / "hazard_audit.json"
    scenario.write_text(
        """
scenario:
  name: hazard_cli
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
  hazards:
    - type: GAS
      location: [1, 1, 0]
      radius: 1.0
      severity: 0.4
"""
    )

    result = CliRunner().invoke(
        cli, ["hazard-audit", str(scenario), "-o", str(output), "--json"]
    )
    payload = json.loads(output.read_text())

    assert result.exit_code == 0, result.output
    assert payload["scenario"] == "hazard_cli"
    assert payload["counts"]["stylized"] == 1
    assert json.loads(result.output)["counts"]["hazards"] == 1
