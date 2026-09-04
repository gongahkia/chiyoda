"""Acquire immutable research sources without placing raw data under version control."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.error import URLError
from urllib.request import urlopen


class EvidenceError(ValueError):
    """An evidence catalog or its acquired source violates the lock contract."""


def load_catalog(path: str | Path) -> dict[str, Any]:
    """Read the shared Rust/Python version-0.1 evidence-catalog representation."""

    catalog_path = Path(path)
    try:
        value = json.loads(catalog_path.read_text(encoding="utf-8"))
    except OSError as error:
        raise EvidenceError(f"cannot read {catalog_path}: {error}") from error
    except json.JSONDecodeError as error:
        raise EvidenceError(f"invalid JSON in {catalog_path}: {error}") from error
    if not isinstance(value, dict):
        raise EvidenceError("evidence catalog root must be a JSON object")
    _validate_catalog(value)
    return value


def fetch_catalog(catalog: dict[str, Any], data_root: str | Path) -> list[Path]:
    """Download every locked source atomically, refusing to replace bad local data.

    Files are downloaded into a sibling ``.part`` file and only moved into place
    after their exact byte count and SHA-256 digest match the catalog.
    """

    _validate_catalog(catalog)
    root = Path(data_root)
    written: list[Path] = []
    for source in _files(catalog):
        destination = _destination(root, source["local_path"])
        if destination.exists():
            _verify_file(destination, source)
            written.append(destination)
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = destination.with_name(f".{destination.name}.part")
        if temporary.exists():
            temporary.unlink()
        try:
            _download(source["source_url"], temporary)
            _verify_file(temporary, source)
            os.replace(temporary, destination)
        except (OSError, URLError) as error:
            temporary.unlink(missing_ok=True)
            raise EvidenceError(f"cannot acquire {source['id']}: {error}") from error
        except EvidenceError:
            temporary.unlink(missing_ok=True)
            raise
        written.append(destination)
    return written


def verify_catalog_files(catalog: dict[str, Any], data_root: str | Path) -> list[Path]:
    """Verify the local content locks without network access."""

    _validate_catalog(catalog)
    root = Path(data_root)
    paths: list[Path] = []
    for source in _files(catalog):
        path = _destination(root, source["local_path"])
        _verify_file(path, source)
        paths.append(path)
    return paths


def _validate_catalog(catalog: dict[str, Any]) -> None:
    if catalog.get("schema_version") != "0.1":
        raise EvidenceError("schema_version must be `0.1`")
    for key in (
        "dataset_id",
        "title",
        "landing_page",
        "license",
        "citation",
        "supported_primitives",
        "exclusions",
    ):
        if not isinstance(catalog.get(key), str) or not catalog[key].strip():
            raise EvidenceError(f"catalog field `{key}` must be a non-empty string")
    license_id = catalog.get("license")
    if license_id not in {"CC-BY-4.0", "ODbL-1.0"} or catalog.get("redistributable") is not True:
        raise EvidenceError(
            "catalog must declare a redistributable CC-BY-4.0 or ODbL-1.0 source"
        )
    landing_page = catalog.get("landing_page")
    if not isinstance(landing_page, str) or not landing_page.startswith("https://"):
        raise EvidenceError("landing_page must be an HTTPS URL")
    purpose = catalog.get("purpose", "empirical_evaluation")
    if purpose not in {"empirical_evaluation", "uncalibrated_reference"}:
        raise EvidenceError(
            "purpose must be `empirical_evaluation` or `uncalibrated_reference`"
        )
    if license_id == "ODbL-1.0":
        attribution = catalog.get("attribution")
        if not isinstance(attribution, str) or not attribution.strip():
            raise EvidenceError("attribution must be a non-empty string for an ODbL-1.0 source")
    files = _files(catalog)
    if purpose == "empirical_evaluation":
        if license_id != "CC-BY-4.0":
            raise EvidenceError(
                "empirical evaluation requires CC-BY-4.0; ODbL-1.0 is limited to uncalibrated source observation"
            )
        split_rationale = catalog.get("split_rationale")
        if not isinstance(split_rationale, str) or not split_rationale.strip():
            raise EvidenceError(
                "split_rationale is required for empirical evaluation and must not be empty"
            )
        roles = {source.get("role") for source in files}
        if roles != {"calibration", "held_out"}:
            raise EvidenceError(
                "empirical-evaluation catalog must contain calibration and held_out source files"
            )
    elif any(source.get("role") is not None for source in files):
        raise EvidenceError(
            "uncalibrated reference sources must not declare calibration or held_out roles"
        )
    seen_ids: set[str] = set()
    seen_paths: set[str] = set()
    for source in files:
        for key in ("id", "source_url", "local_path", "sha256", "transformation"):
            if not isinstance(source.get(key), str) or not source[key].strip():
                raise EvidenceError(f"source field `{key}` must be a non-empty string")
        upstream_checksum = source.get("upstream_checksum")
        if upstream_checksum is not None and (
            not isinstance(upstream_checksum, str) or not upstream_checksum.strip()
        ):
            raise EvidenceError(
                f"source `{source['id']}` upstream_checksum must be a non-empty string when provided"
            )
        if not source["source_url"].startswith("https://"):
            raise EvidenceError(f"source `{source['id']}` must use HTTPS")
        if len(source["sha256"]) != 64 or any(character not in "0123456789abcdefABCDEF" for character in source["sha256"]):
            raise EvidenceError(f"source `{source['id']}` has an invalid SHA-256 digest")
        if not isinstance(source.get("size_bytes"), int) or source["size_bytes"] <= 0:
            raise EvidenceError(f"source `{source['id']}` must have a positive byte size")
        if source["id"] in seen_ids or source["local_path"] in seen_paths:
            raise EvidenceError("source ids and local paths must be unique")
        seen_ids.add(source["id"])
        seen_paths.add(source["local_path"])
        _safe_relative_path(source["local_path"])


def _files(catalog: dict[str, Any]) -> list[dict[str, Any]]:
    files = catalog.get("files")
    if not isinstance(files, list) or not files or not all(isinstance(source, dict) for source in files):
        raise EvidenceError("files must be a non-empty list of objects")
    return files


def _destination(root: Path, local_path: str) -> Path:
    safe_path = _safe_relative_path(local_path)
    return root.joinpath(*safe_path.parts)


def _safe_relative_path(value: str) -> PurePosixPath:
    path = PurePosixPath(value)
    if not value or path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise EvidenceError("local_path must be a non-empty relative path without traversal")
    return path


def _download(url: str, destination: Path) -> None:
    with urlopen(url, timeout=60) as response, destination.open("xb") as output:
        while chunk := response.read(128 * 1024):
            output.write(chunk)
    # Verification is deliberately repeated from disk after close, making the
    # actual lock check identical for newly fetched and already present files.


def _verify_file(path: Path, source: dict[str, Any]) -> None:
    try:
        stat = path.stat()
    except OSError as error:
        raise EvidenceError(f"cannot read acquired source {path}: {error}") from error
    if stat.st_size != source["size_bytes"]:
        raise EvidenceError(
            f"{path}: expected {source['size_bytes']} bytes, found {stat.st_size}"
        )
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            while chunk := handle.read(128 * 1024):
                digest.update(chunk)
    except OSError as error:
        raise EvidenceError(f"cannot hash {path}: {error}") from error
    actual = digest.hexdigest()
    if actual.lower() != source["sha256"].lower():
        raise EvidenceError(
            f"{path}: expected {source['sha256']}, calculated {actual}"
        )
