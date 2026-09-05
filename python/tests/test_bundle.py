from __future__ import annotations

import hashlib
import json
import re
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from chiyoda_analysis import bundle as bundle_reader
from chiyoda_analysis.bundle import BundleError, load_bundle, summarize
from chiyoda_analysis.evidence import EvidenceError, load_catalog, verify_catalog_files


class BundleTests(unittest.TestCase):
    def test_reader_covers_the_current_rust_bundle_contract(self) -> None:
        bundle_source = (Path(__file__).parents[2] / "crates/chiyoda-core/src/bundle.rs").read_text(
            encoding="utf-8"
        )
        match = re.search(r'pub const BUNDLE_VERSION: &str = "([^"]+)";', bundle_source)
        self.assertIsNotNone(match, "Rust bundle version declaration must remain machine-readable")
        self.assertIn(match.group(1), bundle_reader._CURRENT_BUNDLE_VERSIONS)

    def test_loads_and_summarizes_a_valid_bundle(self) -> None:
        bundle = {
            "bundle_version": "0.1",
            "runtime_version": "0.1.0-alpha.1",
            "scenario_hash": "a" * 64,
            "scenario": {"language_version": "0.1", "scenario": {"name": "fixture"}},
            "options": {},
            "trace": [],
            "events": [],
            "metrics": {
                "total_agents": 0,
                "evacuated_by_exit": {"street": 2},
                "remaining_by_state": {"moving": 3},
            },
            "bundle_hash": "",
        }
        bundle["bundle_hash"] = _hash(bundle)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            path.write_text(json.dumps(bundle), encoding="utf-8")
            summary = summarize(load_bundle(path))
        self.assertEqual(summary["scenario"], "fixture")
        self.assertEqual(summary["frames"], 0)
        self.assertEqual(summary["evacuated_by_exit"], {"street": 2})
        self.assertEqual(summary["remaining_by_state"], {"moving": 3})

    def test_summary_exposes_partial_run_last_exit_without_calling_it_clearance(self) -> None:
        bundle = {
            "bundle_version": "0.17",
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {
                "total_agents": 2,
                "evacuated_agents": 1,
                "clearance_time_s": None,
                "last_exit_time_s": 4.5,
            },
        }

        summary = summarize(bundle)

        self.assertIsNone(summary["clearance_time_s"])
        self.assertEqual(summary["last_exit_time_s"], 4.5)

    def test_summary_rejects_partial_017_run_labeled_as_cleared(self) -> None:
        bundle = {
            "bundle_version": "0.17",
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {
                "total_agents": 2,
                "evacuated_agents": 1,
                "clearance_time_s": 4.5,
                "last_exit_time_s": 4.5,
            },
        }

        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_rejects_partial_current_run_labeled_as_cleared(self) -> None:
        bundle = {
            "bundle_version": "0.21",
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {
                "total_agents": 2,
                "evacuated_agents": 1,
                "clearance_time_s": 4.5,
                "last_exit_time_s": 4.5,
            },
        }

        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_exposes_information_delivery_and_acceptance(self) -> None:
        bundle = {
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {
                "information_delivery": {
                    "notice": {
                        "kind": "message",
                        "received_agents": 7,
                        "accepted_agents": 5,
                    }
                }
            },
        }

        summary = summarize(bundle)

        self.assertEqual(summary["information_delivery"]["notice"]["accepted_agents"], 5)

    def test_current_summary_exposes_discrete_queue_telemetry(self) -> None:
        queue_metrics = {
            "lift": {
                "ever_queued_agents": 1,
                "cumulative_wait_agent_seconds": 1.5,
                "peak_waiting_agents": 1,
            },
            "connector": {
                "ever_queued_agents": 0,
                "cumulative_wait_agent_seconds": 0.0,
                "peak_waiting_agents": 0,
            },
            "gate": {
                "ever_queued_agents": 2,
                "cumulative_wait_agent_seconds": 3.0,
                "peak_waiting_agents": 2,
            },
            "exit": {
                "ever_queued_agents": 0,
                "cumulative_wait_agent_seconds": 0.0,
                "peak_waiting_agents": 0,
            },
        }
        bundle = {
            "bundle_version": "0.22",
            "scenario": {"scenario": {"name": "fixture", "messages": [], "countermeasures": []}},
            "metrics": {
                "total_agents": 2,
                "evacuated_agents": 0,
                "remaining_by_state": {"waiting_for_gate": 2},
                "clearance_time_s": None,
                "last_exit_time_s": None,
                "queued_for_lift_agents": 1,
                "queued_for_connector_agents": 0,
                "queued_for_gate_agents": 2,
                "queued_for_exit_agents": 0,
                "queue_metrics": queue_metrics,
            },
        }

        summary = summarize(bundle)

        self.assertEqual(summary["queue_metrics"]["gate"]["peak_waiting_agents"], 2)
        self.assertEqual(summary["queue_metrics"]["lift"]["cumulative_wait_agent_seconds"], 1.5)
        bundle["metrics"]["queued_for_gate_agents"] = 1
        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_current_summary_exposes_resource_attributed_queue_telemetry(self) -> None:
        queue_metrics = {
            "lift": {
                "ever_queued_agents": 1,
                "cumulative_wait_agent_seconds": 1.5,
                "peak_waiting_agents": 1,
            },
            "connector": {
                "ever_queued_agents": 0,
                "cumulative_wait_agent_seconds": 0.0,
                "peak_waiting_agents": 0,
            },
            "gate": {
                "ever_queued_agents": 2,
                "cumulative_wait_agent_seconds": 3.0,
                "peak_waiting_agents": 2,
            },
            "exit": {
                "ever_queued_agents": 0,
                "cumulative_wait_agent_seconds": 0.0,
                "peak_waiting_agents": 0,
            },
            "by_resource": {
                "lifts": {
                    "accessible_lift": {
                        "ever_queued_agents": 1,
                        "cumulative_wait_agent_seconds": 1.5,
                        "peak_waiting_agents": 1,
                    }
                },
                "connectors": {},
                "gates": {
                    "fare_gate": {
                        "ever_queued_agents": 2,
                        "cumulative_wait_agent_seconds": 3.0,
                        "peak_waiting_agents": 2,
                    }
                },
                "exits": {},
            },
        }
        bundle = {
            "bundle_version": "0.23",
            "scenario": {
                "scenario": {
                    "name": "fixture",
                    "messages": [],
                    "countermeasures": [],
                    "connectors": [{"Lift": {"id": "accessible_lift"}}],
                    "gates": [{"id": "fare_gate"}],
                    "exits": [],
                }
            },
            "metrics": {
                "total_agents": 2,
                "evacuated_agents": 0,
                "remaining_by_state": {"waiting_for_gate": 2},
                "clearance_time_s": None,
                "last_exit_time_s": None,
                "queued_for_lift_agents": 1,
                "queued_for_connector_agents": 0,
                "queued_for_gate_agents": 2,
                "queued_for_exit_agents": 0,
                "queue_metrics": queue_metrics,
            },
        }

        summary = summarize(bundle)

        self.assertEqual(
            summary["queue_metrics"]["by_resource"]["gates"]["fare_gate"]["peak_waiting_agents"],
            2,
        )
        queue_metrics["by_resource"]["gates"]["fare_gate"]["cumulative_wait_agent_seconds"] = 2.0
        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_rejects_information_acceptance_above_delivery(self) -> None:
        bundle = {
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {
                "information_delivery": {
                    "notice": {
                        "kind": "message",
                        "received_agents": 1,
                        "accepted_agents": 2,
                    }
                }
            },
        }

        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_rejects_missing_018_information_delivery(self) -> None:
        bundle = {
            "bundle_version": "0.18",
            "scenario": {
                "scenario": {
                    "name": "fixture",
                    "messages": [{"id": "notice"}],
                    "countermeasures": [],
                }
            },
            "metrics": {
                "total_agents": 1,
                "evacuated_agents": 0,
                "clearance_time_s": None,
                "last_exit_time_s": None,
            },
        }

        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_rejects_missing_current_information_delivery(self) -> None:
        bundle = {
            "bundle_version": "0.21",
            "scenario": {
                "scenario": {
                    "name": "fixture",
                    "messages": [{"id": "notice"}],
                    "countermeasures": [],
                }
            },
            "metrics": {
                "total_agents": 1,
                "evacuated_agents": 0,
                "clearance_time_s": None,
                "last_exit_time_s": None,
            },
        }

        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_defaults_missing_exit_attribution_for_older_bundles(self) -> None:
        bundle = {
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {},
            "bundle_hash": "",
        }
        self.assertEqual(summarize(bundle)["evacuated_by_exit"], {})
        self.assertEqual(summarize(bundle)["remaining_by_state"], {})

    def test_summary_rejects_invalid_exit_attribution(self) -> None:
        bundle = {
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {"evacuated_by_exit": {"street": -1}},
            "bundle_hash": "",
        }
        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_summary_rejects_invalid_remaining_state_attribution(self) -> None:
        bundle = {
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {"remaining_by_state": {"moving": -1}},
            "bundle_hash": "",
        }
        with self.assertRaises(BundleError):
            summarize(bundle)

    def test_current_summary_rejects_invalid_exit_and_remaining_attribution(self) -> None:
        exit_bundle = {
            "bundle_version": "0.21",
            "scenario": {"scenario": {"name": "fixture", "exits": [{"id": "street"}]}},
            "metrics": {
                "total_agents": 1,
                "evacuated_agents": 1,
                "evacuated_by_exit": {"unknown": 1},
                "clearance_time_s": 1.0,
                "last_exit_time_s": 1.0,
            },
        }
        remaining_bundle = {
            "bundle_version": "0.21",
            "scenario": {"scenario": {"name": "fixture"}},
            "metrics": {
                "total_agents": 2,
                "evacuated_agents": 1,
                "remaining_by_state": {"evacuated": 1},
                "clearance_time_s": None,
                "last_exit_time_s": 1.0,
            },
        }

        with self.assertRaises(BundleError):
            summarize(exit_bundle)
        with self.assertRaises(BundleError):
            summarize(remaining_bundle)

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

    def test_evidence_lock_verifies_content_without_network_access(self) -> None:
        payload = b"locked evidence"
        digest = hashlib.sha256(payload).hexdigest()
        catalog = {
            "schema_version": "0.1",
            "dataset_id": "fixture",
            "title": "Fixture",
            "landing_page": "https://example.test/record",
            "license": "CC-BY-4.0",
            "redistributable": True,
            "citation": "Fixture (2026)",
            "files": [
                _source("calibration", "calibration.bin", digest, len(payload)),
                _source("held_out", "held-out.bin", digest, len(payload)),
            ],
            "supported_primitives": "fixture only",
            "exclusions": "everything else",
            "split_rationale": "separate fixture files",
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "calibration.bin").write_bytes(payload)
            (root / "held-out.bin").write_bytes(payload)
            catalog_path = root / "catalog.json"
            catalog_path.write_text(json.dumps(catalog), encoding="utf-8")
            self.assertEqual(len(verify_catalog_files(load_catalog(catalog_path), root)), 2)
            (root / "held-out.bin").write_bytes(b"tampered")
            with self.assertRaises(EvidenceError):
                verify_catalog_files(load_catalog(catalog_path), root)

    def test_evidence_catalog_rejects_path_traversal(self) -> None:
        catalog = {
            "schema_version": "0.1",
            "dataset_id": "fixture",
            "title": "Fixture",
            "license": "CC-BY-4.0",
            "redistributable": True,
            "landing_page": "https://example.test/record",
            "citation": "Fixture (2026)",
            "files": [
                _source("calibration", "../outside", "a" * 64, 1),
                _source("held_out", "inside", "b" * 64, 1),
            ],
            "supported_primitives": "fixture only",
            "exclusions": "everything else",
            "split_rationale": "separate fixture files",
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "catalog.json"
            path.write_text(json.dumps(catalog), encoding="utf-8")
            with self.assertRaises(EvidenceError):
                load_catalog(path)

    def test_uncalibrated_reference_catalog_locks_open_data_without_a_split(self) -> None:
        payload = b"open reference"
        digest = hashlib.sha256(payload).hexdigest()
        catalog = {
            "schema_version": "0.1",
            "purpose": "uncalibrated_reference",
            "dataset_id": "reference-fixture",
            "title": "Reference fixture",
            "landing_page": "https://example.test/record",
            "license": "CC-BY-4.0",
            "redistributable": True,
            "citation": "Fixture (2026)",
            "files": [
                {
                    "id": "reference-source",
                    "source_url": "https://example.test/reference.bin",
                    "local_path": "reference.bin",
                    "sha256": digest,
                    "size_bytes": len(payload),
                    "transformation": "retain source values",
                }
            ],
            "supported_primitives": "fixture only",
            "exclusions": "empirical evaluation",
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "reference.bin").write_bytes(payload)
            catalog_path = root / "catalog.json"
            catalog_path.write_text(json.dumps(catalog), encoding="utf-8")
            self.assertEqual(len(verify_catalog_files(load_catalog(catalog_path), root)), 1)

    def test_odbl_source_observation_requires_attribution(self) -> None:
        payload = b"open map extract"
        digest = hashlib.sha256(payload).hexdigest()
        catalog = {
            "schema_version": "0.1",
            "purpose": "uncalibrated_reference",
            "dataset_id": "osm-fixture",
            "title": "OpenStreetMap fixture",
            "landing_page": "https://www.openstreetmap.org/",
            "license": "ODbL-1.0",
            "redistributable": True,
            "attribution": "© OpenStreetMap contributors",
            "citation": "OpenStreetMap contributors",
            "files": [
                {
                    "id": "extract",
                    "source_url": "https://example.test/station.osm",
                    "local_path": "station.osm",
                    "sha256": digest,
                    "size_bytes": len(payload),
                    "transformation": "inspect as source observations only",
                }
            ],
            "supported_primitives": "mapped tags only",
            "exclusions": "scenario geometry and operational claims",
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "catalog.json"
            path.write_text(json.dumps(catalog), encoding="utf-8")
            self.assertEqual(load_catalog(path)["license"], "ODbL-1.0")
            del catalog["attribution"]
            path.write_text(json.dumps(catalog), encoding="utf-8")
            with self.assertRaises(EvidenceError):
                load_catalog(path)


def _hash(bundle: dict[str, object]) -> str:
    unsigned = dict(bundle)
    unsigned["bundle_hash"] = ""
    payload = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _source(role: str, local_path: str, digest: str, size: int) -> dict[str, object]:
    return {
        "id": f"{role}-source",
        "role": role,
        "source_url": f"https://example.test/{local_path}",
        "local_path": local_path,
        "sha256": digest,
        "size_bytes": size,
        "upstream_checksum": "md5:fixture",
        "transformation": "retain source values",
    }


if __name__ == "__main__":
    unittest.main()
