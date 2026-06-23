from __future__ import annotations

from pathlib import Path

import pytest

from chiyoda.studies.runner import run_study

BASELINE_SCENARIOS = (
    "scenarios/benchmark/transit_cbrn.yaml",
    "scenarios/transit_shooter.yaml",
    "scenarios/benchmark/transit_mixed.yaml",
)


@pytest.mark.parametrize("scenario_file", BASELINE_SCENARIOS)
def test_same_seed_exports_byte_identical_telemetry_tables(tmp_path, scenario_file):
    first = run_study(scenario_file)
    second = run_study(scenario_file)
    first_dir = tmp_path / "first"
    second_dir = tmp_path / "second"
    first.export(first_dir, table_formats=("csv",))
    second.export(second_dir, table_formats=("csv",))

    first_tables = sorted((first_dir / "tables").glob("*.csv"))
    second_tables = sorted((second_dir / "tables").glob("*.csv"))
    assert [path.name for path in first_tables] == [path.name for path in second_tables]

    for first_table in first_tables:
        second_table = second_dir / "tables" / first_table.name
        assert first_table.read_bytes() == second_table.read_bytes(), first_table.name


def test_determinism_covers_three_baseline_scenarios():
    assert len(BASELINE_SCENARIOS) >= 3
    assert all(Path(path).exists() for path in BASELINE_SCENARIOS)
