from __future__ import annotations

import pandas as pd

from scripts.summarize_llm_interventions import (
    _generation_summary,
    _policy_comparison,
    _validation_reasons,
)


def test_generation_summary_counts_cache_and_validation_status():
    llm = pd.DataFrame(
        [
            {
                "variant_name": "llm",
                "generation_provider": "openai",
                "generation_model": "gpt-test",
                "cache_status": "miss",
                "validation_status": "accepted",
                "cache_key": "a",
                "recipients": 3,
                "entropy_delta": -0.2,
                "accuracy_delta": 0.3,
            },
            {
                "variant_name": "llm",
                "generation_provider": "openai",
                "generation_model": "gpt-test",
                "cache_status": "hit",
                "validation_status": "rejected",
                "cache_key": "b",
                "recipients": 2,
                "entropy_delta": -0.1,
                "accuracy_delta": 0.1,
            },
        ]
    )

    summary = _generation_summary(llm)

    assert summary.loc[0, "events"] == 2
    assert summary.loc[0, "cache_hits"] == 1
    assert summary.loc[0, "cache_misses"] == 1
    assert summary.loc[0, "accepted"] == 1
    assert summary.loc[0, "rejected"] == 1
    assert summary.loc[0, "unique_cache_keys"] == 2


def test_validation_reasons_counts_accepted_and_rejected_reasons():
    llm = pd.DataFrame(
        [
            {
                "variant_name": "llm",
                "generation_provider": "openai",
                "validation_status": "accepted",
                "validation_reasons": "",
            },
            {
                "variant_name": "llm",
                "generation_provider": "openai",
                "validation_status": "rejected",
                "validation_reasons": "invented_exit:(9, 9);vague_guidance",
            },
        ]
    )

    reasons = _validation_reasons(llm)

    assert set(reasons["reason"]) == {"accepted", "invented_exit:(9, 9)", "vague_guidance"}


def test_policy_comparison_keeps_core_metrics():
    summary = pd.DataFrame(
        [
            {
                "record_type": "run",
                "variant_name": "llm",
                "evacuated": 1,
            },
            {
                "record_type": "aggregate",
                "variant_name": "llm",
                "evacuated": 2,
                "information_safety_efficiency": 0.1,
                "harmful_convergence_index": 3.0,
            },
        ]
    )

    comparison = _policy_comparison(summary)

    assert list(comparison["variant_name"]) == ["llm"]
    assert comparison["information_safety_efficiency"].iloc[0] == 0.1
