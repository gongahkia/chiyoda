from __future__ import annotations

import pandas as pd
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.information.llm import (
    LLM_AUDIT_CHAIN_HASH,
    LLM_AUDIT_CHAIN_PREV_HASH,
    LLM_AUDIT_GENESIS,
    verify_llm_audit_chain,
)
from chiyoda.studies.models import StudyBundle


def _bundle() -> StudyBundle:
    return StudyBundle(
        metadata={"name": "llm_replay_audit"},
        summary=pd.DataFrame([{"record_type": "run"}]),
        steps=pd.DataFrame(),
        cells=pd.DataFrame(),
        agent_steps=pd.DataFrame(),
        agents=pd.DataFrame(),
        bottlenecks=pd.DataFrame(),
        dwell_samples=pd.DataFrame(),
        exits=pd.DataFrame(),
        hazards=pd.DataFrame(),
        measurements=pd.DataFrame(),
        gossip=pd.DataFrame(),
        interventions=pd.DataFrame(),
        llm_decisions=pd.DataFrame(),
        llm_calls=pd.DataFrame(
            [
                {
                    "study_name": "llm_replay_audit",
                    "scenario_name": "scenario",
                    "variant_name": "base",
                    "seed": 1,
                    "run_id": "base:1",
                    "step": 0,
                    "surface": "intervention",
                    "provider": "deterministic",
                    "model": "template",
                    "cache_key": "abc",
                    "cache_status": "miss",
                    "validation_status": "accepted",
                },
                {
                    "study_name": "llm_replay_audit",
                    "scenario_name": "scenario",
                    "variant_name": "base",
                    "seed": 1,
                    "run_id": "base:1",
                    "step": 1,
                    "surface": "intervention",
                    "provider": "deterministic",
                    "model": "template",
                    "cache_key": "def",
                    "cache_status": "hit",
                    "validation_status": "accepted",
                },
            ]
        ),
    )


def test_llm_calls_export_contains_valid_hash_chain(tmp_path):
    _bundle().export(tmp_path, table_formats=("csv",))

    loaded = StudyBundle.load(tmp_path)
    result = verify_llm_audit_chain(loaded.llm_calls)

    assert result.ok
    assert result.row_count == 2
    assert {LLM_AUDIT_CHAIN_PREV_HASH, LLM_AUDIT_CHAIN_HASH}.issubset(
        loaded.llm_calls.columns
    )
    assert loaded.llm_calls.loc[0, LLM_AUDIT_CHAIN_PREV_HASH] == LLM_AUDIT_GENESIS


def test_llm_calls_audit_cli_reports_valid_chain(tmp_path):
    _bundle().export(tmp_path, table_formats=("csv",))

    result = CliRunner().invoke(cli, ["audit", "llm_calls", str(tmp_path)])

    assert result.exit_code == 0
    assert "OK: llm_calls audit rows=2" in result.output


def test_llm_calls_audit_cli_fails_on_corrupted_row(tmp_path):
    _bundle().export(tmp_path, table_formats=("csv",))
    table = tmp_path / "tables" / "llm_calls.csv"
    frame = pd.read_csv(table)
    frame.loc[1, "validation_status"] = "rejected"
    frame.to_csv(table, index=False)

    result = CliRunner().invoke(cli, ["audit", "llm_calls", str(tmp_path)])

    assert result.exit_code == 1
    assert "ERROR: llm_calls audit rows=2 row=1 reason=row_hash_mismatch" in result.output
