# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "observability_bundle", ROOT / "examples/observability/local_bundle.py",
)
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)
SCENARIO = "http"
PG_SETTINGS = (
    "DRASI_DEMO_PG_HOST", "DRASI_DEMO_PG_DATABASE", "DRASI_DEMO_PG_USER",
    "DRASI_DEMO_PG_PASSWORD",
)


class ObservabilityBundleTests(unittest.TestCase):
    def test_live_requires_explicit_unsigned_local_plugin_trust(self):
        environment = os.environ.copy()
        environment.pop("DRASI_DEMO_TRUST_LOCAL_PLUGINS", None)
        for command in (["live", "prometheus"], ["live-postgres", "otlp"]):
            result = subprocess.run(
                ["bash", str(ROOT / "examples/observability/demo.sh"), *command],
                cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 2, command)
            self.assertIn("DRASI_DEMO_TRUST_LOCAL_PLUGINS=1", result.stderr)

    def test_live_postgres_requires_explicit_database_settings_before_preparing(self):
        environment = os.environ.copy()
        environment["DRASI_DEMO_TRUST_LOCAL_PLUGINS"] = "1"
        for name in PG_SETTINGS:
            environment.pop(name, None)
        runs = ROOT / "target/observability-example/runs"
        before = set(runs.iterdir()) if runs.is_dir() else set()
        result = subprocess.run(
            ["bash", str(ROOT / "examples/observability/demo.sh"), "live-postgres"],
            cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("DRASI_DEMO_PG_HOST", result.stderr)
        after = set(runs.iterdir()) if runs.is_dir() else set()
        self.assertEqual(before, after, "no run directory before settings are complete")

    def test_scenarios_replace_mock_and_use_separate_bundles(self):
        http = bundle.plugin_names("http")
        postgres = bundle.plugin_names("postgres")
        self.assertFalse(any("source_mock" in name for name in http + postgres))
        self.assertTrue(any("drasi_source_http" in name for name in http))
        self.assertTrue(any("drasi_source_postgres" in name for name in postgres))
        self.assertFalse(any("bootstrap" in name for name in postgres))
        for names in (http, postgres):
            self.assertTrue(any("transformer_query" in name for name in names))
            self.assertTrue(any("reaction_log" in name for name in names))
        self.assertNotEqual(bundle.bundle_dir("http"), bundle.bundle_dir("postgres"))
        for scenario in ("http", "postgres"):
            self.assertNotEqual(bundle.bundle_dir(scenario), ROOT / "target/debug/plugins")
        with self.assertRaises(ValueError):
            bundle.plugin_names("mock")

    def setUp(self):
        scratch = ROOT / "target/observability-example"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="proof-", dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.server = self.root / "server"
        self.core = self.root / "core"
        for root in [self.server, self.core]:
            root.mkdir()
            subprocess.run(["git", "init", "--quiet", str(root)], check=True)
            (root / ".gitignore").write_text("target/\n")
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("// original\n")
        self.binary = self.server / "target/debug/drasi-server"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b"fixture server, never executed")
        self.plugins = self.server / "target/debug/plugins"
        self.plugins.mkdir()
        self.names = bundle.plugin_names(SCENARIO)
        for name in self.names:
            (self.plugins / name).write_bytes(b"fixture plugin, never opened")
        self.manifest = self.server / "target/local-build.json"

    def state(self):
        return bundle.bundle_state(
            self.server, self.core, self.binary, self.plugins, "fixture rustc", SCENARIO,
        )

    def record(self):
        self.manifest.write_text(json.dumps(self.state()))

    def test_receipt_records_scenario_and_rejects_other_or_old_receipts(self):
        self.record()
        recorded = json.loads(self.manifest.read_text())
        self.assertEqual(recorded["scenario"], SCENARIO)
        self.assertEqual(recorded["plugins"], list(self.names))
        bundle.verify_bundle(self.manifest, self.state())
        recorded["scenario"] = "postgres"
        self.manifest.write_text(json.dumps(recorded))
        with self.assertRaisesRegex(ValueError, "build-plugins"):
            bundle.verify_bundle(self.manifest, self.state())
        old = self.state()
        old["format"] = 1
        del old["scenario"], old["plugins"]
        self.manifest.write_text(json.dumps(old))
        with self.assertRaisesRegex(ValueError, "build-plugins"):
            bundle.verify_bundle(self.manifest, self.state())

    def test_existing_binaries_without_build_receipt_are_not_adopted(self):
        with self.assertRaisesRegex(ValueError, "build-plugins"):
            bundle.verify_bundle(self.manifest, self.state())

    def test_receipt_rejects_changed_core_and_untracked_source(self):
        self.record()
        bundle.verify_bundle(self.manifest, self.state())
        (self.core / "src/lib.rs").write_text("// uncommitted Core guard change\n")
        with self.assertRaisesRegex(ValueError, "build-plugins"):
            bundle.verify_bundle(self.manifest, self.state())
        self.record()
        (self.core / "src/new.rs").write_text("// untracked build input\n")
        with self.assertRaisesRegex(ValueError, "build-plugins"):
            bundle.verify_bundle(self.manifest, self.state())

    def test_receipt_rejects_replaced_server_or_plugin(self):
        for path in [self.binary, self.plugins / self.names[0]]:
            with self.subTest(path=path):
                self.record()
                path.write_bytes(path.read_bytes() + b"changed")
                with self.assertRaisesRegex(ValueError, "build-plugins"):
                    bundle.verify_bundle(self.manifest, self.state())

    def test_missing_plugin_fails_closed(self):
        (self.plugins / self.names[1]).unlink()
        with self.assertRaisesRegex(ValueError, "build-plugins"):
            self.state()

    def test_runs_are_isolated_and_copy_only_the_verified_three_libraries(self):
        (self.plugins / "unrelated.dylib").write_bytes(b"must not be loaded")
        first = bundle.create_run(self.root / "runs", self.plugins, SCENARIO)
        (first / "data").mkdir()
        sentinel = first / "data/keep.wal"
        sentinel.write_bytes(b"owned previous run")
        second = bundle.create_run(self.root / "runs", self.plugins, SCENARIO)
        self.assertNotEqual(first, second)
        self.assertEqual(sentinel.read_bytes(), b"owned previous run")
        for run in [first, second]:
            self.assertEqual(
                {path.name for path in (run / "plugins").iterdir()}, set(self.names),
            )
            self.assertTrue((run / "demo-owner.json").is_file())
            self.assertFalse((run / "data").is_symlink())


if __name__ == "__main__":
    unittest.main()
