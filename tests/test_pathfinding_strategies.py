from __future__ import annotations

import numpy as np
import pytest

from chiyoda.environment.layout import Connector, Floor, Layout
from chiyoda.navigation.pathfinding import SmartNavigator
from chiyoda.scenarios.manager import ScenarioManager


def _cost(nav: SmartNavigator, path) -> float:
    return nav._path_cost(path, nav._weight_fn(None))


def test_heap_astar_matches_networkx_astar_cost():
    layout = Layout.from_text(
        "XXXXXXX\n" "X@...EX\n" "X.XXX.X\n" "X.....X\n" "XXXXXXX\n"
    )
    goals = layout.exit_positions()
    baseline = SmartNavigator(layout, strategy="networkx_astar")
    heap = SmartNavigator(layout, strategy="heap_astar")

    baseline_path = baseline.find_optimal_path((1, 1), goals)
    heap_path = heap.find_optimal_path((1, 1), goals)

    assert heap_path is not None
    assert baseline_path is not None
    assert _cost(heap, heap_path) == pytest.approx(_cost(baseline, baseline_path))


def test_reverse_dijkstra_matches_astar_for_shared_exits_and_caches():
    layout = Layout.from_text(
        "XXXXXXXX\n" "X@....EX\n" "X......X\n" "XE.....X\n" "XXXXXXXX\n"
    )
    goals = layout.exit_positions()
    astar = SmartNavigator(layout, strategy="heap_astar")
    reverse = SmartNavigator(layout, strategy="reverse_dijkstra")

    astar_path = astar.find_optimal_path((1, 1), goals)
    reverse_path = reverse.find_optimal_path((1, 1), goals)
    reverse_again = reverse.find_optimal_path((1, 1), goals)
    stats = reverse.route_stats()

    assert reverse_path == reverse_again
    assert reverse_path is not None
    assert astar_path is not None
    assert _cost(reverse, reverse_path) == pytest.approx(_cost(astar, astar_path))
    assert stats["route_cache_hits"] == 1
    assert stats["route_cache_misses"] == 1


def test_reverse_dijkstra_respects_hazard_penalty():
    layout = Layout.from_text("XXXXXXX\n" "X@...EX\n" "X.....X\n" "XXXXXXX\n")

    def hazard_fn(cell):
        return 10.0 if cell == ("0", 3, 1) else 0.0

    nav = SmartNavigator(layout, hazard_fn=hazard_fn, strategy="reverse_dijkstra")
    path = nav.find_optimal_path((1, 1), layout.exit_positions())

    assert path is not None
    assert ("0", 3, 1) not in path
    assert ("0", 3, 2) in path


def test_directed_connector_edges_are_preserved():
    floor0 = Floor(id="0", z=0.0, grid=np.array([[".", "E"]], dtype="<U1"))
    floor1 = Floor(id="1", z=3.0, grid=np.array([[".", "."]], dtype="<U1"))
    layout = Layout(
        floors={"0": floor0, "1": floor1},
        connectors=[
            Connector(
                id="up_only",
                type="stairs",
                from_cell=("0", 0, 0),
                to_cell=("1", 0, 0),
                bidirectional=False,
            )
        ],
    )
    nav = SmartNavigator(layout, strategy="reverse_dijkstra")

    assert nav.find_optimal_path(("0", 0, 0), [("1", 0, 0)]) == [
        ("0", 0, 0),
        ("1", 0, 0),
    ]
    assert nav.find_optimal_path(("1", 0, 0), [("0", 0, 0)]) is None


def test_scenario_config_selects_pathfinding_strategy():
    scenario = {
        "name": "pathfinding_config",
        "layout": {
            "floors": [
                {
                    "id": "0",
                    "z": 0.0,
                    "text": "XXXXX\nX@.EX\nXXXXX",
                }
            ]
        },
        "population": {"total": 1},
        "simulation": {
            "max_steps": 1,
            "pathfinding_strategy": "heap_astar",
        },
    }
    sim = ScenarioManager().build_simulation(scenario)

    assert sim.config.pathfinding_strategy == "heap_astar"
    assert sim.navigator.strategy == "heap_astar"


def test_unknown_pathfinding_strategy_fails_fast():
    layout = Layout.from_text("XXX\nXEX\nX@X\n")

    with pytest.raises(ValueError, match="pathfinding_strategy"):
        SmartNavigator(layout, strategy="ida_star")
