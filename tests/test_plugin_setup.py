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

from test_plugin_origin import fixture, package, plugin_origin, replace_package


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

    def test_actual_load_requires_pinned_versions_hashes_factories_and_abi(self):
        plugins = []
        for reference, pin in self.pins.items():
            category, kind = reference.split(":")[0].split("/")
            plugins.append({
                "id": f"{category}/{kind}", "sdkVersion": "0.11.0",
                "pluginVersion": pin["version"], "fileHash": pin["file_hash"],
                "status": "Loaded", "kinds": [{"category": category.title(), "kind": kind}],
            })
        installer.validate_loaded_plugins(plugins, self.pins)
        for field, wrong in (
            ("sdkVersion", "0.10.0"), ("pluginVersion", "999.0.0"),
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

    def source_stubs(self):
        (self.root / "scripts").mkdir(exist_ok=True)
        shutil.copyfile(ROOT / "scripts/prepare-trading.sh", self.root / "scripts/prepare-trading.sh")
        (self.root / "scripts/prepare-core.sh").write_text(
            '#!/bin/bash\nprintf \'["prepare-core","%s"]\\n\' "$*" >> "$POLICY_LOG"\n'
            'if [[ "${FAIL_CORE:-0}" == 1 ]]; then exit 17; fi\n'
        )
        executable(self.bin / "python3", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["python3", *sys.argv[1:]]) + "\\n")
if sys.argv[1].endswith("plugin_origin.py"):
    print(os.environ["POLICY_MODE"])
""")
        executable(self.bin / "make", """
import json, os, sys
from pathlib import Path
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["make", *sys.argv[1:]]) + "\\n")
if "build-release" in sys.argv:
    if os.environ.get("FAIL_BUILD") == "1":
        sys.exit(19)
    output = Path(sys.argv[sys.argv.index("-C") + 1]) / "ui/dist"
    output.mkdir(parents=True, exist_ok=True)
    (output / "index.html").write_text("fixture UI source build")
""")
        executable(self.bin / "npm", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["npm", *sys.argv[1:]]) + "\\n")
if sys.argv[-2:] == ["run", "build"] and os.environ.get("FAIL_PACKAGE_BUILD") == "1":
    sys.exit(23)
""")

    def test_make_test_all_uses_resolved_origin_not_sibling_existence(self):
        checkout = self.root / "server"
        checkout.mkdir()
        (self.root / "drasi-core").mkdir()
        for mode in ("registry", "local"):
            with self.subTest(mode=mode):
                metadata = fixture()
                if mode == "local":
                    for name in (*plugin_origin.SDK_PACKAGES, "drasi-lib"):
                        replace_package(
                            metadata, package(name, "0.8.9" if name == "drasi-lib" else "0.10.0", None),
                        )
                _, selected = plugin_origin.classify(metadata)
                core = {
                    "workspace_root": "/fixture/core",
                    "workspace_members": [entry["id"] for entry in selected.values()],
                    "packages": list(selected.values()),
                }
                self.log.write_text("")
                environment = {
                    **self.environment, "SERVER_METADATA": json.dumps(metadata),
                    "CORE_METADATA": json.dumps(core),
                }
                executable(self.bin / "cargo", """
import json, os, sys
from pathlib import Path
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["cargo", *sys.argv[1:]]) + "\\n")
if "metadata" in sys.argv:
    print(os.environ["CORE_METADATA" if "--no-deps" in sys.argv else "SERVER_METADATA"])
""")
                (checkout / "scripts").mkdir(exist_ok=True)
                shutil.copyfile(ROOT / "scripts/plugin_origin.py", checkout / "scripts/plugin_origin.py")
                executable(checkout / "tests/plugin_smoke_test.sh", "print('stub smoke command')\n")
                makefile = checkout / "test.mk"
                makefile.write_text(
                    f"include {ROOT / 'Makefile'}\n"
                    "download-test-plugins:\n\t@echo '[\"registry-install\"]' >> \"$$POLICY_LOG\"\n"
                    "build-local-plugins-debug:\n\t@echo '[\"local-build\"]' >> \"$$POLICY_LOG\"\n"
                )
                result = subprocess.run(
                    ["make", "-f", str(makefile), "test-all", f"MAKE=make -f {makefile}"],
                    cwd=checkout, env=environment, text=True,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                events = self.commands()
                self.assertIn(["registry-install" if mode == "registry" else "local-build"], events)
                self.assertNotIn(["local-build" if mode == "registry" else "registry-install"], events)
                self.assertIn(["cargo", "test", "--locked", "--tests", "--", "--include-ignored"], events)
                self.assertIn(["cargo", "test", "--locked", "--doc"], events)
                if mode == "registry":
                    result = subprocess.run(
                        ["make", "-f", str(makefile), "build-local-plugins"],
                        cwd=checkout, env=environment, text=True,
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("consumes registry SDKs", result.stderr)

    def test_clean_trading_startup_keeps_registry_verification_and_local_development(self):
        for mode in ("registry", "local"):
            with self.subTest(mode=mode):
                self.log.write_text("")
                trading = self.root / "examples/trading"
                (trading / "database").mkdir(parents=True, exist_ok=True)
                (self.root / "ui/dist").mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / "examples/trading/start-demo.sh", trading / "start-demo.sh")
                environment = {**self.environment, "POLICY_MODE": mode}
                self.source_stubs()
                for tool in ("docker",):
                    executable(self.bin / tool, "pass\n")
                executable(self.bin / "docker-compose", "print('fixture-ready')\n")
                executable(self.bin / "curl", "print('200')\n")
                executable(self.bin / "sleep", "import time; time.sleep(0.1)\n")
                executable(self.root / "target/release/drasi-server", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["server", *sys.argv[1:]]) + "\\n")
