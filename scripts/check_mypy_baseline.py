from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

BASELINE_DOC = Path("docs/typing_baseline.md")
SUMMARY_RE = re.compile(
    r"Found (?P<errors>\d+) errors? in (?P<files>\d+) files? "
    r"\(checked (?P<checked>\d+) source files?\)"
)
SUCCESS_RE = re.compile(r"Success: no issues found in (?P<checked>\d+) source files?")


def main() -> int:
    baseline = read_baseline(BASELINE_DOC)
    result = subprocess.run(
        [sys.executable, "-m", "mypy", "chiyoda"],
        check=False,
        capture_output=True,
        text=True,
    )
    output = "\n".join(part for part in (result.stdout, result.stderr) if part)
    print(output, end="" if output.endswith("\n") else "\n")
    errors, _, _ = parse_mypy_summary(output)
    if errors > baseline:
        print(f"mypy errors {errors} exceed baseline {baseline}", file=sys.stderr)
        return 1
    print(f"mypy errors {errors} within baseline {baseline}")
    return 0


def read_baseline(path: Path) -> int:
    rows = [
        line
        for line in path.read_text().splitlines()
        if line.startswith("| 20") and "|" in line
    ]
    if not rows:
        raise ValueError(f"No baseline row found in {path}")
    cells = [cell.strip() for cell in rows[-1].strip("|").split("|")]
    return int(cells[3])


def parse_mypy_summary(output: str) -> tuple[int, int, int]:
    match = SUMMARY_RE.search(output)
    if match is not None:
        return (
            int(match.group("errors")),
            int(match.group("files")),
            int(match.group("checked")),
        )
    success = SUCCESS_RE.search(output)
    if success is not None:
        return (0, 0, int(success.group("checked")))
    raise ValueError("Could not parse mypy summary")


if __name__ == "__main__":
    raise SystemExit(main())
