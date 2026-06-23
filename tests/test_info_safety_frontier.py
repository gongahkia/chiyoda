from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.analysis.info_safety_frontier import (
    BORDERLINE,
    HARMFUL,
    SAFE,
    check_info_safety_scenario,
)
from chiyoda.cli import cli


def test_info_safety_check_station_baseline_is_tagged():
    verdict = check_info_safety_scenario("scenarios/station_baseline.yaml")

    assert verdict.verdict in {SAFE, BORDERLINE}
    assert "station_baseline" in verdict.tags
    assert verdict.entropy_reduction_potential > 0.0


def test_info_safety_check_station_sarin_has_exposure_pressure():
    verdict = check_info_safety_scenario("scenarios/station_sarin.yaml")

    assert verdict.verdict in {BORDERLINE, HARMFUL}
    assert "station_sarin" in verdict.tags
    assert verdict.exposure_pressure > 0.0


def test_info_safety_check_transit_mixed_has_hostile_reason():
    verdict = check_info_safety_scenario("scenarios/benchmark/transit_mixed.yaml")

    assert verdict.verdict in {BORDERLINE, HARMFUL}
    assert "transit_mixed" in verdict.tags
    assert "hostile_channel_convergence" in verdict.reasons


def test_info_safety_cli_json_verdict():
    result = CliRunner().invoke(
        cli,
        [
            "info-safety-check",
            "scenarios/benchmark/transit_mixed.yaml",
            "--json",
        ],
    )

    assert result.exit_code == 0
    payload = json.loads(result.output)
    assert payload["verdict"] in {SAFE, BORDERLINE, HARMFUL}
    assert "reasons" in payload
    assert "transit_mixed" in payload["tags"]
