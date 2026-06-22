from __future__ import annotations

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.studies.runner import run_study
from chiyoda.studies.schema import StudyConfig, StudyVariant


def test_run_study_parallel_jobs_preserves_seed_order(tmp_path):
    scenario_file = _write_parallel_scenario(tmp_path)
    config = StudyConfig(
        name="parallel_order",
        scenario_file=str(scenario_file),
        seeds=[3, 1],
        jobs=2,
        variants=[StudyVariant(name="base"), StudyVariant(name="again")],
    )

    bundle = run_study(config)

    assert [run["run_id"] for run in bundle.metadata["runs"]] == [
        "base__seed_3__run_1",
        "base__seed_1__run_2",
        "again__seed_3__run_3",
        "again__seed_1__run_4",
    ]


def test_sweep_cli_accepts_jobs(tmp_path):
    scenario_file = _write_parallel_scenario(tmp_path)
    study_file = tmp_path / "study.yaml"
    output_dir = tmp_path / "out"
    study_file.write_text(
        f"""
study:
  name: parallel_cli
  scenario_file: "{scenario_file}"
  seeds: [2, 4]
  variants:
    - name: base
  export:
    include_figures: false
"""
    )

    result = CliRunner().invoke(
        cli, ["sweep", str(study_file), "-o", str(output_dir), "--jobs", "2"]
    )

    assert result.exit_code == 0
    assert (output_dir / "metadata.json").exists()


def _write_parallel_scenario(tmp_path):
    scenario_file = tmp_path / "scenario.yaml"
    scenario_file.write_text(
        """
scenario:
  name: parallel_fixture
  layout:
    floors:
      - id: "0"
        z: 0.0
        text: |
          XXXXX
          X@.EX
          XXXXX
  population:
    total: 1
  simulation:
    max_steps: 1
"""
    )
    return scenario_file
