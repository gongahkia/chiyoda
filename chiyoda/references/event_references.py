from __future__ import annotations

from dataclasses import dataclass, field
import json
from pathlib import Path
from typing import Any, Mapping

import pandas as pd
import yaml


REFERENCE_TYPES = {"drill", "vr", "incident", "expert_coded"}
REQUIRED_PROVENANCE_FIELDS = {
    "source",
    "license",
    "timestamp",
    "station",
    "scenario_assumptions",
    "known_missing_data",
}


@dataclass(frozen=True)
class EventProvenance:
    source: str
    license: str
    timestamp: str
    station: str
    scenario_assumptions: tuple[str, ...]
    known_missing_data: tuple[str, ...]
    source_url: str = ""
    access_date: str = ""
    level: str = ""
    coordinate_transform: str = ""
    notes: str = ""


@dataclass(frozen=True)
class EventObservation:
    event_id: str
    label: str
    time_s: float | None = None
    timestamp: str = ""
    location: tuple[float, float] | None = None
    agent_count: int | None = None
    confidence: float | None = None
    notes: str = ""
    attributes: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class EventReference:
    reference_id: str
    reference_type: str
    provenance: EventProvenance
    observations: tuple[EventObservation, ...]
    description: str = ""

    def observations_frame(self) -> pd.DataFrame:
        rows: list[dict[str, Any]] = []
        for observation in self.observations:
            location_x = None
            location_y = None
            if observation.location is not None:
                location_x, location_y = observation.location
            rows.append(
                {
                    "reference_id": self.reference_id,
                    "reference_type": self.reference_type,
                    "event_id": observation.event_id,
                    "label": observation.label,
                    "time_s": observation.time_s,
                    "timestamp": observation.timestamp,
                    "x": location_x,
                    "y": location_y,
                    "agent_count": observation.agent_count,
                    "confidence": observation.confidence,
                    "notes": observation.notes,
                    **{f"attribute_{key}": value for key, value in observation.attributes.items()},
                }
            )
        return pd.DataFrame(rows)

    def provenance_frame(self) -> pd.DataFrame:
        return pd.DataFrame(
            [
                {
                    "reference_id": self.reference_id,
                    "reference_type": self.reference_type,
                    "source": self.provenance.source,
                    "source_url": self.provenance.source_url,
                    "license": self.provenance.license,
                    "timestamp": self.provenance.timestamp,
                    "access_date": self.provenance.access_date,
                    "station": self.provenance.station,
                    "level": self.provenance.level,
                    "coordinate_transform": self.provenance.coordinate_transform,
                    "scenario_assumptions": "; ".join(self.provenance.scenario_assumptions),
                    "known_missing_data": "; ".join(self.provenance.known_missing_data),
                    "notes": self.provenance.notes,
                }
            ]
        )


def load_event_reference(path: str | Path) -> EventReference:
    source = Path(path)
    payload = _load_mapping(source)
    return parse_event_reference(payload, source=source)


def parse_event_reference(
    payload: Mapping[str, Any],
    *,
    source: Path | None = None,
) -> EventReference:
    reference_id = str(payload.get("reference_id", "")).strip()
    if not reference_id:
        raise ValueError("Event reference requires reference_id")

    reference_type = str(payload.get("reference_type", "")).strip().lower()
    if reference_type not in REFERENCE_TYPES:
        allowed = ", ".join(sorted(REFERENCE_TYPES))
        raise ValueError(f"reference_type must be one of: {allowed}")

    provenance_payload = payload.get("provenance")
    if not isinstance(provenance_payload, Mapping):
        raise ValueError("Event reference requires a provenance mapping")
    missing = REQUIRED_PROVENANCE_FIELDS.difference(provenance_payload.keys())
    if missing:
        raise ValueError(f"Event reference provenance missing fields: {sorted(missing)}")

    observations_payload = payload.get("observations", [])
    if not isinstance(observations_payload, list) or not observations_payload:
        raise ValueError("Event reference requires at least one observation")

    provenance = EventProvenance(
        source=str(provenance_payload["source"]),
        source_url=str(provenance_payload.get("source_url", "")),
        license=str(provenance_payload["license"]),
        timestamp=str(provenance_payload["timestamp"]),
        access_date=str(provenance_payload.get("access_date", "")),
        station=str(provenance_payload["station"]),
        level=str(provenance_payload.get("level", "")),
        coordinate_transform=str(provenance_payload.get("coordinate_transform", "")),
        scenario_assumptions=_string_tuple(provenance_payload["scenario_assumptions"]),
        known_missing_data=_string_tuple(provenance_payload["known_missing_data"]),
        notes=str(provenance_payload.get("notes", "")),
    )

    observations = tuple(
        _parse_observation(item, index=index, source=source)
        for index, item in enumerate(observations_payload, start=1)
    )
    return EventReference(
        reference_id=reference_id,
        reference_type=reference_type,
        provenance=provenance,
        observations=observations,
        description=str(payload.get("description", "")),
    )


def _parse_observation(
    payload: Any,
    *,
    index: int,
    source: Path | None,
) -> EventObservation:
    if not isinstance(payload, Mapping):
        raise ValueError(f"Observation {index} in {source or '<memory>'} must be a mapping")
    event_id = str(payload.get("event_id", f"event_{index}")).strip()
    label = str(payload.get("label", "")).strip()
    if not label:
        raise ValueError(f"Observation {event_id} requires label")
    time_s = payload.get("time_s")
    location = payload.get("location")
    attributes = payload.get("attributes", {})
    if attributes is None:
        attributes = {}
    if not isinstance(attributes, Mapping):
        raise ValueError(f"Observation {event_id} attributes must be a mapping")
    return EventObservation(
        event_id=event_id,
        label=label,
        time_s=float(time_s) if time_s is not None else None,
        timestamp=str(payload.get("timestamp", "")),
        location=_location_tuple(location) if location is not None else None,
        agent_count=_optional_int(payload.get("agent_count")),
        confidence=_optional_float(payload.get("confidence")),
        notes=str(payload.get("notes", "")),
        attributes=dict(attributes),
    )


def _load_mapping(path: Path) -> Mapping[str, Any]:
    suffix = path.suffix.lower()
    if suffix in {".yaml", ".yml"}:
        payload = yaml.safe_load(path.read_text())
    elif suffix == ".json":
        payload = json.loads(path.read_text())
    else:
        raise ValueError(f"Unsupported event reference format: {path}")
    if not isinstance(payload, Mapping):
        raise ValueError(f"Event reference root must be a mapping: {path}")
    return payload


def _string_tuple(value: Any) -> tuple[str, ...]:
    if isinstance(value, str):
        return (value,)
    if value is None:
        return ()
    return tuple(str(item) for item in value)


def _location_tuple(value: Any) -> tuple[float, float]:
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise ValueError("Observation location must be a two-value coordinate")
    return (float(value[0]), float(value[1]))


def _optional_int(value: Any) -> int | None:
    return None if value is None else int(value)


def _optional_float(value: Any) -> float | None:
    return None if value is None else float(value)
