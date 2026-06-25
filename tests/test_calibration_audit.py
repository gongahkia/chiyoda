from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.calibration_audit import build_calibration_audit


def test_calibration_audit_reports_profile_provenance_and_population_bounds():
    audit = build_calibration_audit(
        {
            "name": "calibrated",
            "social_force_calibration": "yolov5_mdpi_2024",
            "population": {
                "total": 1,
                "cohorts": [{"name": "baseline", "count": 1, "base_speed": 1.2}],
            },
        }
    )

    assert audit["ok"]
    assert audit["social_force"]["profile"] == "yolov5_mdpi_2024"
    assert audit["social_force"]["has_provenance"] is True
    assert audit["social_force"]["generic_legacy"] is False
    assert audit["population"]["cohort_count"] == 1


def test_calibration_audit_flags_generic_defaults():
    audit = build_calibration_audit({"name": "generic", "population": {"total": 1}})

    assert audit["ok"]
    assert any(
        issue["code"] == "generic_social_force_profile" for issue in audit["issues"]
    )
    assert any(
        issue["code"] == "implicit_population_calibration" for issue in audit["issues"]
    )


def test_calibration_audit_cli_writes_json_artifact(tmp_path):
    scenario = tmp_path / "scenario.yaml"
    output = tmp_path / "calibration_audit.json"
    scenario.write_text(
        """
scenario:
  name: calibration_cli
  social_force_calibration: yolov5_mdpi_2024
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
    cohorts:
      - name: baseline
        count: 1
        base_speed: 1.2
"""
    )

    result = CliRunner().invoke(
        cli, ["calibration-audit", str(scenario), "-o", str(output), "--json"]
    )
    payload = json.loads(output.read_text())

    assert result.exit_code == 0, result.output
    assert payload["scenario"] == "calibration_cli"
    assert payload["social_force"]["profile"] == "yolov5_mdpi_2024"
    assert json.loads(result.output)["population"]["cohort_count"] == 1
