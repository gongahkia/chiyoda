from __future__ import annotations

import numpy as np

from chiyoda.analysis.telemetry import detect_bottleneck_zones
from chiyoda.environment.layout import Layout
from chiyoda.scenarios.manager import ScenarioManager


def test_geojson_ingestion_infers_osm_and_gtfs_station_roles():
    layout = Layout.from_geojson(
        {
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "properties": {"indoor": "corridor", "level": "0"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [[0, 0], [8, 0], [8, 4], [0, 4], [0, 0]]
                        ],
                    },
                },
                {
                    "type": "Feature",
                    "properties": {"indoor": "wall", "level": "0"},
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [
                            [[3, 0], [4, 0], [4, 3], [3, 3], [3, 0]]
                        ],
                    },
                },
                {
                    "type": "Feature",
                    "properties": {"entrance": "main", "level": "0"},
                    "geometry": {"type": "Point", "coordinates": [0, 2]},
                },
                {
                    "type": "Feature",
                    "properties": {"location_type": "2", "level": "0"},
                    "geometry": {"type": "Point", "coordinates": [8, 2]},
                },
                {
                    "type": "Feature",
                    "properties": {"pathway_mode": "1", "is_bidirectional": "1"},
                    "geometry": {
                        "type": "LineString",
                        "coordinates": [[4, 2], [7, 2]],
                    },
                },
                {
                    "type": "Feature",
                    "properties": {"role": "spawn"},
                    "geometry": {"type": "Point", "coordinates": [1, 2]},
                },
            ],
        },
        cell_size=1.0,
        padding=1,
        add_border_walls=True,
    )

    assert len(layout.exit_positions()) == 2
    assert len(layout.people_positions()) == 1
    assert np.count_nonzero(layout.grid == ".") > 0
    assert np.count_nonzero(layout.grid == "X") > 0


def test_edge_bottleneck_station_scenario_builds_from_geojson_fixture():
    sim = ScenarioManager().load_scenario("scenarios/edge_bottleneck_station.yaml")

    assert len(sim.exits) == 2
    assert len(sim.agents) == 24
    assert len(sim.hazards) == 1
    assert sim.intervention_policy is not None
    assert sim.layout.cell_size == 1.0
    assert detect_bottleneck_zones(sim.layout)
