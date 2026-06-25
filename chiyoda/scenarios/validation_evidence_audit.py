from __future__ import annotations

from pathlib import Path
from typing import Any

from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import ScenarioValidationIssue

VALIDATION_EVIDENCE_TYPES = {
    "drill",
    "incident",
    "expert_coded",
    "trajectory",
    "reference",
    "hazard_field",
    "calibration",
}
OPERATIONAL_EVIDENCE_TYPES = {"drill", "incident", "expert_coded", "trajectory"}


def validation_evidence_audit_file(path: str | Path) -> dict[str, Any]:
    manager = ScenarioManager()
    scenario = manager.load_config(str(path))
    audit = build_validation_evidence_audit(scenario)
    audit["source_file"] = str(Path(path))
    return audit


def build_validation_evidence_audit(scenario: dict[str, Any]) -> dict[str, Any]:
    metadata = scenario.get("metadata", {}) or {}
    if not isinstance(metadata, dict):
        metadata = {}
    raw_evidence = metadata.get("external_validation_evidence", []) or []
    required = _requires_external_validation(metadata)
    issues: list[ScenarioValidationIssue] = []
    records: list[dict[str, Any]] = []

    if not isinstance(raw_evidence, list):
        raw_evidence = []
        issues.append(
            ScenarioValidationIssue(
                "error",
                "external_validation_evidence_not_list",
                "metadata.external_validation_evidence must be a list",
                source="metadata.external_validation_evidence",
            )
        )

    for index, item in enumerate(raw_evidence):
        if not isinstance(item, dict):
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "invalid_validation_evidence_record",
                    "validation evidence records must be mappings",
                    source=f"metadata.external_validation_evidence[{index}]",
                )
            )
            continue
        record = _evidence_record(index, item, scenario)
        records.append(record)
        issues.extend(_record_issues(record))

    if required and not records:
        issues.append(
            ScenarioValidationIssue(
                "error",
                "external_validation_evidence_required",
                "report-facing or external-validation claim lacks validation evidence records",
                source="metadata.external_validation_evidence",
            )
        )
    elif not records:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "external_validation_evidence_missing",
                "no drill, incident, trajectory, or expert-coded validation evidence is recorded",
                source="metadata.external_validation_evidence",
            )
        )

    issue_payloads = [issue.to_dict() for issue in issues]
    operational_count = sum(
        1 for record in records if record["type"] in OPERATIONAL_EVIDENCE_TYPES
    )
    counts = {
        "evidence_records": len(records),
        "operational_records": operational_count,
        "file_backed_records": sum(1 for record in records if record["file"]),
        "existing_files": sum(1 for record in records if record["file_exists"]),
    }
    return {
        "ok": not any(issue["severity"] == "error" for issue in issue_payloads),
        "scenario": str(scenario.get("name", "")),
        "required": required,
        "claim_support": _claim_support(records, operational_count),
        "counts": counts,
        "evidence": records,
        "issues": issue_payloads,
    }


def _requires_external_validation(metadata: dict[str, Any]) -> bool:
    return bool(
        metadata.get("external_validation_claim")
        or metadata.get("report_facing_station_case")
        or metadata.get("operational_claim")
    )


def _evidence_record(
    index: int, item: dict[str, Any], scenario: dict[str, Any]
) -> dict[str, Any]:
    evidence_file = str(item.get("file", ""))
    resolved_file = _resolved_evidence_file(evidence_file, scenario)
    return {
        "index": index,
        "type": str(item.get("type", "")),
        "source": str(item.get("source", "")),
        "validation_use": str(item.get("validation_use", "")),
        "scope": str(item.get("scope", "")),
        "metrics": list(item.get("metrics", []) or []),
        "license": str(item.get("license", "")),
        "file": evidence_file,
        "file_exists": bool(resolved_file and resolved_file.exists()),
        "source_path": str(resolved_file or ""),
        "config_source": f"metadata.external_validation_evidence[{index}]",
    }


def _resolved_evidence_file(value: str, scenario: dict[str, Any]) -> Path | None:
    if not value:
        return None
    path = Path(value)
    if path.is_absolute():
        return path
    source_file = scenario.get("_source_file")
    if source_file:
        return Path(str(source_file)).resolve().parent / path
    return path


def _record_issues(record: dict[str, Any]) -> list[ScenarioValidationIssue]:
    issues: list[ScenarioValidationIssue] = []
    source = str(record["config_source"])
    evidence_type = str(record["type"])
    if evidence_type not in VALIDATION_EVIDENCE_TYPES:
        issues.append(
            ScenarioValidationIssue(
                "error",
                "unsupported_validation_evidence_type",
                f"validation evidence type must be one of {sorted(VALIDATION_EVIDENCE_TYPES)}",
                source=f"{source}.type",
            )
        )
    for field_name in ("source", "validation_use", "scope"):
        if not str(record.get(field_name, "")):
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "validation_evidence_field_missing",
                    f"validation evidence field '{field_name}' is required",
                    source=f"{source}.{field_name}",
                )
            )
    if record["file"] and not record["file_exists"]:
        issues.append(
            ScenarioValidationIssue(
                "error",
                "validation_evidence_file_missing",
                "validation evidence file does not exist",
                source=f"{source}.file",
            )
        )
    return issues


def _claim_support(records: list[dict[str, Any]], operational_count: int) -> str:
    if operational_count > 0:
        return "external_evidence_recorded"
    if records:
        return "documented_limits"
    return "none"
