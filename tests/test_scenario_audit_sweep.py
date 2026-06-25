from __future__ import annotations

import yaml

from scripts.audit_scenarios import audit_scenario_corpus, discover_scenario_files


def test_scenario_audit_sweep_covers_corpus_without_error_level_issues():
    payload = audit_scenario_corpus("scenarios")

    assert payload["ok"] is True
    assert payload["scenario_count"] == 35
    assert payload["skipped_count"] >= 1
    assert payload["totals"]["errors"] == 0
    assert payload["totals"]["runtime_assertion_files"] >= 1
    assert all(record["ok"] for record in payload["files"])
    assert any(
        record["path"].endswith("benchmark/v1/large_station_multifloor.yaml")
        for record in payload["files"]
    )
    assert any(
        path.endswith("study_llm_extension.yaml") for path in payload["skipped_files"]
    )


def test_discover_scenario_files_classifies_layout_yamls_only():
    scenario_files, skipped_files = discover_scenario_files("scenarios")
    scenario_paths = {str(path) for path in scenario_files}
    skipped_paths = {str(path) for path in skipped_files}

    assert "scenarios/example.yaml" in scenario_paths
    assert "scenarios/example_study.yaml" in skipped_paths
    for path in scenario_files:
        payload = yaml.safe_load(path.read_text()) or {}
        scenario = payload.get("scenario", payload)
        assert isinstance(scenario, dict)
        assert "layout" in scenario
