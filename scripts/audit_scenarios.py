from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import yaml

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from chiyoda.scenarios.calibration_audit import calibration_audit_file
from chiyoda.scenarios.geometry_audit import geometry_audit_file
from chiyoda.scenarios.hazard_audit import hazard_audit_file
from chiyoda.scenarios.validation import validate_scenario_file
from chiyoda.scenarios.validation_evidence_audit import validation_evidence_audit_file


def discover_scenario_files(
    root: str | Path = "scenarios",
) -> tuple[list[Path], list[Path]]:
    scenario_files: list[Path] = []
    skipped_files: list[Path] = []
    for path in sorted(Path(root).glob("**/*.yaml")):
        payload = yaml.safe_load(path.read_text()) or {}
        if _is_scenario_payload(payload):
            scenario_files.append(path)
        else:
            skipped_files.append(path)
    return scenario_files, skipped_files


def audit_scenario_corpus(root: str | Path = "scenarios") -> dict[str, Any]:
    scenario_files, skipped_files = discover_scenario_files(root)
    records = [_audit_scenario_file(path) for path in scenario_files]
    totals = {
        "errors": sum(record["error_count"] for record in records),
        "warnings": sum(record["warning_count"] for record in records),
        "runtime_assertion_files": sum(
            1 for record in records if record["has_runtime_assertions"]
        ),
    }
    return {
        "ok": totals["errors"] == 0,
        "root": str(root),
        "scenario_count": len(records),
        "skipped_count": len(skipped_files),
        "skipped_files": [str(path) for path in skipped_files],
        "totals": totals,
        "files": records,
    }


def _is_scenario_payload(payload: Any) -> bool:
    if not isinstance(payload, dict):
        return False
    scenario = payload.get("scenario", payload)
    return isinstance(scenario, dict) and isinstance(scenario.get("layout"), dict)


def _audit_scenario_file(path: Path) -> dict[str, Any]:
    validation = validate_scenario_file(path).to_dict()
    audits = {
        "validation": validation,
        "geometry": geometry_audit_file(path),
        "hazard": hazard_audit_file(path),
        "calibration": calibration_audit_file(path),
        "validation_evidence": validation_evidence_audit_file(path),
    }
    errors = _issues(audits, "error")
    warnings = _issues(audits, "warning")
    payload = yaml.safe_load(path.read_text()) or {}
    scenario = payload.get("scenario", payload) if isinstance(payload, dict) else {}
    return {
        "path": str(path),
        "ok": len(errors) == 0,
        "name": (
            str(scenario.get("name", path.stem))
            if isinstance(scenario, dict)
            else path.stem
        ),
        "has_runtime_assertions": bool(
            isinstance(scenario, dict) and scenario.get("assertions")
        ),
        "audit_ok": {name: bool(audit["ok"]) for name, audit in audits.items()},
        "error_count": len(errors),
        "warning_count": len(warnings),
        "errors": errors,
        "warnings": warnings,
    }


def _issues(audits: dict[str, dict[str, Any]], severity: str) -> list[dict[str, Any]]:
    issues: list[dict[str, Any]] = []
    for audit_name, audit in audits.items():
        for issue in audit.get("issues", []):
            if issue.get("severity") == severity:
                issues.append({"audit": audit_name, **issue})
    return issues


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit all scenario YAML files.")
    parser.add_argument("--root", default="scenarios", help="scenario corpus root")
    parser.add_argument("-o", "--output", help="optional JSON output path")
    parser.add_argument("--json", action="store_true", help="print JSON output")
    args = parser.parse_args()

    payload = audit_scenario_corpus(args.root)
    text = json.dumps(payload, indent=2, sort_keys=True)
    if args.output:
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text + "\n")
    if args.json:
        print(text)
    else:
        status = "OK" if payload["ok"] else "ERROR"
        totals = payload["totals"]
        print(
            f"{status}: scenarios={payload['scenario_count']} "
            f"skipped={payload['skipped_count']} errors={totals['errors']} "
            f"warnings={totals['warnings']} assertions={totals['runtime_assertion_files']}"
        )
    return 0 if payload["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
