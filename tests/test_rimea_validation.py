from __future__ import annotations

from click.testing import CliRunner

from chiyoda.analysis.external_validation import (
    run_rimea_validation_scenarios,
    summarize_rimea_validation_runs,
)
from chiyoda.cli import cli
from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import validate_scenario_file

RIMEA_CASES = [
    f"scenarios/validation_rimea_{case:02d}.yaml" for case in range(1, 11)
]


def test_rimea_validation_scenarios_are_static_valid_and_assertable():
    manager = ScenarioManager()
    for scenario_file in RIMEA_CASES:
        validation = validate_scenario_file(scenario_file)
        scenario = manager.load_config(scenario_file)
        simulation = manager.build_simulation(scenario)
        simulation.run()
        assertions = evaluate_scenario_assertions(scenario, simulation)

        assert not validation.has_errors, scenario_file
        assert assertions.ok, scenario_file


def test_rimea_assert_scenario_cli_runs_pr_subset():
    runner = CliRunner()
    for scenario_file in (
        "scenarios/validation_rimea_01.yaml",
        "scenarios/validation_rimea_04.yaml",
        "scenarios/validation_rimea_06.yaml",
        "scenarios/validation_rimea_07.yaml",
    ):
        result = runner.invoke(cli, ["assert-scenario", scenario_file, "--json"])

        assert result.exit_code == 0, result.output
        assert '"ok": true' in result.output


def test_rimea_validation_summary_uses_five_seed_ci():
    runs = run_rimea_validation_scenarios(
        ["scenarios/validation_rimea_01.yaml"], seeds=(42, 43, 44, 45, 46)
    )
    summary = summarize_rimea_validation_runs(runs)

    assert len(runs) == 5
    assert summary.iloc[0]["seed_count"] == 5
    assert summary.iloc[0]["pass_count"] == 5
    assert summary.iloc[0]["evacuation_time_ci95_s"] >= 0.0
