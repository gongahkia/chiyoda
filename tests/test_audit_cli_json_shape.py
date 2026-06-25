from __future__ import annotations

import json
from pathlib import Path

from click.testing import CliRunner

from chiyoda.cli import cli


def test_audit_cli_json_shapes(tmp_path):
    scenario = _scenario_with_validation_evidence(tmp_path)
    cases = {
        "geometry-audit": {
            "root": {
                "ok",
                "scenario",
                "layout",
                "counts",
                "connectors",
                "starts",
                "reachability",
                "provenance",
                "issues",
                "source_file",
            },
            "nested": {
                "layout": {
                    "floor_count",
                    "primary_floor_id",
                    "cell_size_m",
                    "origin",
                    "floors",
                },
                "counts": {
                    "floors",
                    "walkable_cells",
                    "exit_cells",
                    "connectors",
                    "starts",
                },
                "reachability": {
                    "reachable_cells",
                    "unreachable_walkable_cells",
                    "paths",
                },
            },
        },
        "hazard-audit": {
            "root": {"ok", "scenario", "counts", "hazards", "issues", "source_file"},
            "nested": {
                "counts": {
                    "hazards",
                    "imported_fields",
                    "stylized",
                    "with_external_reference",
                },
            },
        },
        "calibration-audit": {
            "root": {
                "ok",
                "scenario",
                "social_force",
                "population",
                "issues",
                "source_file",
            },
            "nested": {
                "social_force": {
                    "profile",
                    "parameters",
                    "provenance",
                    "has_provenance",
                    "generic_legacy",
                },
                "population": {
                    "total",
                    "cohort_count",
                    "cohort_parameter_provenance_count",
                },
            },
        },
        "validation-evidence-audit": {
            "root": {
                "ok",
                "scenario",
                "required",
                "claim_support",
                "counts",
                "evidence",
                "issues",
                "source_file",
            },
            "nested": {
                "counts": {
                    "evidence_records",
                    "operational_records",
                    "file_backed_records",
                    "existing_files",
                },
            },
        },
    }

    for command, expected in cases.items():
        result = CliRunner().invoke(cli, [command, str(scenario), "--json"])
        payload = json.loads(result.output)

        assert result.exit_code == 0, result.output
        assert expected["root"].issubset(payload)
        for key, nested_keys in expected["nested"].items():
            assert nested_keys.issubset(payload[key])


def _scenario_with_validation_evidence(tmp_path: Path) -> Path:
    evidence_file = tmp_path / "drill.csv"
    evidence_file.write_text("time_s,evacuated\n1,1\n")
    scenario = tmp_path / "scenario.yaml"
    scenario.write_text(
        """
scenario:
  name: audit_shape
  metadata:
    external_validation_claim: true
    external_validation_evidence:
      - type: drill
        source: fixture drill
        validation_use: schema smoke
        scope: test fixture
        file: drill.csv
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
  hazards:
    - type: SMOKE
      location: [1, 1, 0]
      radius: 1.0
      severity: 0.3
"""
    )
    return scenario
