from __future__ import annotations

from collections import Counter
from pathlib import Path
from typing import Any

from chiyoda.environment.layout import (
    EXIT,
    PERSON,
    RESPONDER_ENTRY,
    WALL,
    Connector,
    Layout,
)
from chiyoda.scenarios.generated_calibration import (
    apply_generated_population_calibration,
)
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios.validation import (
    ScenarioValidationIssue,
    validate_scenario_config,
)

Cell = tuple[str, int, int]


def geometry_audit_file(path: str | Path) -> dict[str, Any]:
    manager = ScenarioManager()
    scenario = manager.load_config(str(path))
    audit = build_geometry_audit(scenario, manager=manager)
    audit["source_file"] = str(Path(path))
    return audit


def build_geometry_audit(
    scenario: dict[str, Any],
    *,
    manager: ScenarioManager | None = None,
) -> dict[str, Any]:
    manager = manager or ScenarioManager()
    sc = apply_generated_population_calibration(scenario)
    layout = manager._build_layout(sc)
    validation = validate_scenario_config(sc, manager=manager)
    connector_issues = _connector_issues(layout)
    issues = [issue.to_dict() for issue in validation.issues] + [
        issue.to_dict() for issue in connector_issues
    ]
    floor_summary = _floor_summary(layout)
    counts = _counts(floor_summary, layout, validation)
    has_errors = any(issue["severity"] == "error" for issue in issues)

    return {
        "ok": not has_errors,
        "scenario": str(sc.get("name", "")),
        "layout": {
            "floor_count": len(layout.floors),
            "primary_floor_id": layout.primary_floor_id,
            "cell_size_m": float(layout.cell_size),
            "origin": [float(value) for value in layout.origin],
            "floors": floor_summary,
        },
        "counts": counts,
        "connectors": [_connector_record(connector) for connector in layout.connectors],
        "starts": validation.starts,
        "reachability": {
            "reachable_cells": len(validation.reachable_cells),
            "unreachable_walkable_cells": [
                list(cell) for cell in validation.unreachable_walkable_cells
            ],
            "paths": {
                key: [list(cell) for cell in path]
                for key, path in validation.paths.items()
            },
        },
        "provenance": _provenance_summary(sc),
        "issues": issues,
    }


def _floor_summary(layout: Layout) -> dict[str, dict[str, Any]]:
    summary: dict[str, dict[str, Any]] = {}
    for floor_id, floor in layout.floors.items():
        grid = floor.grid
        summary[str(floor_id)] = {
            "width": int(grid.shape[1]),
            "height": int(grid.shape[0]),
            "z": float(floor.z),
            "walkable_cells": int((grid != WALL).sum()),
            "wall_cells": int((grid == WALL).sum()),
            "exit_cells": int((grid == EXIT).sum()),
            "spawn_cells": int((grid == PERSON).sum()),
            "responder_entry_cells": int((grid == RESPONDER_ENTRY).sum()),
        }
    return summary


def _counts(
    floor_summary: dict[str, dict[str, Any]], layout: Layout, validation: Any
) -> dict[str, int]:
    return {
        "floors": len(floor_summary),
        "walkable_cells": sum(
            int(floor["walkable_cells"]) for floor in floor_summary.values()
        ),
        "wall_cells": sum(int(floor["wall_cells"]) for floor in floor_summary.values()),
        "exit_cells": sum(int(floor["exit_cells"]) for floor in floor_summary.values()),
        "spawn_cells": sum(
            int(floor["spawn_cells"]) for floor in floor_summary.values()
        ),
        "responder_entry_cells": sum(
            int(floor["responder_entry_cells"]) for floor in floor_summary.values()
        ),
        "connectors": len(layout.connectors),
        "reachable_cells": len(validation.reachable_cells),
        "unreachable_walkable_cells": len(validation.unreachable_walkable_cells),
        "starts": len(validation.starts),
    }


def _connector_record(connector: Connector) -> dict[str, Any]:
    return {
        "id": connector.id,
        "type": connector.type,
        "from": _cell_record(connector.from_cell),
        "to": _cell_record(connector.to_cell),
        "bidirectional": bool(connector.bidirectional),
        "width_m": float(connector.width),
        "speed_multiplier": float(connector.speed_multiplier),
        "capacity": connector.capacity,
        "flow_rate": connector.flow_rate,
        "queue_mode": connector.queue_mode,
        "dwell_s": float(connector.dwell_s),
        "travel_s": float(connector.travel_s),
        "height_delta_m": float(connector.height_delta_m),
    }


def _cell_record(cell: Cell) -> dict[str, Any]:
    floor, x, y = cell
    return {"floor": str(floor), "x": int(x), "y": int(y)}


def _connector_issues(layout: Layout) -> list[ScenarioValidationIssue]:
    issues: list[ScenarioValidationIssue] = []
    ids = Counter(connector.id for connector in layout.connectors)
    for connector_id, count in ids.items():
        if count > 1:
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "duplicate_connector_id",
                    f"connector id '{connector_id}' appears {count} times",
                    source="layout.connectors",
                )
            )
    if len(layout.floors) > 1 and not layout.connectors:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "multifloor_without_connectors",
                "multi-floor layout has no configured connectors",
                source="layout.connectors",
            )
        )
    for connector in layout.connectors:
        source = f"layout.connectors.{connector.id}"
        if connector.width <= 0:
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "connector_nonpositive_width",
                    "connector width must be positive",
                    source=source,
                )
            )
        if connector.speed_multiplier <= 0:
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "connector_nonpositive_speed",
                    "connector speed multiplier must be positive",
                    source=source,
                )
            )
        if connector.capacity is not None and connector.capacity <= 0:
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "connector_nonpositive_capacity",
                    "connector capacity must be positive when configured",
                    source=source,
                )
            )
        if connector.from_cell == connector.to_cell:
            issues.append(
                ScenarioValidationIssue(
                    "warning",
                    "connector_self_loop",
                    "connector starts and ends on the same cell",
                    cell=connector.from_cell,
                    source=source,
                )
            )
        if (
            connector.type in {"stairs", "elevator", "escalator"}
            and connector.height_delta_m == 0
        ):
            issues.append(
                ScenarioValidationIssue(
                    "warning",
                    "vertical_connector_without_vertical_delta",
                    "vertical connector has zero floor or cell-height delta",
                    source=source,
                )
            )
        if (
            connector.type == "elevator"
            and connector.capacity is None
            and connector.travel_s <= 0
            and connector.dwell_s <= 0
        ):
            issues.append(
                ScenarioValidationIssue(
                    "warning",
                    "elevator_without_service_timing",
                    "elevator has no capacity, travel time, or dwell time calibration",
                    source=source,
                )
            )
    return issues


def _provenance_summary(scenario: dict[str, Any]) -> dict[str, Any]:
    metadata = scenario.get("metadata", {}) or {}
    provenance = metadata.get("station_provenance")
    if not isinstance(provenance, dict):
        provenance = {}
    return {
        "report_facing_station_case": bool(
            metadata.get("report_facing_station_case", False)
        ),
        "has_station_provenance": bool(provenance),
        "has_provenance_file": bool(metadata.get("provenance_file")),
        "station": str(provenance.get("station", "")),
        "source_url": str(provenance.get("source_url", "")),
        "license": str(provenance.get("license", "")),
        "validation_use": str(provenance.get("validation_use", "")),
        "known_missing_indoor_topology": list(
            provenance.get("known_missing_indoor_topology", []) or []
        ),
    }
