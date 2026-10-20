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
NATIVE_TRADING_CRATES = {
    "drasi-computation-source-http", "drasi-computation-source-postgres",
    "drasi-computation-bootstrap-http", "drasi-computation-bootstrap-postgres",
    "drasi-reaction-sse",
}
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
            ("sdk_version", "0.11.1"), ("sdk_version", "0.11.2"),
            ("lib_version", "0.9.1"), ("lib_version", "0.9.2"),
            ("core_version", "0.5.8"), ("core_version", "0.5.9"),
            ("platform", "linux/arm64"),
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
                "id": f"{category}/{kind}", "sdkVersion": "0.14.0",
                "pluginVersion": pin["version"], "fileHash": pin["file_hash"],
                "status": "Loaded", "kinds": [{"category": category.title(), "kind": kind}],
            })
        installer.validate_loaded_plugins(plugins, self.pins)
        for field, wrong in (
            ("sdkVersion", "0.11.0"), ("sdkVersion", "0.13.0"),
            ("sdkVersion", "0.15.0"),
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

    def prepare_stub(self, checkout=None):
        checkout = checkout or self.root
        (checkout / "scripts").mkdir(exist_ok=True)
        (checkout / "scripts/prepare-build.sh").write_text(
            '#!/bin/bash\necho \'["prepare-build"]\' >> "$POLICY_LOG"\n'
            'if [[ "${FAIL_PREPARE:-0}" == 1 ]]; then exit 17; fi\n'
        )

    def source_stubs(self):
        (self.root / "scripts").mkdir(exist_ok=True)
        shutil.copyfile(ROOT / "scripts/prepare-trading.sh", self.root / "scripts/prepare-trading.sh")
        shutil.copyfile(ROOT / "scripts/prepare-build.sh", self.root / "scripts/prepare-build.sh")
        executable(self.bin / "python3", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["python3", *sys.argv[1:]]) + "\\n")
if sys.argv[1].endswith("plugin_origin.py"):
    if os.environ.get("FAIL_ORIGIN") == "1":
        sys.exit(17)
    print(os.environ["SELECTED_SDK"] if sys.argv[-1] == "local-workspace" else os.environ["POLICY_MODE"])
elif sys.argv[1] == "-c" and sys.argv[-1] == "import requests, flask, flask_cors, psycopg2":
    pass
elif sys.argv[1].endswith("trading_api.py"):
    import time
    time.sleep(30)
elif sys.argv[1] in ("-", "-c"):
    os.execv(sys.executable, [sys.executable, *sys.argv[1:]])
""")
        executable(self.bin / "make", """
