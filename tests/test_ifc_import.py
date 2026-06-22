from __future__ import annotations

import importlib
import importlib.util
from types import SimpleNamespace

import pytest

from chiyoda.environment.layout import Layout
from chiyoda.scenarios import ifc_import


class _FakeElement:
    def __init__(
        self,
        ifc_type: str,
        name: str,
        verts: list[float] | None = None,
        *,
        elevation: float | None = None,
    ) -> None:
        self._ifc_type = ifc_type
        self.Name = name
        self.LongName = name
        self.Elevation = elevation
        self.verts = verts or []

    def is_a(self, type_name: str | None = None):
        if type_name is None:
            return self._ifc_type
        return self._ifc_type == type_name


class _FakeModel:
    def __init__(self, storeys: list[_FakeElement], elements: list[_FakeElement]):
        self.storeys = storeys
        self.elements = elements

    def by_type(self, type_name: str):
        if type_name == "IfcBuildingStorey":
            return self.storeys
        return [element for element in self.elements if element.is_a(type_name)]


class _FakeIfc:
    def __init__(self, model: _FakeModel) -> None:
        self.model = model

    def open(self, _path: str):
        return self.model


class _FakeSettings:
    def set(self, _key, _value) -> None:
        return None


class _FakeGeom:
    settings = _FakeSettings

    @staticmethod
    def create_shape(_settings, element: _FakeElement):
        return SimpleNamespace(geometry=SimpleNamespace(verts=element.verts))


def _box(
    x0: float, y0: float, z0: float, x1: float, y1: float, z1: float
) -> list[float]:
    points = [
        (x0, y0, z0),
        (x1, y0, z0),
        (x1, y1, z0),
        (x0, y1, z0),
        (x0, y0, z1),
        (x1, y0, z1),
        (x1, y1, z1),
        (x0, y1, z1),
    ]
    return [coord for point in points for coord in point]


def _run_tiny_ifc_fixture(monkeypatch, tmp_path):
    model = _FakeModel(
        storeys=[
            _FakeElement("IfcBuildingStorey", "0", elevation=0.0),
            _FakeElement("IfcBuildingStorey", "1", elevation=3.0),
        ],
        elements=[
            _FakeElement("IfcSlab", "ground", _box(0, 0, 0, 5, 4, 0.2)),
            _FakeElement("IfcWall", "west_wall", _box(0, 0, 0, 0.4, 4, 2.5)),
            _FakeElement("IfcDoor", "egress", _box(4.4, 1.5, 0, 5, 2.5, 2.2)),
            _FakeElement("IfcSlab", "upper", _box(0, 0, 3, 5, 4, 3.2)),
            _FakeElement("IfcStair", "stair_a", _box(2, 2, 0, 2.8, 2.8, 3.1)),
        ],
    )
    monkeypatch.setattr(
        ifc_import,
        "_import_ifcopenshell",
        lambda: (_FakeIfc(model), _FakeGeom),
    )
    source = tmp_path / "tiny.ifc"
    source.write_text("ISO-10303-21;\nEND-ISO-10303-21;\n")

    return ifc_import.strict_layout_from_ifc(source, cell_size=1.0, padding=1)


def test_strict_layout_from_ifc_lowers_synthetic_fixture(monkeypatch, tmp_path):
    layout = _run_tiny_ifc_fixture(monkeypatch, tmp_path)

    assert [floor["id"] for floor in layout["floors"]] == ["0", "1"]
    assert any("E" in floor["text"] for floor in layout["floors"])
    assert layout["connectors"][0]["type"] == "stairs"
    assert layout["connectors"][0]["from"]["floor"] == "0"
    assert layout["connectors"][0]["to"]["floor"] == "1"
    parsed = Layout.from_floors(
        layout["floors"],
        connectors=layout["connectors"],
        cell_size=layout["cell_size"],
        origin=tuple(layout["origin"]),
    )
    assert len(parsed.connectors) == 1


def test_strict_layout_from_ifc_missing_optional_dependency(monkeypatch):
    real_import = importlib.import_module

    def fake_import(name: str):
        if name.startswith("ifcopenshell"):
            raise ModuleNotFoundError(name)
        return real_import(name)

    monkeypatch.setattr(ifc_import.importlib, "import_module", fake_import)

    with pytest.raises(ImportError, match="ifcopenshell"):
        ifc_import.strict_layout_from_ifc("missing.ifc")


@pytest.mark.skipif(
    importlib.util.find_spec("ifcopenshell") is None,
    reason="ifcopenshell is not installed",
)
def test_strict_layout_from_ifc_fixture_path_with_ifcopenshell(monkeypatch, tmp_path):
    layout = _run_tiny_ifc_fixture(monkeypatch, tmp_path)

    assert len(layout["floors"]) == 2
