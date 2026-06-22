"""
Test suite for ITED crowd dynamics framework.

Tests information layer, cognitive agents, multi-hazard physics,
social force model, and full simulation integration.
"""

from __future__ import annotations

from copy import deepcopy

import numpy as np
import pytest

from chiyoda.agents.behaviors import BehaviorModel
from chiyoda.agents.commuter import Commuter
from chiyoda.agents.responder import FirstResponder
from chiyoda.core.simulation import Simulation, SimulationConfig
from chiyoda.environment.exits import Exit
from chiyoda.environment.hazards import Hazard
from chiyoda.environment.layout import Layout
from chiyoda.information.entropy import (
    agent_entropy,
    global_entropy,
)
from chiyoda.information.field import (
    BeliefVector,
    ExitBelief,
    HazardBelief,
    InformationField,
)
from chiyoda.information.interventions import create_intervention_policy
from chiyoda.information.propagation import GossipConfig, GossipModel
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.navigation.social_force import adjusted_step
from chiyoda.navigation.spatial_index import SpatialIndex


def _agent_pos(layout: Layout, cell) -> np.ndarray:
    return np.array(layout.world_position(cell), dtype=float)


def _test_point3(value) -> np.ndarray:
    if len(value) >= 3 and not isinstance(value[0], str):
        return np.array([float(value[0]), float(value[1]), float(value[2])])
    return np.array([float(value[0]), float(value[1]), 0.0])


def _scalar_observe_reference(
    field: InformationField,
    agent_beliefs: BeliefVector,
    agent_pos: tuple[float, ...],
    vision_radius: float,
    exits: list[tuple],
    hazards: list[Hazard],
    current_step: int,
) -> None:
    agent_point = _test_point3(agent_pos)
    for exit_pos in exits:
        exit_point = field.exit_world_positions[tuple(exit_pos)]
        if (
            float(np.linalg.norm(agent_point - _test_point3(exit_point)))
            <= vision_radius
        ):
            agent_beliefs.exit_beliefs[exit_pos] = ExitBelief(
                position=exit_pos,
                exists_prob=1.0,
                congestion_est=0.0,
                freshness=0.0,
                source_credibility=1.0,
                hop_count=0,
            )

    for hazard in hazards:
        h_pos = tuple(float(value) for value in hazard.pos)
        if float(np.linalg.norm(agent_point - _test_point3(h_pos))) <= vision_radius:
            updated = False
            for hb in agent_beliefs.hazard_beliefs:
                hb_dist = float(
                    np.linalg.norm(_test_point3(hb.position) - _test_point3(h_pos))
                )
                if hb_dist < 2.0:
                    hb.severity_est = float(hazard.severity)
                    hb.radius_est = float(hazard.radius)
                    hb.freshness = 0.0
                    hb.source_credibility = 1.0
                    hb.hop_count = 0
                    updated = True
                    break
            if not updated:
                agent_beliefs.hazard_beliefs.append(
                    HazardBelief(
                        position=h_pos,
                        severity_est=float(hazard.severity),
                        radius_est=float(hazard.radius),
                        freshness=0.0,
                        source_credibility=1.0,
                        hop_count=0,
                    )
                )
    agent_beliefs.last_update_step = current_step


def _assert_beliefs_close(got: BeliefVector, want: BeliefVector) -> None:
    assert got.last_update_step == want.last_update_step
    assert got.exit_beliefs.keys() == want.exit_beliefs.keys()
    for key, got_exit in got.exit_beliefs.items():
        want_exit = want.exit_beliefs[key]
        assert got_exit.exists_prob == pytest.approx(want_exit.exists_prob)
        assert got_exit.congestion_est == pytest.approx(want_exit.congestion_est)
        assert got_exit.freshness == pytest.approx(want_exit.freshness)
        assert got_exit.source_credibility == pytest.approx(
            want_exit.source_credibility
        )
        assert got_exit.hop_count == want_exit.hop_count
    assert len(got.hazard_beliefs) == len(want.hazard_beliefs)
    for got_hazard, want_hazard in zip(
        got.hazard_beliefs, want.hazard_beliefs, strict=False
    ):
        assert got_hazard.position == want_hazard.position
        assert got_hazard.severity_est == pytest.approx(want_hazard.severity_est)
        assert got_hazard.radius_est == pytest.approx(want_hazard.radius_est)
        assert got_hazard.freshness == pytest.approx(want_hazard.freshness)
        assert got_hazard.source_credibility == pytest.approx(
            want_hazard.source_credibility
        )
        assert got_hazard.hop_count == want_hazard.hop_count


