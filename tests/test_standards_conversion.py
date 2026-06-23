from __future__ import annotations

import json
from pathlib import Path

import yaml
from click.testing import CliRunner

from chiyoda.cli import cli
from chiyoda.environment.gtfs_pathways import strict_scenario_from_gtfs_pathways
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.standards import (
    overpass_json_to_geojson,
    strict_layout_from_geojson,
)
from chiyoda.scenarios.validation import validate_scenario_config


def _station_geojson() -> dict:
    return {
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": {"role": "walkable", "level": "0"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[0, 0], [4, 0], [4, 4], [0, 4], [0, 0]]],
                },
            },
            {
                "type": "Feature",
                "properties": {"role": "spawn", "level": "0"},
                "geometry": {"type": "Point", "coordinates": [1, 1]},
            },
            {
                "type": "Feature",
                "properties": {"role": "walkable", "level": "1"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[0, 0], [4, 0], [4, 4], [0, 4], [0, 0]]],
                },
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

    result = CliRunner().invoke(
        cli, ["convert-layout", str(source), str(output), "--name", "converted"]
    )

    assert result.exit_code == 0
    text = output.read_text()
    assert "floors:" in text
    assert "connectors:" in text
    assert "layout:" in text


def test_convert_layout_cli_fetches_osm_bbox(monkeypatch, tmp_path):
    import chiyoda.scenarios.standards as standards

    output = tmp_path / "osm.yaml"
    calls = []

    def fake_fetch(query, overpass_url, timeout):
        calls.append((query, overpass_url, timeout))
        return {
            "elements": [
                {
                    "type": "way",
                    "id": 10,
                    "tags": {"indoor": "corridor", "level": "0"},
                    "geometry": [
                        {"lat": 35.0, "lon": 139.0},
                        {"lat": 35.0, "lon": 139.00002},
                        {"lat": 35.00002, "lon": 139.00002},
                        {"lat": 35.00002, "lon": 139.0},
                        {"lat": 35.0, "lon": 139.0},
                    ],
                },
                {
                    "type": "node",
                    "id": 20,
                    "tags": {"entrance": "yes", "level": "0"},
                    "lat": 35.00001,
                    "lon": 139.00002,
                },
            ]
        }

    monkeypatch.setattr(standards, "_fetch_overpass_json", fake_fetch)

    result = CliRunner().invoke(
        cli,
        [
            "convert-layout",
            str(output),
            "--osm-bbox",
            "35.0,139.0,35.00003,139.00003",
            "--name",
            "osm_fixture",
            "--overpass-timeout",
            "7",
        ],
    )

    assert result.exit_code == 0
    assert calls
    query, overpass_url, timeout = calls[0]
    assert "35.0,139.0,35.00003,139.00003" in query
    assert 'node["indoor"]' in query
    assert 'way["repeat_on"]' in query
    assert overpass_url == standards.OVERPASS_URL
    assert timeout == 7
    payload = yaml.safe_load(output.read_text())["scenario"]
    provenance = payload["metadata"]["station_provenance"]
    assert provenance["station"] == "osm_fixture"
    assert "way/10" in provenance["osm_objects"]
    assert "node/20" in provenance["osm_objects"]
    assert "ODbL" in provenance["license"]
    assert payload["layout"]["floors"][0]["id"] == "0"


