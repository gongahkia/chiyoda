from __future__ import annotations

from pathlib import Path
from typing import Any

from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import ScenarioValidationIssue


def hazard_audit_file(path: str | Path) -> dict[str, Any]:
    manager = ScenarioManager()
    scenario = manager.load_config(str(path))
    audit = build_hazard_audit(scenario)
    audit["source_file"] = str(Path(path))
    return audit


def build_hazard_audit(scenario: dict[str, Any]) -> dict[str, Any]:
    hazards = list(scenario.get("hazards", []) or [])
    metadata = scenario.get("metadata", {}) or {}
    records: list[dict[str, Any]] = []
    issues: list[ScenarioValidationIssue] = []
    for index, hazard in enumerate(hazards):
        if not isinstance(hazard, dict):
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "invalid_hazard_config",
                    "hazard entries must be mappings",
                    source=f"hazards[{index}]",
                )
            )
            continue
        record = _hazard_record(index, hazard, metadata)
        records.append(record)
        issues.extend(_hazard_issues(record))
    issue_payloads = [issue.to_dict() for issue in issues]
    counts = {
        "hazards": len(records),
        "imported_fields": sum(1 for item in records if item["imported_field"]),
        "stylized": sum(1 for item in records if item["stylized"]),
        "with_external_reference": sum(
            1 for item in records if item["has_external_reference"]
        ),
    }
    return {
        "ok": not any(issue["severity"] == "error" for issue in issue_payloads),
        "scenario": str(scenario.get("name", "")),
        "counts": counts,
        "hazards": records,
        "issues": issue_payloads,
    }


def _hazard_record(
    index: int, hazard: dict[str, Any], metadata: dict[str, Any]
) -> dict[str, Any]:
    field = hazard.get("field")
    field_file = ""
    if isinstance(field, str):
        field_file = field
    elif isinstance(field, dict):
        field_file = str(field.get("file", ""))
    imported = field is not None
    external_reference = metadata.get("external_reference") or hazard.get(
        "external_reference"
    )
    validation_scope = metadata.get("validation_scope") or hazard.get(
        "validation_scope"
    )
    return {
        "index": index,
        "kind": str(hazard.get("type", hazard.get("kind", "GAS"))).upper(),
        "imported_field": imported,
        "stylized": not imported,
        "field_file": field_file,
        "has_external_reference": bool(external_reference or validation_scope),
        "external_reference": str(external_reference or ""),
        "validation_scope": str(validation_scope or ""),
        "radius_m": _optional_float(hazard.get("radius")),
        "severity": _optional_float(hazard.get("severity")),
        "spread_rate": _optional_float(hazard.get("spread_rate")),
        "source": f"hazards[{index}]",
    }


def _hazard_issues(record: dict[str, Any]) -> list[ScenarioValidationIssue]:
    issues: list[ScenarioValidationIssue] = []
    source = str(record["source"])
    if record["imported_field"] and not record["field_file"]:
        issues.append(
            ScenarioValidationIssue(
                "error",
                "imported_hazard_missing_file",
                "imported hazard fields require field.file",
                source=source,
            )
        )
    if record["imported_field"] and not record["has_external_reference"]:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "imported_hazard_without_reference_scope",
                "imported hazard field lacks external_reference or validation_scope metadata",
                source=source,
            )
        )
    if record["stylized"]:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "stylized_hazard",
                "hazard uses built-in stylized field dynamics, not an imported reference field",
                source=source,
            )
        )
    if record["severity"] is not None and not 0.0 <= record["severity"] <= 1.0:
        issues.append(
            ScenarioValidationIssue(
                "error",
                "hazard_severity_out_of_range",
                "hazard severity must be within [0, 1]",
                source=source,
            )
        )
    return issues


def _optional_float(value: Any) -> float | None:
    if value is None:
        return None
    return float(value)