# -- Information Layer Tests --


class TestBeliefVector:
    def test_empty_beliefs(self):
        b = BeliefVector()
        assert b.known_exits() == []
        assert b.best_exit() is None
        assert b.perceived_hazard_at((5.0, 5.0)) == 0.0

    def test_known_exits(self):
        b = BeliefVector()
        from chiyoda.information.field import ExitBelief

        b.exit_beliefs[(10, 5)] = ExitBelief(position=(10, 5), exists_prob=0.9)
        b.exit_beliefs[(20, 15)] = ExitBelief(position=(20, 15), exists_prob=0.3)
        assert b.known_exits() == [(10, 5)]
        assert b.best_exit() == (10, 5)


class TestInformationField:
    def test_create_beliefs_perfect(self):
        field = InformationField(30, 20)
        field.set_ground_truth([(5, 0), (25, 19)])
        b = field.create_agent_beliefs((15.0, 10.0), familiarity=1.0)
        assert len(b.known_exits()) == 2

    def test_create_beliefs_unfamiliar(self):
        np.random.seed(42)
        field = InformationField(30, 20)
        field.set_ground_truth([(5, 0), (25, 19)])
        b = field.create_agent_beliefs((15.0, 10.0), familiarity=0.0)
        assert len(b.exit_beliefs) <= 2  # may know 0, 1, or 2

    def test_decay(self):
        field = InformationField(30, 20, decay_rate=0.5)
        b = BeliefVector()

        b.exit_beliefs[(5, 0)] = ExitBelief(
            position=(5, 0), exists_prob=1.0, freshness=0.0
        )
        field.decay_beliefs(b, dt=1.0)
        assert b.exit_beliefs[(5, 0)].freshness > 0.0

    def test_decay_batch_matches_scalar_reference(self):
        field = InformationField(30, 20, decay_rate=0.25)
        batch = [
            BeliefVector(
                exit_beliefs={
                    (5, 0): ExitBelief(position=(5, 0), exists_prob=0.8, freshness=0.1),
                    (25, 19): ExitBelief(
                        position=(25, 19), exists_prob=0.2, freshness=0.9
                    ),
                },
                hazard_beliefs=[
                    HazardBelief(
                        position=(3.0, 4.0),
                        severity_est=0.7,
                        radius_est=2.0,
                        freshness=0.2,
                    )
                ],
                information_age_s=2.0,
            ),
            BeliefVector(
                exit_beliefs={
                    (12, 2): ExitBelief(
                        position=(12, 2), exists_prob=0.01, freshness=0.99
                    )
                },
                hazard_beliefs=[
                    HazardBelief(
                        position=(8.0, 1.0),
                        severity_est=0.3,
                        radius_est=1.0,
                        freshness=0.8,
                    )
                ],
                information_age_s=4.0,
            ),
        ]
        expected = deepcopy(batch)
        dt = 1.5
        decay = field.decay_rate * dt

        for beliefs in expected:
            for eb in beliefs.exit_beliefs.values():
                eb.freshness = min(1.0, eb.freshness + decay)
                eb.exists_prob *= 1.0 - decay * 0.1
                eb.exists_prob = max(0.0, eb.exists_prob)
            for hb in beliefs.hazard_beliefs:
                hb.freshness = min(1.0, hb.freshness + decay)
            beliefs.information_age_s += dt

        field.decay_beliefs_batch(batch, dt=dt)

        for got, want in zip(batch, expected, strict=False):
            assert got.information_age_s == pytest.approx(want.information_age_s)
            for key, got_exit in got.exit_beliefs.items():
                want_exit = want.exit_beliefs[key]
                assert got_exit.exists_prob == pytest.approx(want_exit.exists_prob)
                assert got_exit.freshness == pytest.approx(want_exit.freshness)
            for got_hazard, want_hazard in zip(
                got.hazard_beliefs, want.hazard_beliefs, strict=False
            ):
                assert got_hazard.freshness == pytest.approx(want_hazard.freshness)

    def test_observe_many_matches_scalar_reference(self):
        field = InformationField(30, 20)
        field.exit_world_positions = {
            (5, 0): (5.5, 0.5, 0.0),
            (25, 19): (25.5, 19.5, 0.0),
        }
        exits = [(5, 0), (25, 19)]
        hazards = [
            Hazard(pos=(3.0, 2.0, 0.0), kind="SMOKE", radius=2.0, severity=0.7),
            Hazard(pos=(24.0, 18.5, 0.0), kind="FIRE", radius=1.5, severity=0.4),
        ]
        batch = [
            BeliefVector(
                hazard_beliefs=[
                    HazardBelief(
                        position=(2.5, 2.0, 0.0),
                        severity_est=0.1,
                        radius_est=0.5,
                        freshness=0.5,
                    )
                ]
            ),
            BeliefVector(),
        ]
        expected = deepcopy(batch)
        observations = [
            (batch[0], (3.0, 2.0, 0.0), 5.0),
            (batch[1], (24.0, 18.0, 0.0), 4.0),
        ]
        expected_observations = [
            (expected[0], (3.0, 2.0, 0.0), 5.0),
            (expected[1], (24.0, 18.0, 0.0), 4.0),
        ]

        for beliefs, agent_pos, vision_radius in expected_observations:
            _scalar_observe_reference(
                field,
                beliefs,
                agent_pos,
                vision_radius,
                exits,
                hazards,
                current_step=7,
            )
        field.observe_many(observations, exits, hazards, current_step=7)

        for got, want in zip(batch, expected, strict=False):
            _assert_beliefs_close(got, want)


