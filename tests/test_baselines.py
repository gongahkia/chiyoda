from __future__ import annotations

import json
import logging
from pathlib import Path

import pytest
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.policies import (
    evaluate_baseline,
    oracle_policy_for_scenario,
    policy_from_artifact,
)
from chiyoda.scenarios.manager import ScenarioManager

DOC_PATH = Path("docs/benchmark/baselines.md")
PPO_ARTIFACT = Path("data/baselines/ppo_smoke_discrete_policy.json")


def test_shipped_ppo_artifact_contains_trained_weights():
    artifact = json.loads(PPO_ARTIFACT.read_text())
    model_path = Path(artifact["model_path"])

    assert artifact["algorithm"] == "PPO"
    assert artifact["backend"] == "stable-baselines3"
    assert artifact["trained_with_stable_baselines3"] is True
    assert artifact["policy_source"] == (
        "stable-baselines3 model prediction cached as action_index"
    )
    assert artifact["action_index"] == 1
    assert model_path.exists()
    assert model_path.stat().st_size > 0
    assert policy_from_artifact()["policy"] == "global_broadcast"


def test_oracle_policy_selects_by_scenario_risk():
    manager = ScenarioManager()
    hostile = manager.load_config("scenarios/benchmark/transit_mixed.yaml")
    crush = manager.load_config("scenarios/benchmark/v1/open_air_event_funnel.yaml")

    assert oracle_policy_for_scenario(hostile)["message_type"] == "rumor_control"
    assert oracle_policy_for_scenario(crush)["policy"] == "density_aware"
    assert oracle_policy_for_scenario({})["policy"] == "none"


def test_baseline_cli_group_is_available():
    result = CliRunner().invoke(cli, ["baseline", "--help"])
    train = CliRunner().invoke(cli, ["baseline", "train", "--help"])
    eval_result = CliRunner().invoke(cli, ["baseline", "eval", "--help"])

    assert result.exit_code == 0
    assert train.exit_code == 0
    assert eval_result.exit_code == 0
    assert "train" in result.output
    assert "eval" in result.output
    assert "--allow-fallback" in train.output
    assert "--baseline" in eval_result.output


def test_baseline_eval_reproduces_documented_scores(tmp_path):
    expected = _documented_scores()
    logger = logging.getLogger("chiyoda")
    was_disabled = logger.disabled
    logger.disabled = True
    try:
        actual = {
            "oracle": evaluate_baseline(
                baseline="oracle",
                suite="v1",
                output_dir=tmp_path / "oracle",
                policy_selector=oracle_policy_for_scenario,
            )["leaderboard"]["entries"][0],
            "ppo": evaluate_baseline(
                baseline="ppo",
                suite="v1",
                output_dir=tmp_path / "ppo",
                policy_selector=lambda scenario: policy_from_artifact(),
            )["leaderboard"]["entries"][0],
        }
    finally:
        logger.disabled = was_disabled

    for baseline, entry in actual.items():
        row = expected[baseline]
        assert entry["policy_hash"] == row["policy_hash"]
        assert entry["tier"] == row["tier"]
        assert entry["run_count"] == row["run_count"]
        assert entry["seeds_used"] == row["seeds_used"]
        assert entry["mean_score"] == pytest.approx(row["mean_score"])
        assert entry["score_ci_low"] == pytest.approx(row["score_ci_low"])
        assert entry["score_ci_high"] == pytest.approx(row["score_ci_high"])


def _documented_scores() -> dict[str, dict[str, object]]:
    rows: dict[str, dict[str, object]] = {}
    for line in DOC_PATH.read_text().splitlines():
        if not line.startswith("| "):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 9 or cells[0] in {"baseline", ":--"}:
            continue
        baseline = cells[0]
        rows[baseline] = {
            "suite": cells[1],
            "tier": cells[2],
            "policy_hash": cells[3],
            "mean_score": float(cells[4]),
            "score_ci_low": float(cells[5]),
            "score_ci_high": float(cells[6]),
            "seeds_used": [int(seed) for seed in cells[7].split(",")],
            "run_count": int(cells[8]),
        }
    return rows
