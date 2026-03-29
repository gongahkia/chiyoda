from __future__ import annotations

import json
import tempfile
import unittest
import warnings
from pathlib import Path

import numpy as np
import yaml
from click.testing import CliRunner

from chiyoda.acceleration import create_acceleration_backend
from chiyoda.analysis.reports import export_figures, generate_report
from chiyoda.analysis.telemetry import detect_bottleneck_zones
from chiyoda.cli import cli
from chiyoda.environment.layout import Layout
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.navigation.social_force import adjusted_step
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.studies import compare_studies, run_study
from chiyoda.studies.schema import InterventionConfig, StudyConfig


REPO_ROOT = Path(__file__).resolve().parents[1]
EXAMPLE_SCENARIO = REPO_ROOT / "scenarios" / "example.yaml"
EXAMPLE_STUDY = REPO_ROOT / "scenarios" / "example_study.yaml"


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
        self.assertEqual(
            first.step_history[-1].exit_flow_cumulative,
            second.step_history[-1].exit_flow_cumulative,
        )
        self.assertEqual(
            int(first.step_history[-1].path_usage_grid.sum()),
            int(second.step_history[-1].path_usage_grid.sum()),
        )
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

    def test_cohort_release_creates_pending_agents(self) -> None:
        manager = ScenarioManager()
        scenario = manager.load_config(str(EXAMPLE_SCENARIO))
        scenario["population"] = {
            "cohorts": [
                {
                    "name": "early",
                    "count": 10,
                    "personality": "NORMAL",
                    "release_step": 0,
                },
                {
                    "name": "late",
                    "count": 10,
                    "personality": "WHEELCHAIR",
                    "release_step": 8,
                },
            ]
        }
        sim = manager.build_simulation(scenario)
        sim.config.max_steps = 10
        sim.run()

        self.assertGreater(sim.step_history[0].pending_release, 0)
        self.assertTrue(any(agent.release_step == 8 for agent in sim.agents))

    def test_hazard_penalty_changes_route_choice(self) -> None:
        layout = Layout.from_text(
            "\n".join(
                [
                    "XXXXXXXXXXX",
                    "X@.......EX",
                    "X.XXXXXXX.X",
                    "X.........X",
                    "XXXXXXXXXXX",
                ]
            )
        )
        start = (1, 1)
        goal = [(9, 1)]
        top_hazard = (5, 1)
        plain = SmartNavigator(layout)
        rerouting = SmartNavigator(
            layout,
            hazard_fn=lambda cell: 10.0 if cell == top_hazard else 0.0,
        )

        plain_path = plain.find_optimal_path(start, goal)
        rerouted_path = rerouting.find_optimal_path(start, goal)

        self.assertIn(top_hazard, plain_path)
        self.assertNotIn(top_hazard, rerouted_path)