class TestEntropy:
    def test_perfect_knowledge_zero_entropy(self):
        b = BeliefVector()
        from chiyoda.information.field import ExitBelief

        b.exit_beliefs[(5, 0)] = ExitBelief(position=(5, 0), exists_prob=1.0)
        b.exit_beliefs[(25, 19)] = ExitBelief(position=(25, 19), exists_prob=1.0)
        h = agent_entropy(b, total_exits=2)
        assert h < 0.1  # near-zero entropy

    def test_no_knowledge_high_entropy(self):
        b = BeliefVector()
        h = agent_entropy(b, total_exits=4)
        assert h > 0.5  # high entropy

    def test_global_entropy(self):
        beliefs = [BeliefVector(), BeliefVector(), BeliefVector()]
        h = global_entropy(beliefs, total_exits=4)
        assert h > 0.0


class TestGossipModel:
    def test_exchange(self):
        np.random.seed(42)
        gossip = GossipModel(GossipConfig(gossip_radius=3.0, base_transfer_prob=1.0))
        sender = BeliefVector()
        from chiyoda.information.field import ExitBelief

        sender.exit_beliefs[(5, 0)] = ExitBelief(
            position=(5, 0), exists_prob=0.95, source_credibility=0.9
        )
        receiver = BeliefVector()
        result = gossip.exchange(sender, receiver, 0.9, 0.8, "CALM", 1.0)
        # may or may not transfer depending on RNG — just test it doesn't crash
        assert isinstance(result, bool)