def test_osm_indoor_level_grammar_converts_to_floors_and_connectors():
    geojson = {
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": {"indoor": "area", "level": "-1-1"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[0, 0], [6, 0], [6, 6], [0, 6], [0, 0]]],
                },
            },
            {
                "type": "Feature",
                "properties": {"indoor": "wall", "level": "-1", "repeat_on": "0;1"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[2, 0], [3, 0], [3, 6], [2, 6], [2, 0]]],
                },
            },
            {
                "type": "Feature",
                "properties": {"indoor": "door", "level": "0"},
                "geometry": {"type": "Point", "coordinates": [2.5, 3]},
            },
            {
                "type": "Feature",
                "properties": {"indoor": "no", "level": "0"},
                "geometry": {"type": "Point", "coordinates": [1, 1]},
            },
            {
                "type": "Feature",
                "properties": {"entrance": "yes", "level": "1"},
                "geometry": {"type": "Point", "coordinates": [5, 5]},
            },
            {
                "type": "Feature",
                "properties": {"stairs": "yes", "indoor": "room", "level": "-1-1"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[4, 1], [5, 1], [5, 2], [4, 2], [4, 1]]],
                },
            },
        ],
    }

    layout = strict_layout_from_geojson(geojson, cell_size=1.0, padding=1)
    by_floor = {floor["id"]: floor["text"] for floor in layout["floors"]}
    scenario = {"name": "osm_indoor", "layout": layout, "population": {"total": 0}}
    result = validate_scenario_config(scenario)

    assert not result.has_errors
    assert list(by_floor) == ["-1", "0", "1"]
    assert all("X" in text for text in by_floor.values())
    assert "E" in by_floor["1"]
    assert layout["connectors"][0]["type"] == "stairs"
    assert layout["connectors"][0]["from"]["floor"] == "-1"
    assert layout["connectors"][0]["to"]["floor"] == "1"


def test_real_berlin_hauptbahnhof_osm_extract_converts_without_fixups():
    sample = Path("data/osm_samples/berlin_hauptbahnhof_indoor_excerpt.json")
    payload = json.loads(sample.read_text())
    geojson = overpass_json_to_geojson(payload, bbox=tuple(payload["osm_bbox"]))

    layout = strict_layout_from_geojson(geojson, cell_size=8.0, padding=1)
    scenario = {"name": "berlin_hbf_osm", "layout": layout, "population": {"total": 0}}
    result = validate_scenario_config(scenario)

    assert not result.has_errors
    assert payload["station"] == "Berlin Hauptbahnhof"
    assert {floor["id"] for floor in layout["floors"]} >= {"-2", "-1", "0", "1"}
    assert {connector["type"] for connector in layout["connectors"]} >= {
        "stairs",
        "elevator",
    }
    assert any("E" in floor["text"] for floor in layout["floors"])
    assert any("X" in floor["text"] for floor in layout["floors"])


def test_gtfs_pathways_sample_converts_and_preserves_ids():
    payload = strict_scenario_from_gtfs_pathways(
        "data/gtfs_pathways_samples/waterfront_pathways",
        name="waterfront_pathways",
        cell_size=2.0,
    )
    scenario = payload["scenario"]
    result = validate_scenario_config(scenario)
    metadata = scenario["metadata"]["gtfs_pathways"]

    assert not result.has_errors
    assert {floor["id"] for floor in scenario["layout"]["floors"]} == {
        "surface",
        "concourse",
    }
    assert {item["level_id"] for item in metadata["levels"]} == {
        "surface",
        "concourse",
    }
    assert {item["pathway_id"] for item in metadata["pathways"]} >= {
        "stairsA",
        "escalatorA",
        "elevatorA",
        "underground_walkway1",
    }
    assert {connector["id"] for connector in scenario["layout"]["connectors"]} == {
        "stairsA",
        "escalatorA",
        "elevatorA",
    }


def test_convert_gtfs_cli_writes_strict_scenario(tmp_path):
    output = tmp_path / "waterfront.yaml"

    result = CliRunner().invoke(
        cli,
        [
            "convert-gtfs",
            "data/gtfs_pathways_samples/waterfront_pathways",
            str(output),
            "--name",
            "waterfront_pathways",
            "--cell-size",
            "2",
        ],
    )

    assert result.exit_code == 0, result.output
    payload = yaml.safe_load(output.read_text())["scenario"]
    assert payload["metadata"]["gtfs_pathways"]["levels"][0]["level_id"] == "concourse"
    assert payload["metadata"]["gtfs_pathways"]["pathways"][0]["pathway_id"]
    assert payload["layout"]["connectors"]