class LayoutAndAccelerationTests(unittest.TestCase):
    def test_layout_obstacles_overlay_geometry_on_text_grid(self) -> None:
        manager = ScenarioManager()
        scenario = {
            "layout": {
                "grid": [
                    "XXXXXXX",
                    "X.....X",
                    "X..E..X",
                    "X.....X",
                    "XXXXXXX",
                ],
                "obstacles": [
                    {"shape": "rectangle", "x": 2, "y": 1, "width": 2, "height": 2},
                    {"shape": "circle", "center": [5.5, 3.5], "radius": 0.6},
                ],
            }
        }

        layout = manager._build_layout(scenario)

        self.assertFalse(layout.is_walkable((2, 1)))
        self.assertFalse(layout.is_walkable((3, 2)))
        self.assertFalse(layout.is_walkable((5, 3)))
        self.assertFalse(layout.is_exit((3, 2)))

    def test_geojson_layout_ingests_walkable_obstacle_exit_and_spawn_features(self) -> None:
        manager = ScenarioManager()
        payload = {
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "properties": {"role": "walkable"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[[0, 0], [6, 0], [6, 4], [0, 4], [0, 0]]],
                    },
                },
                {
                    "type": "Feature",
                    "properties": {"role": "obstacle"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[[2, 1], [4, 1], [4, 3], [2, 3], [2, 1]]],
                    },
                },
                {
                    "type": "Feature",
                    "properties": {"role": "exit"},
                    "geometry": {"type": "Point", "coordinates": [5, 2]},
                },
                {
                    "type": "Feature",
                    "properties": {"role": "spawn"},
                    "geometry": {"type": "Point", "coordinates": [1, 2]},
                },
            ],
        }

        with tempfile.TemporaryDirectory() as tmpdir:
            geojson_file = Path(tmpdir) / "layout.geojson"
            geojson_file.write_text(json.dumps(payload))
            layout = manager._build_layout(
                {
                    "_source_file": str(geojson_file),
                    "layout": {"geojson": {"file": str(geojson_file), "cell_size": 1.0, "padding": 1}},
                }
            )

        self.assertEqual(layout.origin, (-1.0, -1.0))
        self.assertIn((2, 3), layout.people_positions())
        self.assertIn((6, 3), layout.exit_positions())
        self.assertFalse(layout.is_walkable((4, 3)))

    def test_dxf_layout_ingests_walkable_obstacle_exit_and_spawn_layers(self) -> None:
        manager = ScenarioManager()
        dxf_text = "\n".join(
            [
                "0", "SECTION",
                "2", "ENTITIES",
                "0", "LWPOLYLINE",
                "8", "WALKABLE",
                "90", "4",
                "70", "1",
                "10", "0",
                "20", "0",
                "10", "6",
                "20", "0",
                "10", "6",
                "20", "4",
                "10", "0",
                "20", "4",
                "0", "LWPOLYLINE",
                "8", "OBSTACLE",
                "90", "4",
                "70", "1",
                "10", "2",
                "20", "1",
                "10", "4",
                "20", "1",
                "10", "4",
                "20", "3",
                "10", "2",
                "20", "3",
                "0", "POINT",
                "8", "EXIT",
                "10", "5",
                "20", "2",
                "0", "POINT",
                "8", "SPAWN",
                "10", "1",
                "20", "2",
                "0", "ENDSEC",
                "0", "EOF",
            ]
        )

        with tempfile.TemporaryDirectory() as tmpdir:
            dxf_file = Path(tmpdir) / "layout.dxf"
            dxf_file.write_text(dxf_text + "\n")
            layout = manager._build_layout(
                {
                    "_source_file": str(dxf_file),
                    "layout": {
                        "cad": {
                            "file": str(dxf_file),
                            "cell_size": 1.0,
                            "padding": 1,
                            "role_layers": {
                                "walkable": ["WALKABLE"],
                                "obstacle": ["OBSTACLE"],
                                "exit": ["EXIT"],
                                "spawn": ["SPAWN"],
                            },
                        }
                    },
                }
            )

        self.assertEqual(layout.origin, (-1.0, -1.0))
        self.assertIn((2, 3), layout.people_positions())
        self.assertIn((6, 3), layout.exit_positions())
        self.assertFalse(layout.is_walkable((4, 3)))

    def test_requested_julia_backend_falls_back_to_python_when_unavailable(self) -> None:
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            backend = create_acceleration_backend("julia")

        self.assertEqual(backend.requested_backend, "julia")
        self.assertEqual(backend.name, "python")
        self.assertTrue(backend.fallback_reason)
        self.assertTrue(caught)

    def test_visualization_source_package_is_removed(self) -> None:
        self.assertFalse((REPO_ROOT / "chiyoda" / "visualization" / "__init__.py").exists())
        self.assertFalse((REPO_ROOT / "chiyoda" / "visualization" / "plotly_viz.py").exists())