class TestInformationInterventions:
    def _make_intervention_sim(self, policy="global_broadcast", **overrides):
        layout = Layout.from_text(
            "XXXXXXXXXXXX\n"
            "X..@@@....EX\n"
            "X..........X\n"
            "X..........X\n"
            "XXXXXXXXXXXX\n"
        )
        exits = [Exit(pos=p) for p in layout.exit_positions()]
        agents = [
            Commuter(id=i, pos=_agent_pos(layout, cell), familiarity=0.0)
            for i, cell in enumerate(layout.people_positions())
        ]
        config = SimulationConfig(
            max_steps=3, dt=0.1, random_seed=42, information_mode="none"
        )
        sim = Simulation(layout=layout, agents=agents, exits=exits, config=config)
        spatial = SpatialIndex()
        sim.attach_spatial_index(spatial)
        nav = SmartNavigator(
            layout,
            density_fn=spatial.density_penalty_fn(),
            hazard_fn=sim.hazard_penalty_at_cell,
        )
        sim.attach_navigation(nav)
        sim.attach_behavior_model(BehaviorModel())
        policy_config = {
            "policy": policy,
            "start_step": 0,
            "interval_steps": 1,
            "budget_per_interval": 1,
            "message_radius": 20.0,
            "credibility": 0.95,
        }
        policy_config.update(overrides)
        sim.attach_intervention_policy(create_intervention_policy(policy_config))
        return sim

    def test_global_broadcast_updates_beliefs_and_records_event(self):
        sim = self._make_intervention_sim("global_broadcast")
        sim.run()
        assert len(sim.intervention_events) > 0
        assert sim.intervention_events[0].recipients == len(sim.agents)
        assert all(len(agent.beliefs.known_exits()) > 0 for agent in sim.agents)

    def test_entropy_targeted_policy_selects_recipients(self):
        sim = self._make_intervention_sim("entropy_targeted")
        sim.run()
        assert len(sim.intervention_events) > 0
        assert sim.intervention_events[0].selected_reason == "highest_agent_entropy"
        assert (
            sim.intervention_events[0].entropy_after
            <= sim.intervention_events[0].entropy_before
        )

    def test_llm_guidance_records_generation_telemetry(self, tmp_path):
        sim = self._make_intervention_sim(
            "llm_guidance",
            llm_provider="template",
            llm_cache_path=str(tmp_path),
        )
        sim.run()

        assert len(sim.intervention_events) > 0
        event = sim.intervention_events[0]
        assert event.policy == "llm_guidance"
        assert event.generation_provider == "deterministic"
        assert event.generation_model == "template"
        assert event.validation_status == "accepted"
        assert event.cache_key
        assert event.cache_status == "miss"
        assert event.generated_recommended_exits
        assert event.generated_confidence > 0.0
        assert event.used_fallback is False
        assert list(tmp_path.glob("*.json"))

    def test_llm_guidance_can_swap_target_selector(self, tmp_path):
        sim = self._make_intervention_sim(
            "llm_guidance",
            llm_provider="template",
            llm_cache_path=str(tmp_path),
            llm_target_policy="global_broadcast",
        )
        sim.run()

        assert len(sim.intervention_events) > 0
        event = sim.intervention_events[0]
        assert event.selected_reason == "llm_global_broadcast_global_broadcast"
        assert event.recipients == len(sim.agents)

    def test_llm_guidance_uses_cache_first_without_regeneration(self, tmp_path):
        first = self._make_intervention_sim(
            "llm_guidance",
            llm_provider="template",
            llm_cache_path=str(tmp_path),
        )
        first.run()

        replay = self._make_intervention_sim(
            "llm_guidance",
            llm_provider="openai",
            llm_cache_path=str(tmp_path),
            llm_cache_mode="cache_first",
        )
        replay.run()

        assert len(replay.intervention_events) > 0
        event = replay.intervention_events[0]
        assert event.cache_status == "hit"
        assert event.generation_provider == "deterministic"

    def test_llm_guidance_replay_requires_cache_path(self):
        with pytest.raises(ValueError):
            create_intervention_policy(
                {
                    "policy": "llm_guidance",
                    "llm_provider": "replay",
                }
            )


# -- Agent Tests --


class TestCognitiveAgent:
    def test_physiology(self):
        agent = Commuter(id=0, pos=np.array([5.0, 5.0]))
        agent.update_physiology(hazard_load=0.5, dt=1.0)
        assert agent.physiology.impairment_level > 0.0
        assert agent.physiology.speed_factor < 1.0

    def test_incapacitation(self):
        agent = Commuter(id=0, pos=np.array([5.0, 5.0]))
        for _ in range(20):
            agent.update_physiology(hazard_load=1.0, dt=1.0)
        assert agent.physiology.incapacitated

    def test_responder(self):
        r = FirstResponder(
            id=99,
            pos=np.array([30.0, 10.0]),
            mission_target=(10.0, 10.0),
            ppe_factor=0.1,
        )
        assert r.credibility == 1.0
        assert r.is_responder
        r.update_physiology(hazard_load=1.0, dt=1.0)
        assert r.physiology.impairment_level < 0.1  # PPE protects


