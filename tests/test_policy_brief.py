from __future__ import annotations

import json

from click.testing import CliRunner
import pandas as pd

from chiyoda.analysis.reports import export_policy_brief
from chiyoda.cli import cli
from chiyoda.studies.models import ComparisonResult, StudyBundle


def test_policy_brief_is_short_and_includes_uncertainty(tmp_path):
    result = _comparison_result()

    path = export_policy_brief(result, tmp_path)
    text = path.read_text()

    assert path.name == "policy_brief.md"
    assert "Recommended policy" in text
    assert "95% uncertainty" in text
    assert "Attacker-induced harm" in text
    assert len(text.split()) < 500


def test_compare_cli_exports_policy_brief_by_default(tmp_path):
    baseline = _study_bundle("baseline", 30.0, 0.4, 0.2)
    variant = _study_bundle("variant", 25.0, 0.3, 0.1)
    baseline_dir = tmp_path / "baseline"
    variant_dir = tmp_path / "variant"
    out_dir = tmp_path / "comparison"
    baseline.export(baseline_dir)
    variant.export(variant_dir)

    result = CliRunner().invoke(
        cli,
        ["compare", str(baseline_dir), str(variant_dir), "-o", str(out_dir)],
    )

    assert result.exit_code == 0, result.output
    assert (out_dir / "policy_brief.md").exists()
    assert (out_dir / "figures" / "06_scenario_comparison.png").exists()
    assert "LLM Provider Cost" in (out_dir / "policy_brief.md").read_text()


def test_study_bundle_export_includes_llm_cost_report(tmp_path):
    bundle = _study_bundle("llm", 25.0, 0.3, 0.1)

    bundle.export(tmp_path, table_formats=("csv",))
    metadata = json.loads((tmp_path / "metadata.json").read_text())
    report = metadata["llm_cost_report"]

    assert report["total"]["calls"] == 1
    assert report["total"]["estimated_total_tokens"] == 125
    assert report["total"]["estimated_usd"] == 0.0002
    assert report["by_provider_model"][0]["provider"] == "openai"
    assert report["by_provider_model"][0]["model"] == "gpt-test"


def _comparison_result() -> ComparisonResult:
    return ComparisonResult(
        metadata={},
        summary=pd.DataFrame(
            [
                _summary_row("baseline", 30.0, 0.4, 0.2),
                _summary_row("variant", 25.0, 0.3, 0.1),
            ]
        ),
        timeseries=_steps(),
        metrics=pd.DataFrame(
            [
                {
                    "metric": "harmful_convergence_index_induced",
                    "baseline_value": 0.2,
                    "variant_value": 0.1,
                    "delta": -0.1,
                    "pct_change": -50.0,
                }
            ]
        ),
    )


def _study_bundle(name: str, total_time: float, exposure: float, harm: float) -> StudyBundle:
    return StudyBundle(
        metadata={
            "study_name": name,
            "export_config": {
                "formats": ["png"],
                "table_formats": ["csv"],
                "include_figures": True,
            },
        },
        summary=pd.DataFrame([_summary_row("study_total", total_time, exposure, harm)]),
        steps=_steps(),
        cells=pd.DataFrame(),
        agent_steps=pd.DataFrame(),
        agents=pd.DataFrame(),
        bottlenecks=pd.DataFrame(),
        dwell_samples=pd.DataFrame(),
        exits=pd.DataFrame(),
        hazards=pd.DataFrame(),
        llm_calls=pd.DataFrame(
            [
                {
                    "provider": "openai",
                    "model": "gpt-test",
                    "cache_status": "miss",
                    "estimated_input_tokens": 100,
                    "estimated_output_tokens": 25,
                    "estimated_total_tokens": 125,
                    "estimated_usd": 0.0002,
                    "raw_input_tokens": 90,
                    "raw_output_tokens": 20,
                    "raw_total_tokens": 110,
                }
            ]
        ),
    )


def _summary_row(series: str, total_time: float, exposure: float, harm: float) -> dict:
    return {
        "study_name": series,
        "scenario_name": "brief",
        "variant_name": series,
        "record_type": "study_aggregate",
        "series": series,
        "run_count": 3,
        "total_time_s": total_time,
        "total_time_s_std": 1.5,
        "mean_hazard_exposure": exposure,
        "mean_hazard_exposure_std": 0.05,
        "harmful_convergence_index_induced": harm,
        "harmful_convergence_index_induced_std": 0.02,
    }


def _steps() -> pd.DataFrame:
    return pd.DataFrame(
        [
            {
                "time_s": 0.0,
                "evacuated_total": 0,
                "mean_speed": 0.0,
                "mean_density": 0.1,
            },
            {
                "time_s": 1.0,
                "evacuated_total": 1,
                "mean_speed": 1.0,
                "mean_density": 0.2,
            },
        ]
    )
