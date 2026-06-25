from __future__ import annotations

import random
import statistics

import pytest

from chiyoda.information.decisions import empirical_distribution
from chiyoda.scenarios.manager import ScenarioManager


def _scenario(prior: str) -> dict:
    return {
        "scenario": {
            "name": f"prior_{prior}",
            "layout": {
                "floors": [{"id": "0", "z": 0.0, "text": "XXXXX\nX@.EX\nXXXXX"}]
            },
            "population": {"total": 1},
            "simulation": {"max_steps": 1, "dt": 1.0, "random_seed": 3},
            "behavior": {
                "milling_time_dist": prior,
                "compliance_dist": prior,
            },
        }
    }["scenario"]


def test_cb_fr_2024_sampled_moments_match_stored_distribution():
    dist = empirical_distribution("cb_fr_2024", "milling_time")
    rng = random.Random(11)
    samples = [dist.sample(rng) for _ in range(50000)]

    assert statistics.fmean(samples) == pytest.approx(dist.mean, abs=20.0)
    assert statistics.pvariance(samples) == pytest.approx(dist.variance, rel=0.08)


def test_cb_fr_2024_compliance_sampled_moments_match_stored_distribution():
    dist = empirical_distribution("cb_fr_2024", "compliance")
    rng = random.Random(12)
    samples = [dist.sample(rng) for _ in range(50000)]

    assert statistics.fmean(samples) == pytest.approx(dist.mean, abs=0.01)
    assert statistics.pvariance(samples) == pytest.approx(dist.variance, rel=0.08)


def test_named_priors_are_selectable_from_yaml():
    for prior in ("cb_fr_2024", "wea_us_default", "synthetic_baseline"):
        sim = ScenarioManager().build_simulation(_scenario(prior))
        agent = sim.agents[0]
        assert agent.milling_time_dist == prior
        assert agent.compliance_dist == prior


def test_cb_fr_2024_yaml_applies_release_delay():
    sim = ScenarioManager().build_simulation(_scenario("cb_fr_2024"))
    agent = sim.agents[0]

    assert agent.milling_time_s > 0.0
    assert agent.release_step == int(agent.milling_time_s)
    assert isinstance(agent.will_comply_with_alert, bool)
