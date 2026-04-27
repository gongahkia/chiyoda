from __future__ import annotations

from pathlib import Path

import pandas as pd

from scripts.compare_llm_claims import compare_variants_for_claim
from scripts.synthesize_llm_results import build_claim_highlights, build_policy_synthesis


def _write_policy_csv(root: Path, rows: list[dict[str, object]]) -> None:
    table_dir = root / "tables"
    table_dir.mkdir(parents=True)
    pd.DataFrame(rows).to_csv(table_dir / "llm_policy_comparison.csv", index=False)


def test_build_policy_synthesis_normalizes_study_rows(tmp_path):
    study_dir = tmp_path / "prompt"
    _write_policy_csv(
        study_dir,
        [
            {
                "variant_name": "llm_openai_safety",
                "agents_evacuated": 9.1,
                "information_safety_efficiency": 0.04,
                "harmful_convergence_index": 8.0,
                "intervention_count": 8,
                "intervention_recipients": 120,
            },
            {
                "variant_name": "static_beacon",
                "agents_evacuated": 11.0,
                "information_safety_efficiency": 0.01,
                "harmful_convergence_index": 6.0,
                "intervention_count": 32,
                "intervention_recipients": 500,
            },
        ],
    )

    synthesis = build_policy_synthesis({"prompt_objective": study_dir})

    assert set(synthesis["study"]) == {"prompt_objective"}
    assert synthesis.loc[synthesis["variant_name"] == "llm_openai_safety", "llm_provider"].iloc[0] == "openai"
    assert synthesis.loc[synthesis["variant_name"] == "static_beacon", "variant_family"].iloc[0] == "deterministic"


def test_build_claim_highlights_compares_expected_variants(tmp_path):
    study_dir = tmp_path / "prompt"
    _write_policy_csv(
        study_dir,
        [
            {
                "variant_name": "llm_openai_safety",
                "agents_evacuated": 9.1,
                "information_safety_efficiency": 0.04,
                "harmful_convergence_index": 8.0,
                "intervention_count": 8,
                "intervention_recipients": 100,
            },
            {
                "variant_name": "static_beacon",
                "agents_evacuated": 11.0,
                "information_safety_efficiency": 0.01,
                "harmful_convergence_index": 6.0,
                "intervention_count": 32,
                "intervention_recipients": 500,
            },
        ],
    )
    synthesis = build_policy_synthesis({"prompt_objective": study_dir})

    highlights = build_claim_highlights(synthesis)

    row = highlights[highlights["claim"] == "sparse_safety_vs_static"].iloc[0]
    assert row["ise_delta"] == 0.03
    assert row["hci_delta"] == 2.0
    assert row["recipient_ratio"] == 0.2


def test_compare_variants_for_claim_uses_run_level_rows():
    summary = pd.DataFrame(
        [
            {
                "record_type": "run",
                "variant_name": "static_beacon",
                "seed": 1,
                "information_safety_efficiency": 0.01,
                "harmful_convergence_index": 6.0,
                "agents_evacuated": 10,
                "intervention_recipients": 500,
            },
            {
                "record_type": "run",
                "variant_name": "static_beacon",
                "seed": 2,
                "information_safety_efficiency": 0.02,
                "harmful_convergence_index": 7.0,
                "agents_evacuated": 12,
                "intervention_recipients": 520,
            },
            {
                "record_type": "run",
                "variant_name": "llm_openai_safety",
                "seed": 1,
                "information_safety_efficiency": 0.04,
                "harmful_convergence_index": 8.0,
                "agents_evacuated": 9,
                "intervention_recipients": 120,
            },
            {
                "record_type": "run",
                "variant_name": "llm_openai_safety",
                "seed": 2,
                "information_safety_efficiency": 0.05,
                "harmful_convergence_index": 9.0,
                "agents_evacuated": 10,
                "intervention_recipients": 130,
            },
        ]
    )

    rows = compare_variants_for_claim(
        summary,
        claim="sparse_safety_vs_static",
        study="prompt_objective",
        baseline="static_beacon",
        test="llm_openai_safety",
        metrics=["information_safety_efficiency", "harmful_convergence_index"],
    )

    by_metric = {row["metric"]: row for row in rows}
    assert by_metric["information_safety_efficiency"]["delta"] == 0.03
    assert by_metric["information_safety_efficiency"]["supports_test"] is True
    assert by_metric["harmful_convergence_index"]["delta"] == 2.0
    assert by_metric["harmful_convergence_index"]["supports_test"] is False
    assert by_metric["information_safety_efficiency"]["n_paired"] == 2
