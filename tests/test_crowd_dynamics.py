from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import numpy as np

from chiyoda.analysis.reports import generate_report
from chiyoda.analysis.telemetry import detect_bottleneck_zones
from chiyoda.environment.layout import Layout
from chiyoda.navigation.social_force import adjusted_step
from chiyoda.scenarios.manager import ScenarioManager


REPO_ROOT = Path(__file__).resolve().parents[1]
EXAMPLE_SCENARIO = REPO_ROOT / "scenarios" / "example.yaml"


def build_example_sim(max_steps: int = 20):
    sim = ScenarioManager().load_scenario(str(EXAMPLE_SCENARIO))
    sim.config.max_steps = max_steps
    return sim


class BottleneckDetectionTests(unittest.TestCase):
    def test_detects_corridor_bottleneck(self) -> None:
        layout = Layout.from_text(
            "\n".join(
                [
                    "XXXXXXX",
                    "XXX.XXX",
                    "XXX.XXX",
                    "XXX.XXX",
                    "XXXEXXX",
                    "XXXXXXX",
                ]
            )
        )
        zones = detect_bottleneck_zones(layout)
        self.assertGreaterEqual(len(zones), 1)
        self.assertGreaterEqual(len(zones[0].cells), 3)

    def test_open_room_has_no_bottleneck(self) -> None:
        layout = Layout.from_text(
            "\n".join(
                [
                    "XXXXX",
                    "X...X",
                    "X...X",
                    "X.E.X",
                    "XXXXX",
                ]
            )
        )
        zones = detect_bottleneck_zones(layout)
        self.assertEqual(zones, [])

    def test_multi_exit_example_layout_still_finds_bottleneck(self) -> None:
        sim = build_example_sim(max_steps=1)
        self.assertGreaterEqual(len(sim.bottleneck_zones), 1)


class SimulationTelemetryTests(unittest.TestCase):
    def test_step_history_and_exit_flow_are_deterministic(self) -> None:
        first = build_example_sim(max_steps=20)
        second = build_example_sim(max_steps=20)
        first.run()
        second.run()

        self.assertEqual(len(first.step_history), first.current_step + 1)
        self.assertEqual(len(second.step_history), second.current_step + 1)
        self.assertEqual(first.step_history[-1].exit_flow_cumulative, second.step_history[-1].exit_flow_cumulative)
        self.assertEqual(int(first.step_history[-1].path_usage_grid.sum()), int(second.step_history[-1].path_usage_grid.sum()))
        self.assertGreater(int(first.step_history[-1].path_usage_grid.sum()), 0)
        self.assertEqual(
            sum(first.step_history[-1].exit_flow_cumulative.values()),
            len(first.completed_agents),
        )
        peak_queue = max(
            metrics.queue_length
            for step in first.step_history
            for metrics in step.bottlenecks.values()
        )
        self.assertGreaterEqual(peak_queue, 1)

    def test_agents_reduce_speed_and_refresh_navigation_under_density(self) -> None:
        sim = build_example_sim(max_steps=15)
        sim.run()

        crowded_agents = [agent for agent in sim.agents if agent.crowd_speed_factor < 1.0]
        rerouted_agents = [agent for agent in sim.agents if agent.last_navigation_step >= 6]

        self.assertTrue(crowded_agents)
        self.assertTrue(rerouted_agents)


class MovementAndExportTests(unittest.TestCase):
    def test_social_force_repulsion_changes_step(self) -> None:
        desired = np.array([0.1, 0.0], dtype=float)
        result = adjusted_step(
            current_pos=np.array([0.0, 0.0], dtype=float),
            desired_step=desired,
            neighbors=np.array([[0.2, 0.0]], dtype=float),
            walls=[],
            dt=0.1,
        )
        self.assertFalse(np.allclose(result, desired))

    def test_dashboard_export_contains_expected_panels(self) -> None:
        sim = build_example_sim(max_steps=18)
        sim.run()
        with tempfile.TemporaryDirectory() as tmpdir:
            output = Path(tmpdir) / "dashboard.html"
            generate_report(sim, str(output))
            html = output.read_text()

        for needle in (
            "Chiyoda Congestion Study Dashboard",
            "Occupancy Heatmap",
            "Speed Heatmap",
            "Bottleneck Queue",
            "Travel Time Distribution",
        ):
            self.assertIn(needle, html)


if __name__ == "__main__":
    unittest.main()
