"""Real owned-resource cleanup regression; requires Podman and rebuilt local binaries."""

import argparse
import json
import os
from pathlib import Path
import tempfile
import unittest

from qualify import Qualification


class OwnedFixtureCleanup(unittest.TestCase):
    def test_failed_server_start_removes_recorded_database_volume(self):
        server = Path(__file__).resolve().parents[2]
        directory = Path(os.environ.get(
            "DRASI_MYSQL_FIXTURE_RECORDS", str(server / "target/native-mysql-fixtures")
        ))
        directory.mkdir(parents=True, exist_ok=True)
        evidence = Path(tempfile.mkdtemp(prefix="failed-server-", dir=directory)) / "proof"
        plugins = Path(os.environ.get("DRASI_NATIVE_PLUGIN_DIR", str(server / "target/debug")))
        qualification = Qualification(argparse.Namespace(
            evidence=evidence,
            plugins=plugins,
            generator=server / "target/debug/examples/native-mysql",
            server=evidence / "intentionally-missing-server",
        ))
        try:
            qualification.setup()
            ownership = qualification.observations["database_ownership"]
            self.assertTrue(ownership["volumes"])
            with self.assertRaises(FileNotFoundError):
                qualification.start_server("expected-failure")
        finally:
            qualification.cleanup()
        ownership = json.loads((evidence / "resource-ownership.json").read_text())
        self.assertTrue(ownership["removed"])
        self.assertEqual(ownership["remaining_owned_volume_count"], 0)
        self.assertEqual(ownership["remaining_owned_volume_bytes"], 0)
        for volume in ownership["volumes"]:
            self.assertEqual(
                qualification.run(["podman", "volume", "exists", volume], check=False).returncode,
                1,
            )


if __name__ == "__main__":
    unittest.main()
