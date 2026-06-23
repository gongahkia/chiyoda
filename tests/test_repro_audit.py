from __future__ import annotations

import hashlib
import subprocess
import sys


def test_repro_audit_passes_when_hashes_match(tmp_path):
    artifact = tmp_path / "artifact.txt"
    artifact.write_text("stable\n")
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    doc = tmp_path / "kit.md"
    doc.write_text(f"{digest}  artifact.txt\n")

    result = subprocess.run(
        [
            sys.executable,
            "scripts/repro_audit.py",
            "--doc",
            str(doc),
            "--root",
            str(tmp_path),
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0
    assert "repro audit ok" in result.stdout


def test_repro_audit_prints_diff_on_hash_drift(tmp_path):
    artifact = tmp_path / "artifact.txt"
    artifact.write_text("changed\n")
    actual = hashlib.sha256(artifact.read_bytes()).hexdigest()
    documented = "0" * 64
    doc = tmp_path / "kit.md"
    doc.write_text(f"{documented}  artifact.txt\n")

    result = subprocess.run(
        [
            sys.executable,
            "scripts/repro_audit.py",
            "--doc",
            str(doc),
            "--root",
            str(tmp_path),
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 1
    assert f"-{documented}  artifact.txt" in result.stderr
    assert f"+{actual}  artifact.txt" in result.stderr
