from __future__ import annotations

import hashlib
from pathlib import Path

import pytest

from chiyoda.information.route_choice_calibration import (
    fit_route_choice_priors,
    load_figshare_route_choice_records,
)

DATA_DIR = Path("data/calibration/route_choice_2025")
ARCHIVE = DATA_DIR / "Snopkova_Isovists.zip"


def test_route_choice_archive_checksum_matches_figshare_metadata():
    digest = hashlib.md5(ARCHIVE.read_bytes()).hexdigest()

    assert digest == "2d53020e8aac54ed270205534623742d"


def test_route_choice_fit_beats_held_out_baseline():
    records = load_figshare_route_choice_records(ARCHIVE)
    fit = fit_route_choice_priors(records)

    assert fit.records["total"] == 4045
    assert fit.records["participants"] == 208
    assert fit.metrics["test_log_loss_improvement"] >= 0.20
    assert fit.metrics["test_log_loss"] < fit.metrics["test_baseline_log_loss"]
    assert fit.metrics["test_accuracy"] == pytest.approx(0.8447, abs=0.002)
    assert set(fit.priors) == {"familiarity", "herding", "exit_affinity"}
    assert all(0.0 <= value <= 1.0 for value in fit.priors.values())
