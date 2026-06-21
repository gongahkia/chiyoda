from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from chiyoda.environment.layout import PERSON, RESPONDER_ENTRY, WALL, Layout
from chiyoda.scenarios.generated_calibration import apply_generated_population_calibration
from chiyoda.scenarios.manager import ScenarioManager


Cell = tuple


@dataclass(frozen=True)
class ScenarioValidationIssue:
    severity: str
    code: str
    message: str
    cell: Cell | None = None
    source: str = ""

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "severity": self.severity,
            "code": self.code,
            "message": self.message,
        }
        if self.cell is not None:
            payload["cell"] = list(self.cell)
        if self.source:
            payload["source"] = self.source
        return payload


@dataclass(frozen=True)
class ScenarioValidationResult:
    layout_width: int
    layout_height: int
    exits: list[Cell]
    starts: list[dict[str, Any]]
    reachable_cells: list[Cell]
    unreachable_walkable_cells: list[Cell]
    paths: dict[str, list[Cell]]
    issues: list[ScenarioValidationIssue]

    @property
    def has_errors(self) -> bool:
        return any(issue.severity == "error" for issue in self.issues)

    def to_dict(self) -> dict[str, Any]:
        return {
            "ok": not self.has_errors,
            "layout_width": self.layout_width,
            "layout_height": self.layout_height,
            "exits": [list(cell) for cell in self.exits],
            "starts": self.starts,
            "reachable_cells": [list(cell) for cell in self.reachable_cells],
            "unreachable_walkable_cells": [list(cell) for cell in self.unreachable_walkable_cells],
            "paths": {
                key: [list(cell) for cell in path]
                for key, path in self.paths.items()
            },
            "issues": [issue.to_dict() for issue in self.issues],
        }


def validate_scenario_file(path: str | Path) -> ScenarioValidationResult:
    manager = ScenarioManager()
    scenario = manager.load_config(str(path))
    return validate_scenario_config(scenario, manager=manager)


def validate_scenario_config(
    scenario: dict[str, Any],
    *,
    manager: ScenarioManager | None = None,
) -> ScenarioValidationResult:
    manager = manager or ScenarioManager()
    sc = apply_generated_population_calibration(scenario)
    layout = manager._build_layout(sc)  # validates the same resolved layout used by runs
    exits = layout.exit_positions()
    walkable = _walkable_cells(layout)
    reachable, parent = _reachable_from_exits(layout, exits)
    starts = _scenario_starts(sc, layout)
    issues: list[ScenarioValidationIssue] = []

    if not walkable:
        issues.append(ScenarioValidationIssue("error", "no_walkable_cells", "layout has no walkable cells"))
    if not exits:
        issues.append(ScenarioValidationIssue("error", "no_exits", "layout has no exit cells"))

    for start in starts:
        cell = tuple(start["cell"])
        source = str(start["source"])
        label = str(start["label"])
        if not _in_bounds(layout, cell):
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "start_out_of_bounds",
                    f"{label} is outside the layout",
                    cell=cell,
                    source=source,
                )
            )
            continue
        if not layout.is_walkable(cell):
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "start_on_wall",
                    f"{label} is on a wall cell",
                    cell=cell,
                    source=source,
                )
            )
            continue
        if exits and cell not in reachable:
            issues.append(
                ScenarioValidationIssue(
                    "error",
                    "start_unreachable",
                    f"{label} cannot reach any exit",
                    cell=cell,
                    source=source,
                )
            )
        if cell in exits:
            issues.append(
                ScenarioValidationIssue(
                    "warning",
                    "start_on_exit",
                    f"{label} is already on an exit cell",
                    cell=cell,
                    source=source,
                )
            )

    population = sc.get("population", {}) or {}
    total = int(population.get("total", 0) or 0)
    has_people_starts = any(start["kind"] == "spawn" for start in starts)
    if total > 0 and not has_people_starts:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "implicit_population_spawn",
                "population has no explicit spawn cells; run will use layout @ cells or random walkable cells",
            )
        )
    responder_count = sum(int(cfg.get("count", 1) or 0) for cfg in sc.get("responders", []) or [])
    has_responder_starts = any(start["kind"] == "responder" for start in starts)
    if responder_count > 0 and not has_responder_starts:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "implicit_responder_spawn",
                "responders have no explicit spawn cells or R entry cells; run will use random walkable cells",
            )
        )

    unreachable = sorted(walkable - reachable)
    if exits and unreachable:
        issues.append(
            ScenarioValidationIssue(
                "warning",
                "unreachable_walkable_cells",
                f"{len(unreachable)} walkable cells cannot reach any exit",
            )
        )

    paths: dict[str, list[Cell]] = {}
    for index, start in enumerate(starts):
        cell = tuple(start["cell"])
        if _in_bounds(layout, cell) and layout.is_walkable(cell) and cell in reachable:
            paths[f"{start['kind']}_{index}"] = _path_to_exit(cell, parent)

    return ScenarioValidationResult(
        layout_width=layout.width,
        layout_height=layout.height,
        exits=sorted(exits),
        starts=starts,
        reachable_cells=sorted(reachable),
        unreachable_walkable_cells=unreachable,
        paths=paths,
        issues=issues,
    )


