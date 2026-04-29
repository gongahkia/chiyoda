from __future__ import annotations

import json
from pathlib import Path

from scripts.audit_llm_cache_usage import (
    collect_cache_records,
    collect_population_calibration_records,
    summarize_cache_records,
    summarize_population_calibration_records,
    summarize_totals,
)


def _write_record(path: Path, *, accepted: bool, input_tokens: int, output_tokens: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "cache_key": path.stem,
        "message": {
            "provider": "openai",
            "model": "test-model",
            "raw_response": {
                "usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "total_tokens": input_tokens + output_tokens,
                }
            },
        },
        "validation": {
            "accepted": accepted,
            "reasons": [] if accepted else ["congested_recommendation:(1, 1)"],
        },
    }
    path.write_text(json.dumps(payload))


def test_cache_audit_summarizes_usage_and_validation(tmp_path):
    cache_root = tmp_path / "llm_cache"
    _write_record(cache_root / "study_a" / "a.json", accepted=True, input_tokens=100, output_tokens=20)
    _write_record(cache_root / "study_a" / "b.json", accepted=False, input_tokens=200, output_tokens=30)

    records = collect_cache_records(cache_root)
    summary = summarize_cache_records(
        records,
        input_usd_per_mtok=1.0,
        output_usd_per_mtok=2.0,
    )
    totals = summarize_totals(summary)

    row = summary.iloc[0]
    assert row["cache_dir"] == "study_a"
    assert row["records"] == 2
    assert row["accepted"] == 1
    assert row["rejected"] == 1
    assert row["input_tokens"] == 300
    assert row["output_tokens"] == 50
    assert round(row["estimated_usd"], 6) == 0.0004
    assert totals.iloc[0]["total_tokens"] == 350


def test_cache_audit_summarizes_generated_population_calibration_records(tmp_path):
    cache_root = tmp_path / "population_calibration_cache"
    path = cache_root / "study_a" / "population.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "cache_key": "population",
                "request": {
                    "scenario_name": "generated_population_smoke",
                    "allowed_targets": ["parameter_priors", "scenario_metadata"],
                },
                "calibration": {
                    "provider": "openai",
                    "model": "test-model",
                    "confidence": 0.72,
                    "parameter_priors": {"visitors": {"base_speed": 1.2}},
                    "scenario_metadata": {"status": "generated"},
                    "raw_response": {
                        "usage": {
                            "input_tokens": 300,
                            "output_tokens": 80,
                            "total_tokens": 380,
                        }
                    },
                },
                "validation": {
                    "accepted": True,
                    "reasons": [],
                },
                "application": {
                    "applied_targets": [
                        "parameter_priors:visitors.base_speed",
                        "scenario_metadata:status",
                    ],
                    "skipped": ["parameter_priors:visitors.calmness:existing_value"],
                },
            }
        )
    )

    records = collect_population_calibration_records(cache_root)
    summary = summarize_population_calibration_records(
        records,
        input_usd_per_mtok=1.0,
        output_usd_per_mtok=2.0,
    )

    record = records.iloc[0]
    row = summary.iloc[0]
    assert record["scenario_name"] == "generated_population_smoke"
    assert record["proposed_targets"] == "parameter_priors;scenario_metadata"
    assert "existing_value" in record["skipped_overwrite_attempts"]
    assert row["records"] == 1
    assert row["accepted"] == 1
    assert row["records_with_usage"] == 1
    assert row["total_tokens"] == 380
    assert round(row["estimated_usd"], 6) == 0.00046
