"""Read and verify Chiyoda run bundles without coupling analysis to the runtime."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any


class BundleError(ValueError):
    """A run bundle is malformed or fails its integrity contract."""


def load_bundle(path: str | Path, *, verify: bool = True) -> dict[str, Any]:
    """Load a `run.json` bundle and optionally verify its SHA-256 digest.

    The Rust reference runtime hashes compact JSON in declaration order with the
    `bundle_hash` field set to an empty string. Python's standard JSON decoder
    preserves object order, allowing this reader to independently reproduce the
    version-0.1 hash without importing the simulator.
    """

    bundle_path = Path(path)
    try:
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    except OSError as error:
        raise BundleError(f"cannot read {bundle_path}: {error}") from error
    except json.JSONDecodeError as error:
        raise BundleError(f"invalid JSON in {bundle_path}: {error}") from error
    if not isinstance(bundle, dict):
        raise BundleError("run bundle root must be a JSON object")
    if verify:
        _verify_hash(bundle)
    return bundle


def summarize(bundle: dict[str, Any]) -> dict[str, Any]:
    """Return a stable, analysis-friendly summary of a verified bundle."""

    metrics = bundle.get("metrics")
    scenario = bundle.get("scenario")
    if not isinstance(metrics, dict) or not isinstance(scenario, dict):
        raise BundleError("bundle does not contain version-0.1 scenario and metrics objects")
    scenario_body = scenario.get("scenario")
    if not isinstance(scenario_body, dict):
        raise BundleError("bundle scenario body is malformed")
    return {
        "bundle_hash": bundle.get("bundle_hash"),
        "scenario_hash": bundle.get("scenario_hash"),
        "scenario": scenario_body.get("name"),
        "runtime_version": bundle.get("runtime_version"),
        "frames": len(bundle.get("trace", [])),
        "events": len(bundle.get("events", [])),
        "metrics": metrics,
    }


def _verify_hash(bundle: dict[str, Any]) -> None:
    supplied = bundle.get("bundle_hash")
    if not isinstance(supplied, str) or len(supplied) != 64:
        raise BundleError("bundle_hash must be a SHA-256 hexadecimal digest")
    unsigned = dict(bundle)
    unsigned["bundle_hash"] = ""
    payload = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    actual = hashlib.sha256(payload).hexdigest()
    if actual != supplied:
        raise BundleError(f"bundle integrity check failed: expected {supplied}, calculated {actual}")