# -- Social Force Tests --


class TestSocialForce:
    def test_adjusted_step_basic(self):
        pos = np.array([5.0, 5.0])
        desired = np.array([0.1, 0.0])
        neighbors = np.zeros((0, 2))
        result = adjusted_step(pos, desired, neighbors, [], 0.1)
        assert np.linalg.norm(result) > 0

    def test_counter_flow(self):
        pos = np.array([5.0, 5.0])
        desired = np.array([0.1, 0.0])
        neighbors = np.zeros((0, 2))
        result = adjusted_step(pos, desired, neighbors, [], 0.1, counter_flow=True)
        assert np.linalg.norm(result) > 0


# -- Integration Test --


class TestSimulationIntegration:
    def _make_simple_sim(self, info_mode="asymmetric"):
        layout = Layout.from_text(
            "XXXXXXXXXX\n"
            "X..@@@@..X\n"
            "X........X\n"
            "X..S.....X\n"
            "X........X\n"
            "X........EX\n"
            "XXXXXXXXXX\n"
        )
        exits = [Exit(pos=p) for p in layout.exit_positions()]
        agents = [
            Commuter(id=i, pos=_agent_pos(layout, cell), familiarity=0.5)
            for i, cell in enumerate(layout.people_positions())
        ]
        config = SimulationConfig(
            max_steps=50, dt=0.1, random_seed=42, information_mode=info_mode
        )
        sim = Simulation(layout=layout, agents=agents, exits=exits, config=config)
        spatial = SpatialIndex()
        sim.attach_spatial_index(spatial)
        nav = SmartNavigator(
            layout,
            density_fn=spatial.density_penalty_fn(),
            hazard_fn=sim.hazard_penalty_at_cell,
        )
        sim.attach_navigation(nav)
        sim.attach_behavior_model(BehaviorModel())
        return sim

    def test_runs_without_error(self):
        sim = self._make_simple_sim()
        sim.run()
        assert sim.current_step > 0
        assert len(sim.entropy_history) > 0

    def test_perfect_info_has_low_entropy(self):
        sim = self._make_simple_sim(info_mode="perfect")
        sim.run()
        assert sim.entropy_history[0] < 0.2

    def test_no_info_has_high_entropy(self):
        sim = self._make_simple_sim(info_mode="none")
        sim.run()
        assert sim.entropy_history[0] > 0.3

    def test_telemetry_has_ited_fields(self):
        sim = self._make_simple_sim()
        sim.run()
        step = sim.step_history[-1]
        assert hasattr(step, "global_entropy")
        if step.agents:
            a = step.agents[0]
            assert hasattr(a, "entropy")
            assert hasattr(a, "belief_accuracy")
            assert hasattr(a, "impairment")
            assert hasattr(a, "decision_mode")

    def test_hazard_causes_incapacitation(self):
        layout = Layout.from_text(
            "XXXXXXXXXX\n" "X..@@@@..X\n" "X........X\n" "X........EX\n" "XXXXXXXXXX\n"
        )
        exits = [Exit(pos=p) for p in layout.exit_positions()]
        agents = [
            Commuter(id=i, pos=_agent_pos(layout, cell))
            for i, cell in enumerate(layout.people_positions())
        ]
        hazards = [Hazard(pos=(4.0, 1.5), kind="GAS", radius=5.0, severity=0.9)]
        config = SimulationConfig(max_steps=200, dt=0.1, random_seed=42)
        sim = Simulation(
            layout=layout, agents=agents, exits=exits, hazards=hazards, config=config
        )
        spatial = SpatialIndex()
        sim.attach_spatial_index(spatial)
        nav = SmartNavigator(
            layout,
            density_fn=spatial.density_penalty_fn(),
            hazard_fn=sim.hazard_penalty_at_cell,
        )
        sim.attach_navigation(nav)
        sim.attach_behavior_model(BehaviorModel())
        sim.run()
        incap = sum(1 for a in sim.agents if a.physiology.incapacitated)
        assert incap > 0  # some agents should be incapacitated


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