sys.exit(42)
""")
                result = subprocess.run(
                    ["bash", str(trading / "start-demo.sh")], cwd=self.root, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
                )
                self.assertNotEqual(result.returncode, 0, "Fixture server deliberately refuses to run")
                events = self.commands()
                server_calls = [event for event in events if event[0] == "server"]
                self.assertEqual(len(server_calls), 1, result.stdout + result.stderr)
                installs = [event for event in events if any("install_plugins.py" in arg for arg in event)]
                builds = [event[-1] for event in events if event[0] == "make"]
                self.assertIn("build-release", builds)
                if mode == "registry":
                    self.assertEqual(len(installs), 1)
                    self.assertNotIn("build-local-plugins", builds)
                    self.assertNotIn("--skip-verification", server_calls[0])
                else:
                    self.assertEqual(installs, [])
                    self.assertIn("build-local-plugins", builds)
                    self.assertIn("--skip-verification", server_calls[0])
                self.assertIn("examples/trading/server/trading-sources-only.yaml", server_calls[0])
                self.assertFalse((self.root / "target/release/plugins").exists())

    def test_post_create_uses_source_build_and_shared_locked_plugin_setup(self):
        post_create = self.root / ".devcontainer/trading/post-create.sh"
        post_create.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / ".devcontainer/trading/post-create.sh", post_create)
        trading = self.root / "examples/trading"
        (trading / "app/node_modules").mkdir(parents=True)
        (trading / "start-demo.sh").write_text("#!/bin/bash\n")
        (trading / "stop-demo.sh").write_text("#!/bin/bash\n")
        executable(self.root / "target/release/drasi-server", "raise SystemExit('arbitrary prebuilt')\n")
        self.source_stubs()
        for tool in ("sudo", "docker", "curl"):
            executable(self.bin / tool, f"""
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps([{tool!r}, *sys.argv[1:]]) + "\\n")
""")
        executable(self.bin / "dpkg-architecture", "print('x86_64-linux-gnu')\n")
        environment = {**self.environment, "POLICY_MODE": "registry"}
        result = subprocess.run(
            ["bash", str(post_create)], cwd=self.root, env=environment, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        events = self.commands()
        self.assertTrue(any(event[0] == "prepare-core" for event in events))
        self.assertTrue(any(event[0] == "make" and event[-1] == "build-release" for event in events))
        self.assertFalse(any(event[0] == "curl" for event in events))
        self.assertFalse(any("--skip-verification" in event for event in events))
        self.assertEqual((self.root / "ui/dist/index.html").read_text(), "fixture UI source build")
        installs = [event for event in events if any("install_plugins.py" in arg for arg in event)]
        self.assertEqual(len(installs), 1)
        npm_calls = [event for event in events if event[0] == "npm"]
        self.assertEqual(
            [(Path(event[2]).name, event[3:]) for event in npm_calls],
            [("react", ["ci"]), ("react", ["run", "build"]), ("app", ["ci"])],
        )

        for failure in ("FAIL_CORE", "FAIL_BUILD"):
            with self.subTest(failure=failure):
                self.log.write_text("")
                result = subprocess.run(
                    ["bash", str(post_create)], cwd=self.root,
                    env={**environment, failure: "1"}, text=True,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(
                    "install_plugins.py" in argument
                    for event in self.commands() for argument in event
                ))

    def test_clean_package_dependency_build_precedes_app_install_and_failure_stops_it(self):
        self.source_stubs()
        environment = {**self.environment, "POLICY_MODE": "registry"}
        for failure in (False, True):
            with self.subTest(failure=failure):
                self.log.write_text("")
                result = subprocess.run(
                    ["bash", str(self.root / "scripts/prepare-trading.sh")],
                    cwd=self.root,
                    env={**environment, "FAIL_PACKAGE_BUILD": "1" if failure else "0"},
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                )
                npm_calls = [event for event in self.commands() if event[0] == "npm"]
                expected = [("react", ["ci"]), ("react", ["run", "build"])]
                if failure:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn("registry", result.stdout)
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), "registry")
                    expected.append(("app", ["ci"]))
                self.assertEqual(
                    [(Path(event[2]).name, event[3:]) for event in npm_calls],
                    expected,
                )

    def test_release_build_requires_real_ui_build_and_locked_cargo(self):
        (self.root / "ui").mkdir()
        for tool in ("npm", "cargo"):
            executable(self.bin / tool, f"""
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps([{tool!r}, *sys.argv[1:]]) + "\\n")
if {tool!r} == "npm" and sys.argv[1:] == ["run", "build"] and os.environ.get("FAIL_UI") == "1":
    sys.exit(21)