import json, os, sys
from pathlib import Path
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["make", *sys.argv[1:]]) + "\\n")
if "build-release" in sys.argv:
    if os.environ.get("FAIL_BUILD") == "1":
        sys.exit(19)
    root = Path(sys.argv[sys.argv.index("-C") + 1]) if "-C" in sys.argv else Path.cwd()
    output = root / "ui/dist"
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
        # The registry subtest needs the reviewed release lock, independent of this checkout's origin.
        (checkout / "Cargo.lock").write_text("version = 4\n" + "".join(
            f'\n[[package]]\nname = "{name}"\nversion = "{version}"\n'
            f'source = "{plugin_origin.REGISTRY}"\nchecksum = "{checksum}"\n'
            for name, (version, checksum) in plugin_origin.REGISTRY_PACKAGES.items()
        ))
        (self.root / "drasi-core").mkdir()
        for mode in ("registry", "local"):
            with self.subTest(mode=mode):
                metadata = fixture()
                if mode == "local":
                    for name in (*plugin_origin.SDK_PACKAGES, "drasi-lib", "drasi-core"):
                        replace_package(
                            metadata, package(name, plugin_origin.REGISTRY_PACKAGES[name][0], None),
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
                self.prepare_stub(checkout)
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

    def test_native_trading_start_requires_local_origin_and_never_registry_fallback(self):
        for mode, arguments, message in (
            ("registry", [], "Requested local plugins, but Cargo resolves registry"),
            ("local", ["--plugin-source", "registry"], "registry fallback is not supported"),
            ("local", [], "Local trading plugins have not been prepared"),
        ):
            with self.subTest(mode=mode, arguments=arguments):
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
                    ["bash", str(trading / "start-demo.sh"), *arguments], cwd=self.root, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
                )
                self.assertNotEqual(result.returncode, 0, "Fixture server deliberately refuses to run")
                events = self.commands()
                server_calls = [event for event in events if event[0] == "server"]
                installs = [event for event in events if any("install_plugins.py" in arg for arg in event)]
                builds = [event[-1] for event in events if event[0] == "make"]
                self.assertIn(message, result.stdout + result.stderr)
                self.assertEqual(server_calls, [])
                self.assertEqual(installs, [])
                self.assertEqual(builds, [])
                self.assertFalse((self.root / "target/release/plugins").exists())

    def local_trading_fixture(self):
        self.source_stubs()
        trading = self.root / "examples/trading"
        (trading / "server").mkdir(parents=True)
        (trading / "database").mkdir()
        (trading / "mock-generator").mkdir()
        (trading / "mock-generator/trading_api.py").write_text("# fixture process\n")
        (trading / "app/scripts").mkdir(parents=True)
        shutil.copyfile(
            ROOT / "examples/trading/app/scripts/export-native-recipe.ts",
            trading / "app/scripts/export-native-recipe.ts",
        )
        for name in ("start-demo.sh", "build-local-plugins.sh"):
            shutil.copyfile(ROOT / "examples/trading" / name, trading / name)
        shutil.copyfile(
            ROOT / "examples/trading/server/trading-sources-only.yaml",
            trading / "server/trading-sources-only.yaml",
        )
        shutil.copyfile(
            ROOT / "examples/trading/server/trading-native-recipe.json",
            trading / "server/trading-native-recipe.json",
        )
        shutil.copyfile(ROOT / "scripts/plugin_origin.py", self.root / "scripts/plugin_origin.py")
        core = self.root.parent / "selected-core"
        core.mkdir()
        metadata = fixture()
        for name in (*plugin_origin.SDK_PACKAGES, "drasi-lib", "drasi-core"):
            replace_package(
                metadata, package(name, plugin_origin.REGISTRY_PACKAGES[name][0], None, str(core)),
            )
        selected = plugin_origin.selected_packages(metadata)
        core_metadata = {
            "workspace_root": str(core), "target_directory": str(core / "target"),
            "workspace_members": [entry["id"] for entry in selected.values()],
            "packages": list(selected.values()),
        }
        self.environment.update({
            "POLICY_MODE": "local", "SELECTED_SDK": str(core),
            "SERVER_METADATA": json.dumps(metadata), "CORE_METADATA": json.dumps(core_metadata),
        })
        executable(self.bin / "cargo", """
import json, os, sys
from pathlib import Path
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["cargo", *sys.argv[1:]]) + "\\n")
if "metadata" in sys.argv:
    print(os.environ["CORE_METADATA" if "--no-deps" in sys.argv else "SERVER_METADATA"])
elif "build" in sys.argv:
    if "--example" in sys.argv:
        assert sys.argv[sys.argv.index("--example") + 1] == "native-postgres"
        output = Path(os.environ["CARGO_TARGET_DIR"]) / "release/examples/native-postgres"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text("#!" + sys.executable + "\\nimport json,sys\\nassert sys.argv[1] == 'settings'\\nprint(json.dumps({}))\\n")
        output.chmod(0o755)
        sys.exit(0)
    name = sys.argv[sys.argv.index("-p") + 1].replace("-", "_")
    prefix, extension = ("", "dll") if sys.platform == "win32" else ("lib", "dylib" if sys.platform == "darwin" else "so")
    output = Path(os.environ["SELECTED_SDK"]) / "target/release"
    output.mkdir(parents=True, exist_ok=True)
    (output / f"{prefix}{name}.{extension}").write_bytes(b"fixture-only plugin")
""")
        executable(self.bin / "node", f"""
import json, os, sys
from pathlib import Path
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["node", *sys.argv[1:]]) + "\\n")
assert sys.argv[1:4] == ["--import", "tsx", "scripts/export-native-recipe.ts"]
assert Path(sys.argv[3]).is_file()
assert Path(sys.argv[4]).is_file()
prefix, suffix = ("", ".dll") if sys.platform == "win32" else ("lib", ".dylib" if sys.platform == "darwin" else ".so")
expected = {{prefix + name.replace("-", "_") + suffix for name in {sorted(NATIVE_TRADING_CRATES)!r}}}
assert {{path.name for path in Path(sys.argv[5]).iterdir()}} == expected
if os.environ.get("FAIL_GENERATION") == "1":
    raise SystemExit("native metadata generation failed")
output = Path("public/native-computation.json")
output.parent.mkdir(exist_ok=True)
output.write_text(json.dumps({{"fixture": "generated from the complete native library set"}}))
""")
        executable(self.root / "target/release/drasi-server", """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["server", *sys.argv[1:]]) + "\\n")
if "--version" in sys.argv:
    print("fixture server\\nplugin-sdk: 0.11.3\\nplugin ABI: 0.14.0")
elif "validate" in sys.argv:
    if os.environ.get("FAIL_VALIDATE") == "1":
        print("[ERR] plugin ABI mismatch")
        sys.exit(1)
    print("Plugins (5 loaded)")
else:
    sys.exit(42)
""")
        executable(self.bin / "docker-compose", """
import json,os,sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["compose", *sys.argv[1:]]) + "\\n")
if "psql" in sys.argv:
    missing = os.environ.get("MISSING_SLOT") == "1" and "drasi_broker_slot" in sys.argv[-1]
    print("0" if missing else "1")
""")
        for tool, body in (
            ("docker", "pass\n"),
            ("curl", "import sys\nprint('503' if sys.argv[-1] == 'http://localhost:8280/health' else '200')\n"),
            ("sleep", "import time; time.sleep(0.1)\n"),
        ):
            executable(self.bin / tool, body)
        for repository in (self.root, core):
            (repository / ".gitignore").write_text(
                "target/\nplugins/\nlogs/\nbin/\ncommands.jsonl\nui/dist/\n__pycache__/\n"
            )
            subprocess.run(["git", "init", "-q", str(repository)], check=True)
            subprocess.run(["git", "-C", str(repository), "add", "."], check=True)
            subprocess.run([
                "git", "-C", str(repository), "-c", "user.name=Fixture",
                "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                "commit", "-qm", "fixture",
            ], check=True)
        return trading, core

    def build_local_fixture(self, trading):
        result = subprocess.run(
            ["bash", str(trading / "build-local-plugins.sh")],
            cwd=self.root, env=self.environment, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return json.loads((trading / "plugins/local/local-build.json").read_text())

    def start_local_fixture(self, trading, **environment):
        self.log.write_text("")
        return subprocess.run(
            ["bash", str(trading / "start-demo.sh"), "--plugin-source", "local"],
            cwd=self.root, env={**self.environment, **environment}, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )

    def test_explicit_local_build_and_start_use_only_five_matched_plugins(self):
        trading, _ = self.local_trading_fixture()
        manifest = self.build_local_fixture(trading)
        self.assertEqual(len(manifest["plugins"]), 5)
        self.assertIn("drasi-ffi-primitives", manifest["resolved_versions"])
        self.assertEqual(manifest["ffi_abi"], "0.14.0")
        builds = [event for event in self.commands() if event[:2] == ["cargo", "build"]]
        self.assertEqual(len(builds), 6)
        library_builds = [event for event in builds if "-p" in event]
        self.assertEqual(
            {event[event.index("-p") + 1] for event in library_builds},
            NATIVE_TRADING_CRATES,
        )
        self.assertTrue(all("--locked" in event for event in builds))
        self.assertEqual(len([event for event in builds if "--example" in event]), 1)
        self.assertTrue((trading / "app/public/native-computation.json").is_file())
        self.assertEqual(len([event for event in self.commands() if event[0] == "node"]), 1)
        result = self.start_local_fixture(trading)
        self.assertNotEqual(result.returncode, 0, "Fixture server deliberately refuses to run")
        events = self.commands()
        launches = [event for event in events if event[0] == "server" and "validate" not in event]
        self.assertEqual(len(launches), 1, result.stdout + result.stderr)
        self.assertIn("--skip-verification", launches[0])
        self.assertIn(str(trading / "plugins/local"), launches[0])
        self.assertFalse(any(event[0] in ("cargo", "make") for event in events))
        self.assertFalse(any("install_plugins.py" in arg for event in events for arg in event))
        config = (trading / "server/trading-sources-only.yaml").read_text()
        self.assertIn("autoInstallPlugins: false", config)
        self.assertIn("sources: []", config)
        self.assertIn("plugins: []", config)
        self.assertFalse((trading / "logs/trading-sources-local.yaml").exists())
        self.assertFalse(any("pg_drop_replication_slot" in arg for event in events for arg in event))

    def test_local_start_rejects_stale_or_incomplete_artifacts_without_rebuilding(self):
        trading, core = self.local_trading_fixture()
        manifest = self.build_local_fixture(trading)
        manifest_path = trading / "plugins/local/local-build.json"
        cases = [
            (self.root / "target/release/drasi-server", b"\n# changed"),
            (trading / "plugins/local" / next(iter(manifest["plugins"])), b"changed"),
            (core / ".gitignore", b"\n# changed"),
            (self.root / ".gitignore", b"\n# changed"),
        ]
        for path, addition in cases:
            with self.subTest(path=path):
                original = path.read_bytes()
                try:
                    path.write_bytes(original + addition)
                    result = self.start_local_fixture(trading)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("stale or incomplete", result.stderr)
                    self.assertFalse(any(event[0] in ("server", "make", "cargo") for event in self.commands()))
                finally:
                    path.write_bytes(original)
        manifest["plugins"].pop(next(iter(manifest["plugins"])))
        manifest_path.write_text(json.dumps(manifest))
        result = self.start_local_fixture(trading)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exactly the five", result.stderr)

    def test_local_build_validation_failure_preserves_installed_set(self):
        trading, _ = self.local_trading_fixture()
        manifest = self.build_local_fixture(trading)
        self.environment["FAIL_VALIDATE"] = "1"
        result = subprocess.run(
            ["bash", str(trading / "build-local-plugins.sh")],
            cwd=self.root, env=self.environment, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rejected the local plugin set", result.stderr)
        self.assertEqual(json.loads((trading / "plugins/local/local-build.json").read_text()), manifest)
        self.assertEqual([path.name for path in (trading / "plugins").iterdir()], ["local"])

    def test_native_generation_failure_preserves_the_installed_library_set(self):
        trading, _ = self.local_trading_fixture()
        manifest = self.build_local_fixture(trading)
        self.environment["FAIL_GENERATION"] = "1"
        result = subprocess.run(
            ["bash", str(trading / "build-local-plugins.sh")],
            cwd=self.root, env=self.environment, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("native metadata generation failed", result.stderr)
        self.assertEqual(json.loads((trading / "plugins/local/local-build.json").read_text()), manifest)
        self.assertEqual([path.name for path in (trading / "plugins").iterdir()], ["local"])

    def test_native_start_refuses_missing_configured_slots_without_repair(self):
        trading, _ = self.local_trading_fixture()
        self.build_local_fixture(trading)
        result = self.start_local_fixture(trading, MISSING_SLOT="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Configured slot drasi_broker_slot is missing", result.stderr)
        self.assertFalse(any(event[0] == "server" and "validate" not in event for event in self.commands()))
        sql = [event[-1] for event in self.commands() if event[0] == "compose" and "psql" in event]
        self.assertTrue(sql)
        self.assertTrue(all(statement.startswith("SELECT count(*)") for statement in sql))

    def test_explicit_local_request_rejects_registry_graph_before_build_or_install(self):
        self.source_stubs()
        result = subprocess.run(
            ["bash", str(self.root / "scripts/prepare-trading.sh"), "--plugin-source", "local"],
            cwd=self.root, env={**self.environment, "POLICY_MODE": "registry"}, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Requested local plugins, but Cargo resolves registry", result.stderr)
        self.assertFalse(any(event[0] in ("make", "npm") for event in self.commands()))
        self.assertFalse(any("install_plugins.py" in arg for event in self.commands() for arg in event))

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
        self.assertTrue(any("plugin_origin.py" in event[1] for event in events if event[0] == "python3"))
        self.assertFalse((self.root.parent / "drasi-core").exists())
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

        for failure in ("FAIL_ORIGIN", "FAIL_BUILD"):
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

    def test_getting_started_proof_uses_only_selected_artifacts_and_isolated_cases(self):
        scripts = self.root / "examples/getting-started/scripts"
        scripts.mkdir(parents=True)
        shutil.copyfile(ROOT / "examples/getting-started/scripts/run-end-to-end.sh",
                        scripts / "run-end-to-end.sh")
        selected = self.root / "chosen-target"
        unrelated = self.root.parent / "drasi-core/target/release/plugins"
        unrelated.mkdir(parents=True)
        executable(self.bin / "python3", """
import json,os,sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["python3", *sys.argv[1:]]) + "\\n")
assert sys.argv[1] == "examples/postgres/qualify.py"
""")
        for explicit in (False, True):
            with self.subTest(explicit=explicit):
                self.log.write_text("")
                environment = {
                    **self.environment, "CARGO_TARGET_DIR": str(selected),
                    "EVIDENCE_DIR": str(self.root / "new-evidence"),
                }
                overrides = {
                    "SERVER_BIN": str(selected / "custom-server"),
                    "GENERATOR_BIN": str(selected / "custom-generator"),
                    "LOCAL_PLUGINS_DIR": str(selected / "native-libraries"),
                }
                if explicit:
                    environment.update(overrides)
                result = subprocess.run(
                    ["bash", str(scripts / "run-end-to-end.sh")],
                    cwd=self.root, env=environment, stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(len(self.commands()), 1)
                command = self.commands()[0]
                for flag, expected in (
                    ("--server", overrides["SERVER_BIN"] if explicit else str(selected / "debug/drasi-server")),
                    ("--generator", overrides["GENERATOR_BIN"] if explicit else str(selected / "debug/examples/native-postgres")),
                    ("--plugins", overrides["LOCAL_PLUGINS_DIR"] if explicit else str(selected / "debug")),
                ):
                    self.assertEqual(command[command.index(flag) + 1], expected)
                self.assertEqual([command[index + 1] for index, value in enumerate(command) if value == "--case"],
                    ["getting-started-server", *[f"getting-started-step-{n}" for n in range(3, 7)]])
                self.assertNotIn(str(unrelated), command)
                self.assertFalse(any("cleanup.sh" in value or "install_plugins.py" in value for value in command))

    def test_getting_started_setup_preserves_complete_and_partial_installations(self):
        scripts = self.root / "examples/getting-started/scripts"
        scripts.mkdir(parents=True)
        (scripts.parent / "database").mkdir()
        shutil.copyfile(ROOT / "examples/getting-started/scripts/setup-database.sh",
                        scripts / "setup-database.sh")
        executable(self.bin / "podman", """
import json,os,sys
args=sys.argv[1:]
data=sys.stdin.read() if "-i" in args else ""
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps(["podman", *args, data]) + "\\n")
if "psql" in args and "-c" in args:
    query=args[args.index("-c") + 1]
    if "to_regclass" in query: print(os.environ["SETUP_STATE"])
    elif "rolreplication" in query: print(os.environ.get("SETUP_VALID", "t"))
""")
        for state, valid, success in [
            ("0,0,0", "t", True), ("1,1,1", "t", True),
            ("1,0,1", "t", False), ("1,1,1", "f", False),
        ]:
            with self.subTest(state=state, valid=valid):
                self.log.write_text("")
                result = subprocess.run(["bash", str(scripts / "setup-database.sh")],
                    cwd=self.root, env={**self.environment, "CONTAINER_CLI": str(self.bin / "podman"),
                                       "SETUP_STATE": state, "SETUP_VALID": valid},
                    input="", text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
                self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
                statements = "\n".join(argument for event in self.commands() for argument in event)
                self.assertNotIn("pg_drop_replication_slot", statements)
                self.assertNotIn("DROP TABLE", statements)
                self.assertNotIn("down", [argument for event in self.commands() for argument in event])
                if state == "0,0,0":
                    self.assertIn("CREATE TABLE message", statements)
                    self.assertIn("pg_create_logical_replication_slot", statements)
                else:
                    self.assertNotIn("CREATE TABLE", statements)
                    self.assertNotIn("pg_create_logical_replication_slot", statements)
                if state == "1,0,1":
                    self.assertIn("Incomplete tutorial installation", result.stderr)

    def test_playground_provides_python_with_tomllib_for_build_preflight(self):
        config = json.loads((ROOT / ".devcontainer/playground/devcontainer.json").read_text())
        python = config["features"]["ghcr.io/devcontainers/features/python:1"]["version"]
        self.assertGreaterEqual(tuple(map(int, python.split("."))), (3, 11))

    def test_release_build_entry_points_require_ui_before_locked_cargo(self):
        (self.root / "ui").mkdir()
        self.prepare_stub()
        shutil.copyfile(ROOT / "Makefile", self.root / "Makefile")
        executable(self.bin / "sudo", "pass\n")
        executable(self.bin / "dpkg-architecture", "print('x86_64-linux-gnu')\n")
        # Exercise the post-create build without installing packages or starting services.
        playground_build = (
            ROOT / ".devcontainer/playground/post-create.sh"
        ).read_text().split("# Make scripts executable", 1)[0]
        for tool in ("npm", "cargo"):
            executable(self.bin / tool, f"""
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps([{tool!r}, *sys.argv[1:]]) + "\\n")
if {tool!r} == "npm" and sys.argv[1:] == ["run", "build"] and os.environ.get("FAIL_UI") == "1":
    sys.exit(21)
""")
        for command in (
            ["make", "build-release"],
            ["bash", "-c", playground_build],
        ):
            for fail in ("0", "1"):
                with self.subTest(entry_point=command[0], fail=fail):
                    self.log.write_text("")
                    result = subprocess.run(
                        command, cwd=self.root, env={**self.environment, "FAIL_UI": fail},
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                    )
                    events = self.commands()
                    self.assertEqual(events[:3], [
                        ["prepare-build"], ["npm", "ci"], ["npm", "run", "build"],
                    ])
                    if fail == "0":
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertEqual(events[3:], [["cargo", "build", "--locked", "--release"]])
                    else:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertFalse(any(event[0] == "cargo" for event in events))

    def test_test_plugin_download_dispatches_to_the_same_locked_installer(self):
        self.prepare_stub()
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
        self.assertEqual(events[0], ["prepare-build"])
        self.assertEqual(events[1], ["cargo", "build", "--locked"])
        self.assertEqual(events[2][:4], ["python3", "scripts/install_plugins.py", "--group", "test"])
        self.assertFalse(any("latest" in argument for event in events for argument in event))

    def test_public_cargo_targets_check_origin_and_fail_before_cargo(self):
        self.prepare_stub()
        (self.root / "ui").mkdir()
        (self.root / "config").mkdir()
        (self.root / "config/server.yaml").write_text("apiVersion: drasi.io/v1\n")
        for tool in ("cargo", "npm", "python3"):
            executable(self.bin / tool, f"""
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as log:
    log.write(json.dumps([{tool!r}, *sys.argv[1:]]) + "\\n")
""")
        makefile = self.root / "targets.mk"
        makefile.write_text(f"include {ROOT / 'Makefile'}\ndoctor:\n\t@:\n")
        targets = ("setup", "build", "build-release", "run", "run-config", "run-release",
                   "test", "clippy", "fmt", "fmt-check", "dev-run", "validate",
                   "download-test-plugins")
        for target in targets:
            for failure in ("0", "1"):
                with self.subTest(target=target, preparation_failure=failure):
                    self.log.write_text("")
                    result = subprocess.run(
                        ["make", "-f", str(makefile), target, "CONFIG=config/server.yaml"],
                        cwd=self.root, env={**self.environment, "FAIL_PREPARE": failure},
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                    )
                    calls = self.commands()
                    self.assertEqual(calls[0], ["prepare-build"])
                    if failure == "1":
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(calls, [["prepare-build"]])
                    else:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertTrue(any(call[0] == "cargo" for call in calls))

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
