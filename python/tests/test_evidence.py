from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from chiyoda_analysis.evidence import EvidenceError, verify_catalog_files


class EvidenceArchiveTests(unittest.TestCase):
    def test_archive_members_lock_disjoint_empirical_partitions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive_path = root / "trials.zip"
            calibration = b"calibration trial"
            held_out = b"held-out trial"
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("trials/calibration.txt", calibration)
                archive.writestr("trials/held-out.txt", held_out)
            archive_bytes = archive_path.read_bytes()
            catalog = {
                "schema_version": "0.1",
                "dataset_id": "archive-fixture",
                "title": "Archive fixture",
                "landing_page": "https://example.test/archive",
                "license": "CC0-1.0",
                "redistributable": True,
                "citation": "Fixture (2026)",
                "files": [
                    {
                        "id": "archive",
                        "source_url": "https://example.test/trials.zip",
                        "local_path": "trials.zip",
                        "sha256": hashlib.sha256(archive_bytes).hexdigest(),
                        "size_bytes": len(archive_bytes),
                        "transformation": "retain the publisher ZIP unchanged",
                    }
                ],
                "archive_members": [
                    {
                        "id": "calibration",
                        "archive_file_id": "archive",
                        "member_path": "trials/calibration.txt",
                        "role": "calibration",
                        "sha256": hashlib.sha256(calibration).hexdigest(),
                        "size_bytes": len(calibration),
                        "transformation": "read the source trial unchanged",
                    },
                    {
                        "id": "held-out",
                        "archive_file_id": "archive",
                        "member_path": "trials/held-out.txt",
                        "role": "held_out",
                        "sha256": hashlib.sha256(held_out).hexdigest(),
                        "size_bytes": len(held_out),
                        "transformation": "read the source trial unchanged",
                    },
                ],
                "supported_primitives": "fixture horizontal avoidance",
                "exclusions": "all other primitives",
                "split_rationale": "named archive members are disjoint trials",
            }

            self.assertEqual(verify_catalog_files(catalog, root), [archive_path])
            catalog["archive_members"][1]["sha256"] = "0" * 64
            with self.assertRaises(EvidenceError):
                verify_catalog_files(catalog, root)


if __name__ == "__main__":
    unittest.main()
