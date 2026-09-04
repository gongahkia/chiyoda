from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from chiyoda_analysis.bundle import BundleError, load_bundle, summarize


class BundleTests(unittest.TestCase):
    def test_loads_and_summarizes_a_valid_bundle(self) -> None:
        bundle = {
            "bundle_version": "0.1",
            "runtime_version": "0.1.0-alpha.1",
            "scenario_hash": "a" * 64,
            "scenario": {"language_version": "0.1", "scenario": {"name": "fixture"}},
            "options": {},
            "trace": [],
            "events": [],
            "metrics": {"total_agents": 0},
            "bundle_hash": "",
        }
        bundle["bundle_hash"] = _hash(bundle)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            path.write_text(json.dumps(bundle), encoding="utf-8")
            summary = summarize(load_bundle(path))
        self.assertEqual(summary["scenario"], "fixture")
        self.assertEqual(summary["frames"], 0)

    def test_rejects_a_tampered_bundle(self) -> None:
        bundle = {
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {},
            "bundle_hash": "0" * 64,
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            path.write_text(json.dumps(bundle), encoding="utf-8")
            with self.assertRaises(BundleError):
                load_bundle(path)


def _hash(bundle: dict[str, object]) -> str:
    unsigned = dict(bundle)
    unsigned["bundle_hash"] = ""
    payload = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


if __name__ == "__main__":
    unittest.main()

