from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.cli import cli
from chiyoda.information.warfare import BeliefRevisionConfig, BeliefRevisionModel
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies.runner import _materialize_variants
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
        "information": {"mode": "asymmetric", "observation_radius": 10.0, "gossip_radius": 0.0},
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
    assert metrics["harmful_convergence_index_induced"] >= metrics["harmful_convergence_index_accidental"]
    assert metrics["information_safety_efficiency_adversarial"] <= metrics["information_safety_efficiency"]


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
    assert variants[0].scenario_overrides["interventions"]["policy"] == "entropy_targeted"


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
