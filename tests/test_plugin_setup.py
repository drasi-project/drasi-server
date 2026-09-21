# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

from test_plugin_origin import plugin_origin


ROOT = Path(__file__).resolve().parent.parent
sys.modules["plugin_origin"] = plugin_origin
SPEC = importlib.util.spec_from_file_location(
    "install_plugins", ROOT / "scripts/install_plugins.py",
)
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


def executable(path, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"#!{sys.executable}\n" + body)
    path.chmod(0o755)


class LockedTradingPluginsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name) / "plugins"
        path, target = installer.pinned_lock_path("Linux", "x86_64")
        self.pins = installer.read_pins(path, target)
        self.payload = b"fixture-only plugin binary"
        for pin in self.pins.values():
            pin["file_hash"] = hashlib.sha256(self.payload).hexdigest()

    def fake_install(self, command, *, check):
        self.assertTrue(check)
        self.assertEqual(command[-4:], ["plugin", "install", "--from-config", "--locked"])
        self.assertNotIn("--skip-verification", command)
        config = json.loads(Path(command[command.index("--config") + 1]).read_text())
        self.assertTrue(config["verifyPlugins"])
        self.assertFalse(config["autoInstallPlugins"])
        self.assertEqual(
            {entry["ref"] for entry in config["plugins"]}, set(self.pins),
        )
        self.assertIn("reaction/sse", self.pins)
        for pin in self.pins.values():
            (self.directory / pin["filename"]).write_bytes(self.payload)

    def test_clean_install_includes_all_five_kinds_and_keeps_exact_lock_entries(self):
        installer.install(Path("/fixture/server"), self.directory, self.pins, self.fake_install)
        actual = tomllib.loads((self.directory / "plugins.lock").read_text())
        self.assertEqual(actual, {"version": 1, "plugins": self.pins})

    def test_success_shaped_cli_failure_cannot_pass_missing_plugin_postcondition(self):
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "Missing"):
            installer.install(
                Path("/fixture/server"), self.directory, self.pins,
                lambda command, **kwargs: subprocess.CompletedProcess(command, 0),
            )

    def test_installer_error_propagates(self):
        def fail(command, **kwargs):
            raise subprocess.CalledProcessError(9, command)
        with self.assertRaises(subprocess.CalledProcessError):
            installer.install(Path("/fixture/server"), self.directory, self.pins, fail)

    def test_existing_mismatched_binary_is_not_overwritten(self):
        self.directory.mkdir()
        pin = next(iter(self.pins.values()))
        binary = self.directory / pin["filename"]
        binary.write_bytes(b"unrelated local SDK binary")
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "hash mismatch"):
            installer.install(Path("/fixture/server"), self.directory, self.pins, self.fake_install)
        self.assertEqual(binary.read_bytes(), b"unrelated local SDK binary")

    def test_unrelated_lock_entries_and_comments_are_preserved(self):
        self.directory.mkdir()
        other = copy.deepcopy(next(iter(self.pins.values())))
        other["filename"] = "libdrasi_other.so"
        original = "# Existing user lock\nversion = 1\n" + installer.pin_toml("source/other", other)
        (self.directory / "plugins.lock").write_text(original)
        installer.install(Path("/fixture/server"), self.directory, self.pins, self.fake_install)
        updated = (self.directory / "plugins.lock").read_text()
        self.assertTrue(updated.startswith(original))
        self.assertEqual(tomllib.loads(updated)["plugins"]["source/other"], other)

    def test_conflicting_pin_is_not_replaced(self):
        installer.prepare_lock(self.directory, self.pins)
        lock_path = self.directory / "plugins.lock"
        original = lock_path.read_text()
        changed = copy.deepcopy(self.pins)
        next(iter(changed.values()))["digest"] = "sha256:" + "0" * 64
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "conflicts"):
            installer.prepare_lock(self.directory, changed)
        self.assertEqual(lock_path.read_text(), original)

    def test_symlink_lock_and_binary_are_rejected(self):
        self.directory.mkdir()
        outside = Path(self.temporary.name) / "outside"
        outside.write_bytes(self.payload)
        pin = next(iter(self.pins.values()))
        binary = self.directory / pin["filename"]
        binary.symlink_to(outside)
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "non-regular"):
            installer.prepare_lock(self.directory, self.pins)
        binary.unlink()
        (self.directory / "plugins.lock").symlink_to(outside)
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "symlink"):
            installer.prepare_lock(self.directory, self.pins)
        self.assertEqual(outside.read_bytes(), self.payload)

    def test_pins_cover_only_the_reviewed_platforms_and_versions(self):
        for system, machine in (("Linux", "x86_64"), ("Linux", "aarch64"), ("Darwin", "arm64")):
            with self.subTest(system=system, machine=machine):
                path, target = installer.pinned_lock_path(system, machine)
                pins = installer.read_pins(path, target)
                self.assertEqual({key.split(":")[0] for key in pins}, set(installer.VERSIONS))
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "No reviewed"):
            installer.pinned_lock_path("Darwin", "x86_64")

    def test_unsigned_or_missing_sse_pin_is_rejected(self):
        path, target = installer.pinned_lock_path("Linux", "x86_64")
        bad_lock = Path(self.temporary.name) / "bad.lock"
        bad_lock.write_text(path.read_text().replace("verified = true", "verified = false", 1))
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "invalid"):
            installer.read_pins(bad_lock, target)
        bad_lock.write_text(
            "version = 1\n" + "".join(
                installer.pin_toml(ref, pin) for ref, pin in self.pins.items()
                if ref != "reaction/sse"
            )
        )
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "5 reviewed"):
            installer.read_pins(bad_lock, target)

    def test_auxiliary_test_pins_reuse_the_original_scriptfile_entry(self):
        for system, machine in (("Linux", "x86_64"), ("Linux", "aarch64"), ("Darwin", "arm64")):
            with self.subTest(system=system, machine=machine):
                trading = installer.group_pins("trading", system, machine)
                pins = installer.group_pins("test", system, machine)
                self.assertEqual(
                    {key.split(":")[0] for key in pins},
                    set(installer.TEST_VERSIONS) | {"bootstrap/scriptfile"},
                )
                scriptfile = next(key for key in trading if key.split(":")[0] == "bootstrap/scriptfile")
                self.assertEqual(pins[scriptfile], trading[scriptfile])
                self.assertEqual(trading, installer.group_pins("trading", system, machine))

    def test_getting_started_reuses_the_signed_runtime_pins(self):
        for system, machine in (("Linux", "x86_64"), ("Linux", "aarch64"), ("Darwin", "arm64")):
            with self.subTest(system=system, machine=machine):
                expected = {
                    **installer.group_pins("trading", system, machine),
                    **installer.group_pins("test", system, machine),
                }
                pins = installer.group_pins("getting-started", system, machine)
                self.assertEqual(
                    {key.split(":")[0] for key in pins},
                    {"source/postgres", "bootstrap/postgres", "reaction/log"},
                )
                for reference, pin in pins.items():
                    kind = reference.split(":")[0]
                    expected_pin = next(
                        value for key, value in expected.items() if key.split(":")[0] == kind
                    )
                    self.assertEqual(pin, expected_pin)
                    self.assertEqual(reference, f"{kind}:{pin['version']}")
                    self.assertEqual(pin["signature"]["subject"], installer.SUBJECT)

    def test_current_pins_reject_wrong_sdk_crate_target_and_publisher(self):
        path, target = installer.pinned_lock_path("Linux", "x86_64")
        original = tomllib.loads(path.read_text())["plugins"]
        for field, value in (
            ("sdk_version", "0.10.0"), ("sdk_version", "0.11.0"),
            ("lib_version", "0.9.0"), ("platform", "linux/arm64"),
            ("signature", {
                "verified": True, "issuer": installer.ISSUER,
                "subject": installer.SUBJECT.replace("@refs/heads/main", "@refs/heads/experimental"),
            }),
        ):
            with self.subTest(field=field, value=value):
                pins = copy.deepcopy(original)
                next(iter(pins.values()))[field] = value
                candidate = Path(self.temporary.name) / "invalid.lock"
                candidate.write_text(
                    "version = 1\n" + "".join(installer.pin_toml(ref, pin) for ref, pin in pins.items())
                )
                with self.assertRaisesRegex(plugin_origin.PluginOriginError, "invalid"):
                    installer.read_pins(candidate, target)

    def test_actual_load_requires_pinned_versions_hashes_factories_and_abi(self):
        plugins = []
        for reference, pin in self.pins.items():
            category, kind = reference.split(":")[0].split("/")
            plugins.append({
                "id": f"{category}/{kind}", "sdkVersion": "0.13.0",
                "pluginVersion": pin["version"], "fileHash": pin["file_hash"],
                "status": "Loaded", "kinds": [{"category": category.title(), "kind": kind}],
            })
        installer.validate_loaded_plugins(plugins, self.pins)
        for field, wrong in (
            ("sdkVersion", "0.11.0"), ("sdkVersion", "0.14.0"),
            ("pluginVersion", "999.0.0"),
            ("fileHash", "0" * 64), ("status", "Failed"), ("kinds", []),
        ):
            with self.subTest(field=field):
                changed = copy.deepcopy(plugins)
                changed[0][field] = wrong
                with self.assertRaisesRegex(plugin_origin.PluginOriginError, "Incompatible"):
                    installer.validate_loaded_plugins(changed, self.pins)
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "not loaded"):
            installer.validate_loaded_plugins(plugins[:-1], self.pins)
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "Duplicate"):
            installer.validate_loaded_plugins(plugins + [plugins[0]], self.pins)


class PluginEntryPointTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "workspace"
        self.root.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "commands.jsonl"
        self.environment = {
            **os.environ, "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
            "POLICY_LOG": str(self.log),
        }

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_getting_started_installs_compatible_pins_before_launch_and_stops_on_failure(self):
        directory = self.root / "tests/integration/getting-started"
        directory.mkdir(parents=True)
        script = directory / "run-integration-test.sh"
        shutil.copyfile(ROOT / "tests/integration/getting-started/run-integration-test.sh", script)
        config = directory / "config.yaml"
        config.write_text("apiVersion: drasi.io/v1\n")
        server = self.bin / "test-server"
        executable(server, """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["server", *sys.argv[1:]]) + "\\n")
sys.exit(42)
""")
        executable(self.bin / "python3", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["python3", *sys.argv[1:]]) + "\\n")
sys.exit(int(os.environ.get("FAIL_INSTALL", "0")))
""")
        executable(self.bin / "curl", "raise SystemExit(1)\n")
        executable(self.bin / "sleep", "import time; time.sleep(0.1)\n")
        plugins = self.root / "isolated-getting-started-plugins"
        for failure in ("0", "1"):
            with self.subTest(failure=failure):
                self.log.write_text("")
                result = subprocess.run(
                    ["bash", str(script)], cwd=self.root,
                    env={
                        **self.environment, "SERVER_BINARY": str(server),
                        "SERVER_LOG": str(self.root / "server.log"),
                        "PLUGINS_DIR": str(plugins), "FAIL_INSTALL": failure,
                    },
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=15,
                )
                self.assertNotEqual(result.returncode, 0)
                calls = self.commands()
                self.assertEqual(calls[0][0], "python3")
                self.assertTrue(calls[0][1].endswith("scripts/install_plugins.py"))
                self.assertEqual(calls[0][2:4], ["--group", "getting-started"])
                launches = [call for call in calls if call[0] == "server"]
                if failure == "1":
                    self.assertEqual(launches, [])
                else:
                    self.assertEqual(launches, [[
                        "server", "--config", str(config), "--plugins-dir", str(plugins),
                    ]])


if __name__ == "__main__":
    unittest.main()
