# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

import importlib.util
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "observability_bundle", ROOT / "examples/observability/local_bundle.py",
)
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)
TRAFFIC_SPEC = importlib.util.spec_from_file_location(
    "observability_traffic", ROOT / "examples/observability/showcase/traffic.py",
)
traffic = importlib.util.module_from_spec(TRAFFIC_SPEC)
TRAFFIC_SPEC.loader.exec_module(traffic)
DEMO = ROOT / "examples/observability/demo.sh"
SHOWCASE = ROOT / "examples/observability/showcase"
PROCESS_FAMILIES = {
    "process_cpu_seconds_total", "process_resident_memory_bytes", "process_virtual_memory_bytes",
    "process_open_fds", "process_max_fds", "process_threads", "process_start_time_seconds",
}
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


class ObservabilityShowcaseTests(unittest.TestCase):
    def demo(self, *arguments, **settings):
        environment = os.environ.copy()
        environment.pop("DRASI_DEMO_TRUST_LOCAL_PLUGINS", None)
        environment.update(settings)
        return subprocess.run(
            ["bash", str(DEMO), *arguments],
            cwd=ROOT, env=environment, capture_output=True, text=True, check=False,
        )

    def test_showcase_and_send_reject_invalid_arguments(self):
        for arguments in (
            ["showcase", "extra"], ["send", "0"], ["send", "1001"], ["send", "05"],
            ["send", "five"], ["send", "1", "2"],
        ):
            result = self.demo(*arguments)
            self.assertEqual(result.returncode, 2, arguments)
            self.assertIn("Usage:", result.stderr, arguments)

    def test_showcase_requires_trust_before_podman_or_a_run_directory(self):
        runs = ROOT / "target/observability-example/runs"
        before = set(runs.iterdir()) if runs.is_dir() else set()
        result = self.demo("showcase")
        self.assertEqual(result.returncode, 2)
        self.assertIn("DRASI_DEMO_TRUST_LOCAL_PLUGINS=1", result.stderr)
        after = set(runs.iterdir()) if runs.is_dir() else set()
        self.assertEqual(before, after)

    def test_send_refuses_without_a_running_demo(self):
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        # Bound but never listening, then closed: nothing answers on this port.
        result = self.demo("send", "3", DRASI_DEMO_PORT=str(port))
        self.assertEqual(result.returncode, 1)
        self.assertIn("No running observability demo", result.stderr)
        self.assertIn("demo.sh showcase", result.stderr)

    def test_trickle_inserts_each_reading_once_then_only_updates_them(self):
        changes = [traffic.trickle_change(sequence) for sequence in range(3 * traffic.READINGS)]
        first, later = changes[:traffic.READINGS], changes[traffic.READINGS:]
        ids = {change["element"]["id"] for change in first}
        self.assertEqual(len(ids), traffic.READINGS)
        self.assertEqual({change["operation"] for change in first}, {"insert"})
        self.assertEqual({change["operation"] for change in later}, {"update"})
        self.assertLessEqual({change["element"]["id"] for change in later}, ids)
        for change in changes:
            self.assertEqual(change["element"]["labels"], ["Reading"])
            self.assertIsInstance(change["element"]["properties"]["value"], float)

    def test_dashboard_uses_documented_families_and_provisioned_data_sources(self):
        dashboard = json.loads(
            (SHOWCASE / "grafana/dashboards/drasi-server-observability.json").read_text(),
        )
        self.assertEqual(dashboard["uid"], "drasi-server-observability")
        self.assertEqual(dashboard["refresh"], "5s")
        self.assertEqual(dashboard["time"], {"from": "now-15m", "to": "now"})
        targets = [target for panel in dashboard["panels"] for target in panel.get("targets", [])]
        expressions = [target["expr"] for target in targets if "expr" in target]
        self.assertIn("rate(process_cpu_seconds_total[1m]) * 100", expressions)
        families = {
            re.sub(r"_(bucket|sum|count)$", "", name) if name.startswith("drasi_") else name
            for expression in expressions
            for name in re.findall(r"\b(?:drasi|process)_[a-z_]+", expression)
        }
        readme = (ROOT / "examples/observability/README.md").read_text()
        for family in families:
            self.assertIn(f"`{family}`", readme, family)
        self.assertEqual(
            {family for family in families if family.startswith("process_")},
            PROCESS_FAMILIES - {"process_virtual_memory_bytes"},
        )
        for family in families - PROCESS_FAMILIES:
            self.assertFalse(family.endswith("_total"), f"{family}: Server adds no _total suffix")
        provisioning = (SHOWCASE / "grafana/provisioning/datasources/drasi-showcase.yaml").read_text()
        used = {target["datasource"]["uid"] for target in targets}
        self.assertEqual(used, {"drasi-prometheus", "drasi-jaeger"})
        for uid in used:
            self.assertIn(f"uid: {uid}", provisioning)

    def test_showcase_publishes_loopback_only_pinned_images_without_credentials(self):
        script = DEMO.read_text()
        published = re.findall(r'--publish "([^"]+)"', script)
        self.assertEqual(len(published), 4)
        for mapping in published:
            self.assertTrue(mapping.startswith("127.0.0.1:"), mapping)
        images = re.findall(r"^[A-Z]+_IMAGE=(\S+)$", script, re.MULTILINE)
        self.assertEqual(len(images), 3)
        for image in images:
            self.assertRegex(image, r":v?\d+\.\d+\.\d+$")
        self.assertNotRegex(script, r"(^|[\s;|&(])docker\s")
        for path in SHOWCASE.rglob("*"):
            if path.suffix in {".yml", ".yaml", ".json"}:
                text = path.read_text().lower()
                for word in ("password", "secret", "token", "basicauth: true"):
                    self.assertNotIn(word, text, path)


if __name__ == "__main__":
    unittest.main()
