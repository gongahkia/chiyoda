from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.validation_evidence_audit import build_validation_evidence_audit


def test_validation_evidence_audit_flags_required_missing_records():
    audit = build_validation_evidence_audit(
        {
            "name": "claimed_station_case",
            "metadata": {"external_validation_claim": True},
        }
    )

    assert audit["ok"] is False
    assert audit["required"] is True
    assert audit["claim_support"] == "none"
    assert any(
        issue["code"] == "external_validation_evidence_required"
        for issue in audit["issues"]
    )


def test_validation_evidence_audit_accepts_file_backed_operational_record(tmp_path):
    evidence_file = tmp_path / "drill.csv"
    evidence_file.write_text("time_s,evacuated\n1,1\n")
    scenario = {
        "name": "evidence_case",
        "_source_file": str(tmp_path / "scenario.yaml"),
        "metadata": {
            "external_validation_claim": True,
            "external_validation_evidence": [
                {
                    "type": "drill",
                    "source": "fixture drill",
                    "validation_use": "egress curve calibration",
                    "scope": "synthetic fixture",
                    "metrics": ["evacuation_time"],
                    "file": "drill.csv",
                }
            ],
        },
    }

    audit = build_validation_evidence_audit(scenario)

    assert audit["ok"] is True
    assert audit["claim_support"] == "external_evidence_recorded"
    assert audit["counts"]["operational_records"] == 1
    assert audit["counts"]["existing_files"] == 1
    assert audit["evidence"][0]["source"] == "fixture drill"


def test_validation_evidence_audit_cli_writes_json_artifact(tmp_path):
    evidence_file = tmp_path / "incident.json"
    evidence_file.write_text("{}\n")
    scenario = tmp_path / "scenario.yaml"
    output = tmp_path / "validation_evidence_audit.json"
    scenario.write_text(
        """
scenario:
  name: validation_evidence_cli
  metadata:
    external_validation_claim: true
    external_validation_evidence:
      - type: incident
        source: fixture incident
        validation_use: smoke test only
        scope: egress timing
        file: incident.json
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
        cli,
        ["validation-evidence-audit", str(scenario), "-o", str(output), "--json"],
    )
    payload = json.loads(output.read_text())

    assert result.exit_code == 0, result.output
    assert payload["scenario"] == "validation_evidence_cli"
    assert payload["counts"]["operational_records"] == 1
    assert json.loads(result.output)["claim_support"] == "external_evidence_recorded"
