from __future__ import annotations

from pathlib import Path

import pandas as pd

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
