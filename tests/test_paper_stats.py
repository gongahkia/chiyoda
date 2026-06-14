from __future__ import annotations

import json

import pandas as pd

from paper.scripts import gen_stats


def test_gen_stats_formats_best_policy_as_paper_text(tmp_path, capsys):
    tables = tmp_path / "tables"
    tables.mkdir()
    (tmp_path / "metadata.json").write_text(
        json.dumps({"study_name": "Entropy_Guided_Study"}),
        encoding="utf-8",
    )
    pd.DataFrame(
        [
            {
                "record_type": "run",
                "variant_name": "static_beacon",
                "information_safety_efficiency": 0.0136,
            },
            {
                "record_type": "variant_aggregate",
                "variant_name": "static_beacon",
                "information_safety_efficiency": 0.0136,
                "harmful_convergence_index": 7.27,
            },
            {
                "record_type": "variant_aggregate",
                "variant_name": "global_broadcast",
                "information_safety_efficiency": 0.0058,
                "harmful_convergence_index": 8.28,
            },
        ]
    ).to_csv(tables / "summary.csv", index=False)
    pd.DataFrame([{"policy": "static_beacon"}]).to_csv(tables / "interventions.csv", index=False)

    assert gen_stats.main(["gen_stats.py", str(tmp_path)]) == 0

    output = capsys.readouterr().out
    assert r"\newcommand{\statStudyName}{Entropy\_Guided\_Study}" in output
    assert r"\newcommand{\statBestPolicy}{Static beacon}" in output
    assert "static\\_beacon" not in output
