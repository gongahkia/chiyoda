from __future__ import annotations

import json

from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.standards import strict_layout_from_geojson
from chiyoda.scenarios.validation import validate_scenario_config


def _station_geojson() -> dict:
    return {
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": {"role": "walkable", "level": "0"},
                "geometry": {"type": "Polygon", "coordinates": [[[0, 0], [4, 0], [4, 4], [0, 4], [0, 0]]]},
            },
            {
                "type": "Feature",
                "properties": {"role": "spawn", "level": "0"},
                "geometry": {"type": "Point", "coordinates": [1, 1]},
            },
            {
                "type": "Feature",
                "properties": {"role": "walkable", "level": "1"},
                "geometry": {"type": "Polygon", "coordinates": [[[0, 0], [4, 0], [4, 4], [0, 4], [0, 0]]]},
            },
            {
                "type": "Feature",
                "properties": {"role": "exit", "level": "1"},
                "geometry": {"type": "Point", "coordinates": [3, 3]},
            },
            {
                "type": "Feature",
                "properties": {
                    "pathway_id": "lift_a",
                    "pathway_mode": "5",
                    "from_level": "0",
                    "to_level": "1",
                    "capacity": 1,
                    "dwell_s": 0.1,
                    "travel_s": 0.5,
                },
                "geometry": {"type": "LineString", "coordinates": [[2, 2], [2, 2]]},
            },
        ],
    }


def test_strict_layout_from_gtfs_like_geojson_validates():
    layout = strict_layout_from_geojson(_station_geojson(), cell_size=1.0, padding=1)
    scenario = {
        "name": "converted",
        "layout": layout,
        "population": {"total": 1},
    }
    result = validate_scenario_config(scenario)
    sim = ScenarioManager().build_simulation(scenario)

    assert not result.has_errors
    assert [floor["id"] for floor in layout["floors"]] == ["0", "1"]
    assert layout["connectors"][0]["id"] == "lift_a"
    assert layout["connectors"][0]["type"] == "elevator"
    assert len(sim.layout.connectors) == 1


def test_convert_layout_cli_writes_strict_scenario(tmp_path):
    source = tmp_path / "station.geojson"
    output = tmp_path / "station.yaml"
    source.write_text(json.dumps(_station_geojson()))

    result = CliRunner().invoke(cli, ["convert-layout", str(source), str(output), "--name", "converted"])

    assert result.exit_code == 0
    text = output.read_text()
    assert "floors:" in text
    assert "connectors:" in text
    assert "layout:" in text
