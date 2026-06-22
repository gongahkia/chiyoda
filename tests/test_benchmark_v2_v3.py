from __future__ import annotations

import json
from pathlib import Path

import pytest

from chiyoda.studies.benchmark import (
    BenchmarkSpec,
    _spec_for_suite,
    benchmark_spec_v2,
    benchmark_spec_v3,
)


def test_benchmark_spec_v2_registration():
    spec = benchmark_spec_v2()
    assert spec.suite == "v2"
    assert {scenario.name for scenario in spec.scenarios} == {
        "wildfire_wui",
        "transit_shooter",
    }
    assert spec.scoring_rule == "composite_v1"
    assert _spec_for_suite("v2") == spec


def test_benchmark_spec_v3_registration():
    spec = benchmark_spec_v3()
    assert spec.suite == "v3"
    assert {scenario.name for scenario in spec.scenarios} == {
        "flood_urban",
        "quake_aftershock",
    }
    assert _spec_for_suite("v3") == spec


def test_benchmark_spec_v2_v3_schema_validity():
    schema = BenchmarkSpec.json_schema()
    for spec in (benchmark_spec_v2(), benchmark_spec_v3()):
        payload = spec.to_dict()
        for key in schema["required"]:
            assert key in payload


def test_benchmark_spec_v2_v3_seed_reproducibility():
    spec_a = benchmark_spec_v2()
    spec_b = benchmark_spec_v2()
    assert spec_a.seeds == spec_b.seeds
    assert benchmark_spec_v3().seeds == [42, 137]


def test_benchmark_spec_artifacts_exist():
    for suite in ("v2", "v3"):
        path = Path(f"docs/benchmark/benchmark_spec_{suite}.json")
        assert path.exists(), f"missing spec artifact: {path}"
        payload = json.loads(path.read_text())
        assert payload["suite"] == suite


def test_unknown_suite_raises():
    with pytest.raises(ValueError):
        _spec_for_suite("v999")
