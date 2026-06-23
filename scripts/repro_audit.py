#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import hashlib
import os
import re
import sys
from pathlib import Path

DEFAULT_DOC = Path("docs/reproducibility_kit.md")
HASH_LINE = re.compile(r"^([0-9a-f]{64})  (.+)$")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Audit reproducibility-kit SHA-256 hashes."
    )
    parser.add_argument("--doc", type=Path, default=DEFAULT_DOC)
    parser.add_argument("--root", type=Path, default=Path("."))
    args = parser.parse_args(argv)

    root = args.root.resolve()
    doc = args.doc if args.doc.is_absolute() else root / args.doc
    documented = documented_hash_lines(doc)
    if not documented:
        print(f"no SHA-256 manifest lines found in {doc}", file=sys.stderr)
        return 2

    expected = [f"{digest}  {path}" for digest, path in documented]
    actual = [f"{sha256_file(root / path)}  {path}" for _, path in documented]
    if expected == actual:
        print(f"repro audit ok: {len(actual)} artifact hashes match")
        return 0

    diff = "\n".join(
        difflib.unified_diff(
            expected,
            actual,
            fromfile="docs/reproducibility_kit.md",
            tofile="computed hashes",
            lineterm="",
        )
    )
    print("repro audit failed: artifact hash drift detected", file=sys.stderr)
    print(diff, file=sys.stderr)
    write_step_summary(diff)
    return 1


def documented_hash_lines(doc: Path) -> list[tuple[str, Path]]:
    rows: list[tuple[str, Path]] = []
    for line in doc.read_text().splitlines():
        match = HASH_LINE.match(line.strip())
        if match:
            rows.append((match.group(1), Path(match.group(2))))
    return rows


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_step_summary(diff: str) -> None:
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary:
        return
    with Path(summary).open("a") as handle:
        handle.write("### Reproducibility hash drift\n\n")
        handle.write("```diff\n")
        handle.write(diff)
        handle.write("\n```\n")


if __name__ == "__main__":
    raise SystemExit(main())
