import hashlib
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch


EXPORT = runpy.run_path(str(Path(__file__).with_name("export-runtime.py")))


class ExportTests(unittest.TestCase):
    def test_hosting_exports_have_separate_destinations(self):
        for hosting in ("embedded", "server"):
            with self.subTest(hosting=hosting):
                example, destination = EXPORT["hosting_paths"](hosting)
                self.assertEqual(example, EXPORT["LAB"] / hosting)
                self.assertEqual(destination, example / ".build" / "runtime-src")
        self.assertEqual(EXPORT["hosting_paths"]("embedded")[1], EXPORT["DESTINATION"])
        with self.assertRaises(ValueError):
            EXPORT["hosting_paths"]("../embedded")

    def test_export_roots_match_the_shared_layout(self):
        self.assertEqual(EXPORT["EXAMPLE"], Path(__file__).resolve().parents[1])
        self.assertEqual(EXPORT["DESTINATION"], EXPORT["EXAMPLE"] / ".build" / "runtime-src")
        self.assertEqual(EXPORT["LAB"], EXPORT["EXAMPLE"].parent)
        for relative in (
            "shared/Cargo.toml", "shared/Cargo.lock", "shared/crates/native/Cargo.toml",
            "embedded/src/main.rs", "embedded/control/main.rs", "rust-toolchain.toml",
        ):
            with self.subTest(relative=relative):
                self.assertTrue((EXPORT["LAB"] / relative).is_file())
        self.assertEqual(EXPORT["WORKSPACE"] / "drasi-server" / "examples" / "gpu-cluster-lab",
                         EXPORT["LAB"])

    def test_shared_hasher_sources_and_license_are_selected(self):
        for name in ("mod.rs", "spooky.rs", "LICENSE-MIT"):
            with self.subTest(name=name):
                self.assertTrue(EXPORT["is_source_file"](Path("core/src/hashing") / name))
        for name in ("LICENSE", "NOTICE", "NOTICE-third-party", "Cargo.lock", "computation-contracts.tsv"):
            self.assertTrue(EXPORT["is_source_file"](Path(name)))
        for name in ("gpu-native.so", "build.log", "capture.ndjson"):
            self.assertFalse(EXPORT["is_source_file"](Path(name)))

    def test_selected_license_is_copied_and_fingerprinted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source" / "LICENSE-MIT"
            source.parent.mkdir()
            source.write_text("License fixture\n")
            staging = root / "staging"
            relative = "drasi-core/core/src/hashing/LICENSE-MIT"
            target = staging / relative
            manifest = {}
            EXPORT["copy_file"](source, target, manifest, staging)
            self.assertEqual(target.read_bytes(), source.read_bytes())
            self.assertEqual(manifest, {relative: hashlib.sha256(source.read_bytes()).hexdigest()})

    def test_server_export_preserves_embedded_staging_and_includes_current_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary)
            lab = workspace / "drasi-server" / "examples" / "gpu-cluster-lab"
            files = {
                "shared/Cargo.toml": "shared manifest",
                "shared/Cargo.lock": "shared lock",
                "shared/crates/native/src/lib.rs": "native source",
                "shared/ui/src/App.tsx": "shared UI source",
                "embedded/src/main.rs": "embedded source",
                "embedded/control/main.rs": "control source",
                "embedded/.build/runtime-src/frozen": "preserve embedded export",
                "server/Cargo.toml": "server manifest",
                "server/Cargo.lock": "server lock",
                "server/src/lib.rs": "server source",
                "server/.env": "not for export",
                "server/target/debug/generated.rs": "not for export",
                "rust-toolchain.toml": "toolchain",
            }
            for name, content in files.items():
                path = lab / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            for name in ("drasi-core", "drasi-server"):
                (workspace / name).mkdir(exist_ok=True)
                (workspace / name / "Cargo.toml").write_text("repository manifest")
                (workspace / name / "Cargo.lock").write_text("repository lock")
            function = EXPORT["main"]
            with patch.dict(function.__globals__, LAB=lab, WORKSPACE=workspace), \
                    patch.object(function.__globals__["subprocess"], "check_output",
                                 side_effect=lambda command, **_: "fixture-head\n"
                                 if "rev-parse" in command else b"Cargo.toml\0"):
                function("server")
            self.assertEqual((lab / "embedded/.build/runtime-src/frozen").read_text(),
                             "preserve embedded export")
            destination = lab / "server/.build/runtime-src"
            manifest = json.loads((destination / "source-manifest.json").read_text())
            self.assertIn("drasi-core/Cargo.lock", manifest["files"])
            prefix = "drasi-server/examples/gpu-cluster-lab/"
            for name in ("server/Cargo.toml", "server/Cargo.lock", "server/src/lib.rs",
                         "shared/crates/native/src/lib.rs", "shared/ui/src/App.tsx",
                         "embedded/control/main.rs"):
                self.assertEqual((destination / prefix / name).read_text(), files[name])
                self.assertIn(prefix + name, manifest["files"])
            self.assertFalse((destination / prefix / "server/.env").exists())
            self.assertFalse((destination / prefix / "server/target").exists())


if __name__ == "__main__":
    unittest.main()