class StudyWorkflowTests(unittest.TestCase):
    def test_offline_report_exports_expected_figures(self) -> None:
        sim = build_example_sim(max_steps=18)
        sim.run()
        with tempfile.TemporaryDirectory() as tmpdir:
            output_dir = Path(tmpdir) / "legacy_report"
            generate_report(sim, output_dir)

            for figure_name in (
                "01_layout_and_keyframes.png",
                "02_occupancy_and_slowdown.png",
                "03_bottleneck_dynamics.png",
                "04_exit_and_flow.png",
                "05_distributions.png",
                "06_scenario_comparison.png",
            ):
                self.assertTrue((output_dir / figure_name).exists(), figure_name)

    def test_run_study_bundle_exports_tables_and_figures(self) -> None:
        bundle = run_study(str(EXAMPLE_SCENARIO))
        self.assertFalse(bundle.summary.empty)
        self.assertFalse(bundle.steps.empty)
        self.assertFalse(bundle.agents.empty)

        with tempfile.TemporaryDirectory() as tmpdir:
            output_dir = Path(tmpdir) / "study"
            bundle.export(output_dir)
            export_figures(bundle, output_dir=output_dir / "figures")
            reloaded = type(bundle).load(output_dir)

            self.assertFalse(reloaded.summary.empty)
            self.assertTrue((output_dir / "tables" / "summary.parquet").exists())
            self.assertTrue((output_dir / "figures" / "01_layout_and_keyframes.png").exists())

    def test_study_definition_runs_multiple_variants(self) -> None:
        bundle = run_study(str(EXAMPLE_STUDY))
        variant_rows = bundle.summary[bundle.summary["record_type"] == "variant_aggregate"]
        self.assertGreaterEqual(variant_rows["variant_name"].nunique(), 3)
        self.assertIn("mean_hazard_exposure", bundle.summary.columns)

    def test_repeated_seed_study_is_reproducible_for_fixed_seeds(self) -> None:
        config = StudyConfig(
            name="repeatable",
            scenario_file=str(EXAMPLE_SCENARIO),
            seeds=[11, 11],
            variants=[],
        )
        bundle = run_study(config)
        run_rows = bundle.summary[bundle.summary["record_type"] == "run"].sort_values("seed")
        self.assertEqual(len(run_rows), 2)
        self.assertAlmostEqual(
            float(run_rows.iloc[0]["mean_travel_time_s"]),
            float(run_rows.iloc[1]["mean_travel_time_s"]),
            places=6,
        )
        self.assertAlmostEqual(
            float(run_rows.iloc[0]["peak_bottleneck_queue"]),
            float(run_rows.iloc[1]["peak_bottleneck_queue"]),
            places=6,
        )

    def test_compare_studies_produces_metric_deltas(self) -> None:
        baseline = run_study(str(EXAMPLE_SCENARIO))
        variant = run_study(str(EXAMPLE_STUDY))
        result = compare_studies(baseline, variant)
        self.assertFalse(result.metrics.empty)
        self.assertIn("delta", result.metrics.columns)

    def test_cli_sweep_honors_study_export_config(self) -> None:
        runner = CliRunner()
        study_config = {
            "study": {
                "name": "cli-export-config",
                "scenario_file": str(EXAMPLE_SCENARIO),
                "export": {
                    "profile": "paper",
                    "formats": ["png"],
                    "table_formats": ["csv"],
                    "include_figures": True,
                },
                "variants": [{"name": "baseline"}],
            }
        }

        with tempfile.TemporaryDirectory() as tmpdir:
            study_file = Path(tmpdir) / "study.yaml"
            output_dir = Path(tmpdir) / "bundle"
            study_file.write_text(yaml.safe_dump(study_config, sort_keys=False))

            result = runner.invoke(cli, ["sweep", str(study_file), "--out", str(output_dir)])

            self.assertEqual(result.exit_code, 0, result.output)
            self.assertTrue((output_dir / "tables" / "summary.csv").exists())
            self.assertFalse((output_dir / "tables" / "summary.parquet").exists())
            self.assertTrue((output_dir / "figures" / "01_layout_and_keyframes.png").exists())
            self.assertFalse((output_dir / "figures" / "01_layout_and_keyframes.svg").exists())

    def test_schema_rejects_invalid_intervention(self) -> None:
        with self.assertRaises(ValueError):
            InterventionConfig(type="exit_closure")

        with self.assertRaises(ValueError):
            StudyConfig(
                name="invalid",
                scenario_file=str(EXAMPLE_SCENARIO),
                repetitions=0,
            )

        with self.assertRaises(ValueError):
            StudyConfig(
                name="invalid_export",
                scenario_file=str(EXAMPLE_SCENARIO),
                export={"table_formats": []},
            )


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


if __name__ == "__main__":
    unittest.main()
