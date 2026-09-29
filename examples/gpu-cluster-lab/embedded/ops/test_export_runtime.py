import hashlib
from pathlib import Path
import runpy
import tempfile
import unittest


EXPORT = runpy.run_path(str(Path(__file__).with_name("export-runtime.py")))


class ExportTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
