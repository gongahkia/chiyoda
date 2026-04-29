from __future__ import annotations

import pytest

from chiyoda.references import load_event_reference


def test_event_reference_loader_validates_provenance_and_exports_frames(tmp_path):
    path = tmp_path / "drill.yaml"
    path.write_text(
        """
reference_id: drill_fixture
reference_type: drill
description: CI-scale event reference fixture.
provenance:
  source: Example drill coding sheet
  source_url: https://example.invalid/drill
  license: CC-BY-4.0
  timestamp: "2026-01-15T09:00:00Z"
  access_date: "2026-04-29"
  station: Example Station
  level: concourse
  coordinate_transform: local meters, origin at southwest fixture corner
  scenario_assumptions:
    - single-level public-area evacuation
  known_missing_data:
    - no physiology observations
observations:
  - event_id: first_queue
    label: queue_detected
    time_s: 42.0
    location: [12.5, 8.0]
    agent_count: 18
    confidence: 0.8
    attributes:
      queue_length_m: 7.5
"""
    )

    reference = load_event_reference(path)
    observations = reference.observations_frame()
    provenance = reference.provenance_frame()

    assert reference.reference_id == "drill_fixture"
    assert reference.reference_type == "drill"
    assert reference.provenance.license == "CC-BY-4.0"
    assert observations.loc[0, "label"] == "queue_detected"
    assert observations.loc[0, "x"] == pytest.approx(12.5)
    assert observations.loc[0, "attribute_queue_length_m"] == pytest.approx(7.5)
    assert provenance.loc[0, "station"] == "Example Station"
    assert "no physiology observations" in provenance.loc[0, "known_missing_data"]


def test_event_reference_loader_rejects_missing_required_provenance(tmp_path):
    path = tmp_path / "incident.yaml"
    path.write_text(
        """
reference_id: bad_reference
reference_type: incident
provenance:
  source: Example incident note
  license: public-domain
  timestamp: "2026-01-15T09:00:00Z"
  station: Example Station
  scenario_assumptions: []
observations:
  - label: evacuation_started
    time_s: 0.0
"""
    )

    with pytest.raises(ValueError, match="known_missing_data"):
        load_event_reference(path)


def test_event_reference_loader_rejects_unknown_reference_type(tmp_path):
    path = tmp_path / "unknown.yaml"
    path.write_text(
        """
reference_id: bad_reference
reference_type: survey
provenance:
  source: Example
  license: CC0
  timestamp: "2026-01-15T09:00:00Z"
  station: Example Station
  scenario_assumptions: []
  known_missing_data: []
observations:
  - label: evacuation_started
    time_s: 0.0
"""
    )

    with pytest.raises(ValueError, match="reference_type"):
        load_event_reference(path)
