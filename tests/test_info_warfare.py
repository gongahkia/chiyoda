from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.cli import cli
from chiyoda.information.warfare import BeliefRevisionConfig, BeliefRevisionModel
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.runner import _materialize_variants, load_study_config
from chiyoda.studies.schema import AdversarialStudyConfig, StudyConfig


def _hostile_scenario() -> dict:
    return {
        "name": "hostile_decoy_regression",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXX\nX@..EX\nX...XX\nXXXXXX",
                }
            ]
        },
        "population": {
            "total": 1,
            "cohorts": [{"name": "baseline", "count": 1, "familiarity": 0.0}],
        },
        "information": {
            "mode": "asymmetric",
            "observation_radius": 10.0,
            "gossip_radius": 0.0,
        },
        "simulation": {"max_steps": 3, "random_seed": 7},
        "hostile_channels": [
            {
                "id": "decoy",
                "channel_type": "gossip",
                "objective": "decoy-exit",
                "budget": 1,
                "plausibility": 0.8,
                "claimed_exit": {"floor": "0", "x": 2, "y": 1},
            }
        ],
    }


def test_belief_revision_model_updates_against_bayesian_example():
    model = BeliefRevisionModel(
        BeliefRevisionConfig(prior_alpha=2.0, prior_beta=2.0, forgetting_factor=1.0)
    )

    assert model.source_credibility("source") == 0.5
    assert model.update_source("source", True) == 0.6
    assert model.update_source("source", False) == 0.5


def test_hostile_channel_records_provenance_and_adversarial_metrics():
    simulation = ScenarioManager().build_simulation(_hostile_scenario())
    simulation.run()

    assert len(simulation.hostile_channel_events) == 1
    agent = simulation.agents[0]
    record = agent.belief_revision.provenance[0]
    assert record.source_id == "attacker"
    assert record.claimed_exit == ("0", 2, 1)
    assert record.observed_outcome is False
    assert record.credibility_after < 0.5

    metrics = SimulationAnalytics().calculate_performance_metrics(simulation)
    assert metrics["hostile_channel_event_count"] == 1
    assert metrics["hostile_channel_recipients"] == 1
    assert (
        metrics["harmful_convergence_index_induced"]
        >= metrics["harmful_convergence_index_accidental"]
    )
    assert (
        metrics["information_safety_efficiency_adversarial"]
        <= metrics["information_safety_efficiency"]
    )


def test_adversarial_study_schema_materializes_budget_defender_pairings():
    config = StudyConfig(
        name="adv",
        scenario_file="scenarios/station_baseline.yaml",
        adversarial=AdversarialStudyConfig(
            attacker_budget=[1, 3],
            defender_policy=["entropy_targeted"],
        ),
    )

    variants = _materialize_variants(config)

    assert [variant.name for variant in variants] == [
        "adv_stackelberg__budget_1__defender_entropy_targeted",
        "adv_stackelberg__budget_3__defender_entropy_targeted",
    ]
    assert variants[0].scenario_overrides["hostile_channels"][0]["budget"] == 1
    assert (
        variants[0].scenario_overrides["interventions"]["policy"] == "entropy_targeted"
    )


def test_red_team_cli_runs_hostile_channel(tmp_path):
    scenario_file = tmp_path / "scenario.yaml"
    scenario_file.write_text(
        """
scenario:
  name: cli_red_team
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |
          XXXXXX
          X@..EX
          X...XX
          XXXXXX
  population:
    total: 1
    cohorts:
      - name: baseline
        count: 1
        familiarity: 0.0
  information:
    mode: asymmetric
    observation_radius: 10.0
  simulation:
    max_steps: 2
    random_seed: 5
"""
    )

    result = CliRunner().invoke(
        cli,
        ["red-team", str(scenario_file), "--budget", "1", "--objective", "decoy-exit"],
    )
    payload = json.loads(result.output)

    assert result.exit_code == 0
    assert payload["hostile_events"] == 1
    assert payload["metrics"]["hostile_channel_recipients"] == 1


def test_llm_hostile_channel_uses_template_cache_and_audit(tmp_path):
    scenario = _hostile_scenario()
    cache_path = tmp_path / "llm_hostile_cache"
    scenario["hostile_channels"][0].update(
        {
            "llm_claims_enabled": True,
            "llm_provider": "template",
            "llm_model": "template",
            "llm_cache_path": str(cache_path),
            "llm_cache_mode": "cache_first",
            "llm_store_cache": True,
            "llm_prompt_style": "hostile_red_team",
            "llm_max_calls_per_run": 1,
        }
    )

    simulation = ScenarioManager().build_simulation(scenario)
    simulation.run()

    assert len(simulation.hostile_channel_events) == 1
    event = simulation.hostile_channel_events[0]
    assert event.claimed_exit == ("0", 2, 1)
    assert list(cache_path.glob("*.json"))
    audit = simulation.llm_call_audit[0]
    assert audit["surface"] == "hostile_channel"
    assert audit["provider"] == "deterministic"
    assert audit["cache_status"] == "miss"
    assert audit["validation_status"] == "accepted"
    assert audit["prompt_style"] == "hostile_red_team"


def test_llm_red_team_study_declares_template_and_replay_variants():
    config = load_study_config("scenarios/study_llm_red_team.yaml")
    variants = _materialize_variants(config)
    by_name = {variant.name: variant for variant in variants}

    template = by_name["llm_template_decoy_exit"].scenario_overrides[
        "hostile_channels"
    ][0]
    replay = by_name["llm_replay_decoy_exit"].scenario_overrides["hostile_channels"][0]

    assert template["llm_provider"] == "template"
    assert template["llm_store_cache"] is True
    assert replay["llm_provider"] == "replay"
    assert replay["llm_cache_mode"] == "replay_only"
    assert replay["llm_cache_path"] == template["llm_cache_path"]
