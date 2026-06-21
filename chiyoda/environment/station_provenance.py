from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Mapping, Optional


REQUIRED_FIELDS = {
    "station",
    "level",
    "source_url",
    "license",
    "access_date",
    "coordinate_transform",
    "manual_edits",
    "known_missing_indoor_topology",
    "validation_use",
    "attribution",
}


def load_station_provenance(
    metadata: Mapping[str, Any],
    *,
    source_file: Optional[str] = None,
) -> dict[str, Any] | None:
    if not metadata:
        return None

    inline = metadata.get("station_provenance")
    provenance_file = metadata.get("provenance_file")
    report_facing = bool(metadata.get("report_facing_station_case", False))

    if inline is None and provenance_file is None:
        if report_facing:
            raise ValueError("report-facing station cases require metadata.provenance_file or metadata.station_provenance")
        return None

    if inline is not None:
        if not isinstance(inline, Mapping):
            raise ValueError("metadata.station_provenance must be a mapping")
        provenance = dict(inline)
    else:
        path = _resolve_path(str(provenance_file), source_file)
        provenance = json.loads(path.read_text())

    validate_station_provenance(provenance, report_facing=report_facing)
    return provenance


def validate_station_provenance(
    provenance: Mapping[str, Any],
    *,
    report_facing: bool = True,
) -> None:
    missing = sorted(field for field in REQUIRED_FIELDS if _is_missing(provenance.get(field)))
    if missing:
        raise ValueError(f"Station provenance missing required fields: {missing}")
    if not (provenance.get("osm_objects") or provenance.get("gtfs_feeds") or provenance.get("source_files")):
        raise ValueError("Station provenance requires one of osm_objects, gtfs_feeds, or source_files")
    for field in ("manual_edits", "known_missing_indoor_topology"):
        value = provenance.get(field)
        if not isinstance(value, list) or not value:
            raise ValueError(f"Station provenance field {field} must be a non-empty list")
    limitation = str(provenance.get("validation_use", "")).lower()
    if report_facing and not any(token in limitation for token in ("not", "only", "proxy", "diagnostic")):
        raise ValueError("Report-facing station provenance must state validation limitations")


def _resolve_path(raw_path: str, source_file: Optional[str]) -> Path:
    path = Path(raw_path)
    if path.is_absolute():
        return path
    if path.exists():
        return path.resolve()
    if source_file is not None:
        candidate = Path(source_file).resolve().parent / path
        if candidate.exists():
            return candidate
    return path


def _is_missing(value: Any) -> bool:
    if value is None:
        return True
    if isinstance(value, str) and not value.strip():
        return True
    if isinstance(value, (list, tuple, dict)) and not value:
        return True
    return False
