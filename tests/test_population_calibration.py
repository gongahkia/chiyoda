from __future__ import annotations

import json
from pathlib import Path

import pytest

from chiyoda.information.route_choice_calibration import (
    fit_population_demand_profile,
    load_mta_hourly_ridership,
)
from scripts.run_population_calibration import run

DATA_DIR = Path("data/calibration/population_mta_2024")
FEED = DATA_DIR / "times_sq_2024_12_31_hourly.csv"
FIT = DATA_DIR / "fit_parameters.json"
STATION = "Times Sq-42 St (N,Q,R,W,S,1,2,3,7)/42 St (A,C,E)"


def test_mta_hourly_ridership_fixture_loads_station_feed():
    observations = load_mta_hourly_ridership(FEED)

    assert len(observations) == 24
    assert {observation.station_complex for observation in observations} == {STATION}
    assert observations[0].timestamp == "2024-12-31T00:00:00"
    assert observations[-1].timestamp == "2024-12-31T23:00:00"
    assert sum(observation.ridership for observation in observations) == pytest.approx(
        90131.0
    )


def test_population_demand_fit_reports_residuals_and_scenario_population():
    observations = load_mta_hourly_ridership(FEED, station_complex=STATION)
    fit = fit_population_demand_profile(observations, target_population=240)

    assert fit.records["total"] == 24
    assert fit.source["api_id"] == "wujg-7c2s"
    assert fit.source["station_complex"] == STATION
    assert fit.scaling["observed_total_ridership"] == pytest.approx(90131.0)
    assert fit.scaling["target_population"] == 240
    assert fit.metrics["mean_absolute_residual_ridership"] == pytest.approx(
        99.86006944444449
    )
    assert fit.metrics["rmse_residual_ridership"] == pytest.approx(112.59463320471644)
    assert sum(cohort["count"] for cohort in fit.scenario_population["cohorts"]) == 240
    assert fit.hourly_profile[15]["calibrated_agents"] == 16
    assert fit.hourly_profile[15]["observed_ridership"] == pytest.approx(5997.0)


def test_population_calibration_script_matches_committed_fit():
    result = run(FEED, target_population=240)
    committed = json.loads(FIT.read_text())

    assert result["source"] == committed["source"]
    assert result["records"] == committed["records"]
    assert result["scaling"] == committed["scaling"]
    assert result["metrics"] == committed["metrics"]
    assert result["scenario_population"] == committed["scenario_population"]