""")
        for fail in ("0", "1"):
            with self.subTest(fail=fail):
                self.log.write_text("")
                result = subprocess.run(
                    ["make", "-f", str(ROOT / "Makefile"), "build-release"],
                    cwd=self.root, env={**self.environment, "FAIL_UI": fail},
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                )
                events = self.commands()
                self.assertEqual(events[:2], [["npm", "ci"], ["npm", "run", "build"]])
                if fail == "0":
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(["cargo", "build", "--locked", "--release"], events)
                else:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(any(event[0] == "cargo" for event in events))

    def test_test_plugin_download_dispatches_to_the_same_locked_installer(self):
        executable(self.bin / "cargo", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["cargo", *sys.argv[1:]]) + "\\n")
""")
        executable(self.bin / "python3", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["python3", *sys.argv[1:]]) + "\\n")
""")
        result = subprocess.run(
            ["make", "-f", str(ROOT / "Makefile"), "download-test-plugins"],
            cwd=self.root, env=self.environment, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        events = self.commands()
        self.assertEqual(events[0], ["cargo", "build", "--locked"])
        self.assertEqual(events[1][:4], ["python3", "scripts/install_plugins.py", "--group", "test"])
        self.assertFalse(any("latest" in argument for event in events for argument in event))


if __name__ == "__main__":
    unittest.main()
