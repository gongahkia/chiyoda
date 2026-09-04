from __future__ import annotations

from pathlib import Path
from typing import Any

from chiyoda.navigation.social_force import load_social_force_calibration
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import ScenarioValidationIssue


def calibration_audit_file(path: str | Path) -> dict[str, Any]:
    manager = ScenarioManager()
    scenario = manager.load_config(str(path))
    audit = build_calibration_audit(scenario)
    audit["source_file"] = str(Path(path))
    return audit


def build_calibration_audit(scenario: dict[str, Any]) -> dict[str, Any]:
    sfm = load_social_force_calibration(
        scenario.get("social_force_calibration", scenario.get("sfm_calibration"))
    )
    population = scenario.get("population", {}) or {}
    cohorts = list(population.get("cohorts", []) or [])
    issues = _social_force_issues(sfm)
    issues.extend(_population_issues(population, cohorts))
    issue_payloads = [issue.to_dict() for issue in issues]
    parameter_provenance_count = sum(
        len(cohort.get("parameter_provenance", {}) or {})
        for cohort in cohorts
        if isinstance(cohort, dict)
    )
    generated_audit = (scenario.get("metadata", {}) or {}).get(
        "generated_population_calibration_audit",
        {},
    )
    return {
        "ok": not any(issue["severity"] == "error" for issue in issue_payloads),
        "scenario": str(scenario.get("name", "")),
        "social_force": {
            "profile": sfm.profile,
            "parameters": sfm.to_parameters(),
            "provenance": dict(sfm.provenance or {}),
            "has_provenance": bool(sfm.provenance),
            "generic_legacy": sfm.profile == "generic_legacy",
        },
        "population": {
            "total": int(population.get("total", 0) or 0),
            "cohort_count": len(cohorts),
            "cohort_parameter_provenance_count": parameter_provenance_count,
            "generated_calibration_status": str(
                generated_audit.get("validation_status", "")
            ),
            "generated_cache_status": str(generated_audit.get("cache_status", "")),
        },
        "issues": issue_payloads,
    }


def _social_force_issues(sfm: Any) -> list[ScenarioValidationIssue]:
    issues: list[ScenarioValidationIssue] = []
    params = sfm.to_parameters()
    if sfm.profile == "generic_legacy":
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "generic_social_force_profile",
                "scenario uses legacy generic social-force defaults",
                source="social_force_calibration",
            )
        )
    if not sfm.provenance:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "social_force_provenance_missing",
                "social-force profile has no parameter provenance metadata",
                source="social_force_calibration",
            )
        )
    _range_issue(
        issues,
        "desired_speed_mps",
        params["desired_speed_mps"],
        0.2,
        3.0,
        "social_force_calibration.desired_speed_mps",
    )
    _positive_issue(
        issues,
        "relaxation_time_s",
        params["relaxation_time_s"],
        "social_force_calibration.relaxation_time_s",
    )
    return issues


def _population_issues(
    population: dict[str, Any], cohorts: list[Any]
) -> list[ScenarioValidationIssue]:
    issues: list[ScenarioValidationIssue] = []
    total = int(population.get("total", 0) or 0)
    if total > 0 and not cohorts:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "implicit_population_calibration",
                "population has no explicit cohorts; defaults will be used",
                source="population",
            )
        )
    for index, cohort in enumerate(cohorts):
        if not isinstance(cohort, dict):
            continue
        speed = cohort.get("base_speed", cohort.get("base_speed_mps"))
        if speed is not None:
            _range_issue(
                issues,
                "cohort_base_speed_mps",
                float(speed),
                0.2,
                3.0,
                f"population.cohorts[{index}]",
            )
    return issues


def _range_issue(
    issues: list[ScenarioValidationIssue],
    code: str,
    value: float,
    lower: float,
    upper: float,
    source: str,
) -> None:
    if lower <= value <= upper:
        return
    issues.append(
        ScenarioValidationIssue(
            "error",
            code,
            f"value {value} is outside [{lower}, {upper}]",
            source=source,
        )
    )


def _positive_issue(
    issues: list[ScenarioValidationIssue], code: str, value: float, source: str
) -> None:
    if value > 0:
        return
    issues.append(
        ScenarioValidationIssue(
            "error",
            code,
            f"value {value} must be positive",
            source=source,
        )
    )
