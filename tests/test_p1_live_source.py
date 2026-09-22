# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import copy
import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "p1_source_provenance",
    ROOT / "examples/trading/app/test/live/source_provenance.py",
)
provenance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(provenance)
CORE = Path("/fixture/pinned-core")
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def fixture():
    return {"packages": [
        {
            "name": name, "version": version,
            "source": None if folder else REGISTRY,
            "manifest_path": str(CORE / folder / "Cargo.toml") if folder else f"/registry/{name}/Cargo.toml",
        }
        for name, version, folder in [
            ("drasi-core", "0.5.9", "core"),
            ("drasi-query-ast", "0.3.5", "query-ast"),
            ("drasi-query-cypher", "0.3.6", "query-cypher"),
            ("drasi-index-rocksdb", "0.6.3", None),
            ("drasi-query-gql", "0.3.6", None),
        ]
    ]}


class LiveSourceProvenanceTests(unittest.TestCase):
    def test_records_the_exact_engine_parser_and_registry_boundary(self):
        selected = provenance.selected_engine(fixture(), CORE)
        self.assertEqual(len(selected), 5)
        self.assertIsNone(selected["drasi-core"]["source"])
        self.assertEqual(selected["drasi-core"]["version"], "0.5.9")
        self.assertEqual(selected["drasi-query-cypher"]["version"], "0.3.6")
        self.assertEqual(selected["drasi-index-rocksdb"]["version"], "0.6.3")
        self.assertEqual(selected["drasi-index-rocksdb"]["source"], REGISTRY)

    def test_rejects_unapproved_engine_version(self):
        for version in ("0.5.8", "0.6.0"):
            with self.subTest(version=version):
                metadata = fixture()
                metadata["packages"][0]["version"] = version
                with self.assertRaisesRegex(ValueError, "version"):
                    provenance.selected_engine(metadata, CORE)

    def test_rejects_registry_engine_disguised_by_same_version(self):
        metadata = fixture()
        metadata["packages"][0]["source"] = REGISTRY
        with self.assertRaisesRegex(ValueError, "verified engine source"):
            provenance.selected_engine(metadata, CORE)

    def test_rejects_a_different_path_checkout(self):
        metadata = fixture()
        metadata["packages"][1]["manifest_path"] = "/fixture/unrelated/query-ast/Cargo.toml"
        with self.assertRaisesRegex(ValueError, "verified engine source"):
            provenance.selected_engine(metadata, CORE)

    def test_rejects_duplicate_engine_identity(self):
        metadata = fixture()
        metadata["packages"].append(copy.deepcopy(metadata["packages"][0]))
        with self.assertRaisesRegex(ValueError, "one selected drasi-core"):
            provenance.selected_engine(metadata, CORE)

    def test_rejects_an_index_source_override(self):
        metadata = fixture()
        metadata["packages"][3]["source"] = None
        with self.assertRaisesRegex(ValueError, "registry-sourced"):
            provenance.selected_engine(metadata, CORE)

    def test_rejects_the_previous_registry_index_version(self):
        for version in ("0.5.8", "0.6.1"):
            with self.subTest(version=version):
                metadata = fixture()
                metadata["packages"][3]["version"] = version
                with self.assertRaisesRegex(ValueError, "version"):
                    provenance.selected_engine(metadata, CORE)

    def test_rejects_an_unapproved_gql_version(self):
        metadata = fixture()
        metadata["packages"][4]["version"] = "0.4.0"
        with self.assertRaisesRegex(ValueError, "version"):
            provenance.selected_engine(metadata, CORE)


if __name__ == "__main__":
    unittest.main()
