import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent


class DevcontainerNetworkTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.log = self.directory / "calls.jsonl"
        self.env = {
            **os.environ,
            "PATH": str(self.directory),
            "ENGINE_LOG": str(self.log),
        }
        self.env.pop("DRASI_CONTAINER_ENGINE", None)

    def engine(self, name):
        executable = self.directory / name
        executable.write_text(f"#!{sys.executable}\n" + """
import json, os, sys
from pathlib import Path
with open(os.environ["ENGINE_LOG"], "a") as output:
    output.write(json.dumps([Path(sys.argv[0]).name, *sys.argv[1:]]) + "\\n")
if sys.argv[1:] == ["info"]:
    raise SystemExit(int(os.environ.get("INFO_STATUS", "0")))
if sys.argv[1:3] == ["network", "inspect"]:
    raise SystemExit(int(os.environ.get("INSPECT_STATUS", "0")))
raise SystemExit(int(os.environ.get("CREATE_STATUS", "0")))
""")
        executable.chmod(0o755)

    def run_setup(self, **environment):
        return subprocess.run(
            ["/bin/sh", str(ROOT / ".devcontainer/initialize-network.sh")],
            env={**self.env, **environment}, cwd=ROOT,
            text=True, capture_output=True,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_podman_only_host_reuses_existing_network(self):
        self.engine("podman")
        result = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [
            ["podman", "info"], ["podman", "network", "inspect", "drasi-network"],
        ])

    def test_docker_remains_default_when_both_are_installed(self):
        self.engine("docker")
        self.engine("podman")
        result = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(all(call[0] == "docker" for call in self.calls()))

    def test_explicit_podman_creates_missing_network(self):
        self.engine("docker")
        self.engine("podman")
        result = self.run_setup(DRASI_CONTAINER_ENGINE="podman", INSPECT_STATUS="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [
            ["podman", "info"],
            ["podman", "network", "inspect", "drasi-network"],
            ["podman", "network", "create", "drasi-network"],
        ])

    def test_unavailable_engine_does_not_create_network_or_fall_back(self):
        self.engine("docker")
        self.engine("podman")
        result = self.run_setup(INFO_STATUS="125")
        self.assertEqual(result.returncode, 125)
        self.assertEqual(self.calls(), [["docker", "info"]])

    def test_create_failure_is_propagated(self):
        self.engine("podman")
        result = self.run_setup(INSPECT_STATUS="1", CREATE_STATUS="125")
        self.assertEqual(result.returncode, 125)

    def test_missing_or_invalid_engine_has_actionable_error(self):
        for environment, message in (
            ({}, "Docker or Podman is required"),
            ({"DRASI_CONTAINER_ENGINE": "docker"}, "was not found on PATH"),
            ({"DRASI_CONTAINER_ENGINE": "unknown"}, "must be docker or podman"),
        ):
            with self.subTest(environment=environment):
                result = self.run_setup(**environment)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertFalse(self.log.exists())

    def test_getting_started_uses_shared_initializer(self):
        config = json.loads(
            (ROOT / ".devcontainer/getting-started/devcontainer.json").read_text()
        )
        self.assertEqual(config["initializeCommand"], [
            "sh", ".devcontainer/initialize-network.sh",
        ])


if __name__ == "__main__":
    unittest.main()
