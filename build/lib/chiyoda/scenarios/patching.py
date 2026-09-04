from __future__ import annotations

import hashlib
import json
from copy import deepcopy
from pathlib import Path
from typing import Any

import yaml


def apply_exported_patch_file(path: str | Path) -> dict[str, Any]:
    export_path = Path(path)
    payload = yaml.safe_load(export_path.read_text()) or {}
    origin = payload.get("origin", {}) or {}
    patch = payload.get("patch", {}) or {}
    origin_path = Path(str(origin.get("path", "")))
    if not origin_path.is_absolute():
        origin_path = export_path.parent / origin_path
    if not origin_path.exists():
        raise FileNotFoundError(origin_path)
    expected_sha = str(origin.get("sha256", ""))
    actual_sha = hashlib.sha256(origin_path.read_bytes()).hexdigest()
    if expected_sha and actual_sha != expected_sha:
        raise ValueError("origin sha256 mismatch")
    source_payload = yaml.safe_load(origin_path.read_text()) or {}
    source = source_payload.get("scenario", source_payload)
    return apply_json_patch(source, patch.get("ops", []) or [])


def exported_scenario_body(path: str | Path) -> dict[str, Any]:
    payload = yaml.safe_load(Path(path).read_text()) or {}
    scenario = payload.get("scenario", payload)
    return scenario if isinstance(scenario, dict) else {}


def canonical_scenario_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    ).encode()


def apply_json_patch(source: Any, ops: list[dict[str, Any]]) -> Any:
    document = deepcopy(source)
    for op in ops:
        action = str(op.get("op", ""))
        path = str(op.get("path", ""))
        if action in {"add", "replace"}:
            document = _set_pointer(document, path, deepcopy(op.get("value")), action)
        elif action == "remove":
            document = _remove_pointer(document, path)
        else:
            raise ValueError(f"unsupported patch op: {action}")
    return document


def _pointer_parts(path: str) -> list[str]:
    if path == "":
        return []
    if not path.startswith("/"):
        raise ValueError(f"invalid json pointer: {path}")
    return [part.replace("~1", "/").replace("~0", "~") for part in path[1:].split("/")]


def _set_pointer(document: Any, path: str, value: Any, action: str) -> Any:
    parts = _pointer_parts(path)
    if not parts:
        return value
    parent = _resolve_parent(document, parts)
    key = parts[-1]
    if isinstance(parent, list):
        if key == "-" and action == "add":
            parent.append(value)
            return document
        index = int(key)
        if action == "add":
            parent.insert(index, value)
        else:
            parent[index] = value
        return document
    if action == "replace" and key not in parent:
        raise KeyError(key)
    parent[key] = value
    return document


def _remove_pointer(document: Any, path: str) -> Any:
    parts = _pointer_parts(path)
    if not parts:
        return None
    parent = _resolve_parent(document, parts)
    key = parts[-1]
    if isinstance(parent, list):
        del parent[int(key)]
    else:
        del parent[key]
    return document


def _resolve_parent(document: Any, parts: list[str]) -> Any:
    current = document
    for part in parts[:-1]:
        current = current[int(part)] if isinstance(current, list) else current[part]
    return current