def _walkable_cells(layout: Layout) -> set[Cell]:
    return set(layout.all_walkable_cells())


def _reachable_from_exits(layout: Layout, exits: list[Cell]) -> tuple[set[Cell], dict[Cell, Cell | None]]:
    reached: set[Cell] = set()
    parent: dict[Cell, Cell | None] = {}
    queue: deque[Cell] = deque()
    for exit_cell in exits:
        if not _in_bounds(layout, exit_cell) or not layout.is_walkable(exit_cell):
            continue
        reached.add(exit_cell)
        parent[exit_cell] = None
        queue.append(exit_cell)
    while queue:
        cell = queue.popleft()
        for neighbor in _neighbors(layout, cell):
            if neighbor in reached or not layout.is_walkable(neighbor):
                continue
            reached.add(neighbor)
            parent[neighbor] = cell
            queue.append(neighbor)
    return reached, parent


def _scenario_starts(scenario: dict[str, Any], layout: Layout) -> list[dict[str, Any]]:
    starts: list[dict[str, Any]] = []
    for cell in _token_cells(layout, PERSON):
        starts.append({
            "kind": "spawn",
            "label": "layout spawn",
            "source": "layout.@",
            "cell": list(cell),
        })
    population = scenario.get("population", {}) or {}
    for cohort in population.get("cohorts", []) or []:
        name = str(cohort.get("name", "cohort"))
        for cell in cohort.get("spawn_cells", []) or []:
            parsed = _parse_cell(cell, layout)
            if parsed is not None:
                starts.append({
                    "kind": "spawn",
                    "label": f"cohort {name} spawn",
                    "source": f"population.cohorts.{name}.spawn_cells",
                    "cell": list(parsed),
                })
    for cell in _token_cells(layout, RESPONDER_ENTRY):
        starts.append({
            "kind": "responder",
            "label": "layout responder entry",
            "source": "layout.R",
            "cell": list(cell),
        })
    for index, cfg in enumerate(scenario.get("responders", []) or []):
        for cell in cfg.get("spawn_cells", []) or []:
            parsed = _parse_cell(cell, layout)
            if parsed is not None:
                starts.append({
                    "kind": "responder",
                    "label": f"responder group {index + 1} spawn",
                    "source": f"responders.{index}.spawn_cells",
                    "cell": list(parsed),
                })
    return starts


def _token_cells(layout: Layout, token: str) -> list[Cell]:
    return layout.positions_for_token(token)


def _parse_cell(value: Any, layout: Layout) -> Cell | None:
    try:
        if isinstance(value, dict):
            return layout.cell((str(value["floor"]), int(value["x"]), int(value["y"])))
        return layout.cell(value)
    except (TypeError, ValueError, IndexError, KeyError):
        return None


def _in_bounds(layout: Layout, cell: Cell) -> bool:
    floor_id, x, y = layout.cell(cell)
    floor = layout.floors.get(floor_id)
    return floor is not None and 0 <= x < floor.grid.shape[1] and 0 <= y < floor.grid.shape[0]


def _neighbors(layout: Layout, cell: Cell) -> list[Cell]:
    floor_id, x, y = layout.cell(cell)
    neighbors = [
        (floor_id, x + 1, y),
        (floor_id, x - 1, y),
        (floor_id, x, y + 1),
        (floor_id, x, y - 1),
    ]
    for source, target, _connector in layout.connector_edges():
        if source == (floor_id, x, y):
            neighbors.append(target)
    return neighbors


def _path_to_exit(start: Cell, parent: dict[Cell, Cell | None]) -> list[Cell]:
    path = [start]
    current = start
    while parent.get(current) is not None:
        current = parent[current]  # type: ignore[assignment]
        path.append(current)
    return path
