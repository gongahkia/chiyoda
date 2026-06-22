from __future__ import annotations

import copy

import numpy as np

from chiyoda.information.warfare import (
    HostileChannel,
    HostileChannelConfig,
    _normalize_persona,
    _persona_match,
)
from chiyoda.scenarios.manager import ScenarioManager


def _persona_scenario() -> dict:
    return {
        "name": "hostile_persona_regression",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXXXX\nX@.@.EX\nX.....X\nXXXXXXX",
                }
            ]
        },
        "population": {
            "total": 2,
            "cohorts": [
                {"name": "tourist", "count": 1, "familiarity": 0.0},
                {"name": "commuter", "count": 1, "familiarity": 0.9},
            ],
        },
        "information": {
            "mode": "asymmetric",
            "observation_radius": 10.0,
            "gossip_radius": 0.0,
        },
        "simulation": {"max_steps": 3, "random_seed": 13},
        "hostile_channels": [
            {
                "id": "decoy_tourist_only",
                "channel_type": "gossip",
                "objective": "decoy-exit",
                "budget": 1,
                "plausibility": 0.85,
                "claimed_exit": {"floor": "0", "x": 2, "y": 1},
                "target_persona": {"cohort": "tourist"},
            }
        ],
    }


def _belief_vector(agent) -> np.ndarray:
    beliefs = agent.beliefs
    extras: list[float] = []
    for attr in ("general_danger_level", "preferred_exit_x", "preferred_exit_y"):
        if hasattr(beliefs, attr):
            value = getattr(beliefs, attr)
            if callable(value):
                continue
            try:
                extras.append(float(value))
            except (TypeError, ValueError):
                continue
    return np.array(extras or [0.0], dtype=float)


def test_persona_targeted_decoy_only_affects_matched_cohort():
    manager = ScenarioManager()
    targeted_sim = manager.build_simulation(_persona_scenario())

    baseline_config = copy.deepcopy(_persona_scenario())
    baseline_config["hostile_channels"] = []
    baseline_sim = manager.build_simulation(baseline_config)

    baseline_sim.run()
    targeted_sim.run()

    assert len(targeted_sim.hostile_channel_events) >= 1
    event = targeted_sim.hostile_channel_events[0]
    assert event.recipients == 1

    targeted_by_cohort = {a.cohort_name: a for a in targeted_sim.agents}
    baseline_by_cohort = {a.cohort_name: a for a in baseline_sim.agents}

    tourist_target = targeted_by_cohort["tourist"].belief_revision.provenance
    commuter_target = targeted_by_cohort["commuter"].belief_revision.provenance
    assert any(
        record.source_id == "attacker" for record in tourist_target
    ), "tourist should receive the persona-targeted claim"
    assert not any(
        record.source_id == "attacker" for record in commuter_target
    ), "commuter should not be targeted by a cohort='tourist' persona"

    commuter_diff = np.linalg.norm(
        _belief_vector(targeted_by_cohort["commuter"])
        - _belief_vector(baseline_by_cohort["commuter"])
    )
    assert commuter_diff < 1e-6


def test_persona_normalizer_handles_subset_fields():
    assert _normalize_persona({}) is None
    assert _normalize_persona({"cohort": "tourist"}) == {"cohort": "tourist"}
    assert _normalize_persona({"mobility": "Wheelchair", "age_band": "Senior"}) == {
        "mobility": "wheelchair",
        "age_band": "senior",
    }


def test_persona_match_filters_by_mobility_and_age():
    class Agent:
        cohort_name = "commuter"
        mobility_class = "standard"
        age_band = "adult"

    assert _persona_match(Agent(), {"cohort": "commuter"}) is True
    assert _persona_match(Agent(), {"cohort": "tourist"}) is False
    assert _persona_match(Agent(), {"mobility": "wheelchair"}) is False
    assert _persona_match(Agent(), {"age_band": "adult"}) is True


def test_hostile_channel_config_accepts_persona_payload():
    cfg = HostileChannelConfig.from_mapping(
        {
            "id": "h",
            "target_persona": {"cohort": "tourist", "mobility": "standard"},
        }
    )
    assert cfg.target_persona == {"cohort": "tourist", "mobility": "standard"}

    channel = HostileChannel(cfg)
    assert channel.config.target_persona is not None
