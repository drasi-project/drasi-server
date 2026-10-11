# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import copy
from contextlib import ExitStack
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

from test_plugin_origin import fixture, package, plugin_origin, replace_package


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "p1_source_provenance",
    ROOT / "examples/trading/app/test/live/source_provenance.py",
)
provenance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(provenance)


def locked_packages():
    return {"package": [
        {"name": name, "version": version, "source": plugin_origin.REGISTRY, "checksum": checksum}
        for name, (version, checksum) in plugin_origin.REGISTRY_PACKAGES.items()
    ]}


class LiveSourceProvenanceTests(unittest.TestCase):
    def selected(self, metadata=None, manifest=None, lock=None):
        return provenance.selected_registry(
            metadata if metadata is not None else fixture(),
            manifest if manifest is not None else {},
            lock if lock is not None else locked_packages(),
            plugin_origin,
        )

    def test_selects_all_reviewed_registry_identities_without_a_sibling(self):
        selected = self.selected()
        self.assertEqual(set(selected), set(plugin_origin.REGISTRY_PACKAGES))
        for name, entry in selected.items():
            self.assertEqual(entry["source"], plugin_origin.REGISTRY)
            self.assertEqual(entry["version"], plugin_origin.REGISTRY_PACKAGES[name][0])

    def test_rejects_old_release_versions(self):
        for name, version in (
            ("drasi-core", "0.5.8"), ("drasi-core", "0.5.9"),
            ("drasi-lib", "0.9.2"), ("drasi-index-rocksdb", "0.6.3"),
            ("drasi-plugin-sdk", "0.11.2"), ("drasi-query-gql", "0.4.0"),
        ):
            with self.subTest(name=name, version=version):
                metadata = fixture()
                replace_package(metadata, package(name, version))
                with self.assertRaises(plugin_origin.PluginOriginError):
                    self.selected(metadata=metadata)

    def test_rejects_local_and_git_overrides_with_identical_versions(self):
        for name in plugin_origin.REGISTRY_PACKAGES:
            for source in (None, "git+https://example.invalid/unreviewed"):
                with self.subTest(name=name, source=source):
                    metadata = fixture()
                    replace_package(metadata, package(name, plugin_origin.REGISTRY_PACKAGES[name][0], source))
                    with self.assertRaises(plugin_origin.PluginOriginError):
                        self.selected(metadata=metadata)

    def test_rejects_missing_and_duplicate_reachable_identities(self):
        for name in plugin_origin.REGISTRY_PACKAGES:
            for duplicate in (False, True):
                with self.subTest(name=name, duplicate=duplicate):
                    metadata = fixture()
                    entry = next(item for item in metadata["packages"] if item["name"] == name)
                    if duplicate:
                        other = {**entry, "id": entry["id"] + "-duplicate"}
                        metadata["packages"].append(other)
                        metadata["resolve"]["nodes"][0]["deps"].append({"pkg": other["id"]})
                        metadata["resolve"]["nodes"].append({"id": other["id"], "deps": []})
                    else:
                        metadata["resolve"]["nodes"][0]["deps"] = [
                            dep for dep in metadata["resolve"]["nodes"][0]["deps"] if dep["pkg"] != entry["id"]
                        ]
                    with self.assertRaises(plugin_origin.PluginOriginError):
                        self.selected(metadata=metadata)

    def test_rejects_changed_missing_and_duplicate_lock_checksums(self):
        for name in plugin_origin.REGISTRY_PACKAGES:
            for operation in ("change", "remove", "duplicate"):
                with self.subTest(name=name, operation=operation):
                    lock = locked_packages()
                    entry = next(item for item in lock["package"] if item["name"] == name)
                    if operation == "change":
                        entry["checksum"] = "0" * 64
                    elif operation == "remove":
                        del entry["checksum"]
                    else:
                        lock["package"].append(copy.deepcopy(entry))
                    with self.assertRaises(plugin_origin.PluginOriginError):
                        self.selected(lock=lock)

    def test_rejects_even_unused_or_same_version_source_patches(self):
        for field in ("patch", "replace"):
            with self.subTest(field=field):
                with self.assertRaisesRegex(ValueError, "source overrides"):
                    self.selected(manifest={field: {"crates-io": {"drasi-core": {"path": "../drasi-core/core"}}}})
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "unused source patches"):
            self.selected(lock={**locked_packages(), "patch": {"unused": [{"name": "drasi-core"}]}})

    def test_explicit_local_mode_is_not_a_passing_released_live_gate(self):
        metadata = fixture()
        for name in (*plugin_origin.SDK_PACKAGES, "drasi-lib"):
            replace_package(metadata, package(name, plugin_origin.REGISTRY_PACKAGES[name][0], None))
        with self.assertRaisesRegex(ValueError, "requires registry dependencies"):
            self.selected(metadata=metadata)


class PublishedArchiveTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name).resolve()
        self.package = {
            "name": "drasi-core",
            "version": plugin_origin.REGISTRY_PACKAGES["drasi-core"][0],
            "manifest_path": str(self.root / "Cargo.toml"),
        }

    def archive(self, revision=provenance.RELEASE_REVISION, include_vcs=True, extra_files=None):
        files = {"Cargo.toml": b"[package]\n", "src/lib.rs": b"pub fn example() {}\n"}
        if include_vcs:
            files[".cargo_vcs_info.json"] = json.dumps({
                "git": {"sha1": revision}, "path_in_vcs": "core",
            }).encode()
        files.update(extra_files or {})
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, content in files.items():
                source = self.root / name
                source.parent.mkdir(parents=True, exist_ok=True)
                source.write_bytes(content)
                info = tarfile.TarInfo(f"{self.package['name']}-{self.package['version']}/{name}")
                info.size = len(content)
                archive.addfile(info, io.BytesIO(content))
        data = buffer.getvalue()
        return data, hashlib.sha256(data).hexdigest()

    def test_verified_archive_binds_vcs_and_resolved_source_bytes(self):
        data, checksum = self.archive()
        proof = provenance.verify_archive(self.package, checksum, data)
        self.assertEqual(proof["sha256"], checksum)
        self.assertEqual(proof["vcs"]["git"]["sha1"], provenance.RELEASE_REVISION)
        self.assertTrue(proof["resolvedSourceMatchesArchive"])
        self.assertEqual(proof["verifiedFileCount"], 3)
        self.assertIn("src/lib.rs", proof["rustSourceSha256"])

    def test_rejects_wrong_archive_checksum_before_reading_source(self):
        data, _ = self.archive()
        with self.assertRaisesRegex(ValueError, "archive checksum"):
            provenance.verify_archive(self.package, "0" * 64, data)

    def test_rejects_modified_or_missing_resolved_source(self):
        data, checksum = self.archive()
        (self.root / "src/lib.rs").write_text("modified")
        with self.assertRaisesRegex(ValueError, "differs from published archive"):
            provenance.verify_archive(self.package, checksum, data)
        (self.root / "src/lib.rs").unlink()
        with self.assertRaises(FileNotFoundError):
            provenance.verify_archive(self.package, checksum, data)

    def test_rejects_missing_and_unapproved_archive_vcs(self):
        for revision, include in (("0" * 40, True), (provenance.RELEASE_REVISION, False)):
            with self.subTest(revision=revision, include=include):
                data, checksum = self.archive(revision, include)
                with self.assertRaisesRegex(ValueError, "VCS identity"):
                    provenance.verify_archive(self.package, checksum, data)

    def test_unchanged_parsers_keep_their_actual_older_published_revision(self):
        self.package.update(name="drasi-query-ast", version=plugin_origin.REGISTRY_PACKAGES["drasi-query-ast"][0])
        data, checksum = self.archive(provenance.PARSER_REVISION)
        proof = provenance.verify_archive(self.package, checksum, data)
        self.assertEqual(proof["vcs"]["git"]["sha1"], provenance.PARSER_REVISION)
        data, checksum = self.archive(provenance.RELEASE_REVISION)
        with self.assertRaisesRegex(ValueError, "VCS identity"):
            provenance.verify_archive(self.package, checksum, data)

    def documentation_fixture(self, aliases, tampered=False, hard_links=False, both_spellings=False):
        self.package.update(name="drasi-middleware", version=plugin_origin.REGISTRY_PACKAGES["drasi-middleware"][0])
        entries = {"README.md": b"middleware documentation", "readme.md": b"workspace documentation"}
        data, checksum = self.archive(extra_files=entries)
        original_read, original_stat, original_same, original_iter = Path.read_bytes, Path.stat, Path.samefile, Path.iterdir

        def is_readme(path):
            return path.parent == self.root and path.name in entries

        def read(path):
            if is_readme(path):
                content = entries["readme.md" if aliases else path.name]
                return content + (b"tampered" if tampered and path.name == "README.md" else b"")
            return original_read(path)

        def stat(path, **kwargs):
            if is_readme(path):
                return SimpleNamespace(st_dev=-1, st_ino=1 if aliases else list(entries).index(path.name) + 1,
                                       st_nlink=2 if hard_links else 1, st_mode=0o100644)
            return original_stat(path, **kwargs)

        def same(path, other):
            return aliases if is_readme(path) and is_readme(other) else original_same(path, other)

        def directory(path):
            if path == self.root:
                return iter([self.root / name for name in (
                    ["README.md"] if aliases and not both_spellings else list(entries)
                )])
            return original_iter(path)

        with ExitStack() as stack:
            stack.enter_context(patch.object(Path, "read_bytes", read))
            stack.enter_context(patch.object(Path, "stat", stat))
            stack.enter_context(patch.object(Path, "samefile", same))
            stack.enter_context(patch.object(Path, "iterdir", directory))
            return provenance.verify_archive(self.package, checksum, data)

    def test_case_insensitive_documentation_alias_checks_the_last_archive_entry(self):
        proof = self.documentation_fixture(aliases=True)
        self.assertEqual(proof["archiveFileCount"], 5)
        self.assertEqual(proof["verifiedFileCount"], 4)
        collision, = proof["documentationCaseCollisions"]
        self.assertEqual(collision["retainedArchivePath"], "readme.md")
        self.assertEqual(set(collision["archiveHashes"]), {"README.md", "readme.md"})

    def test_case_sensitive_documentation_files_are_both_verified(self):
        proof = self.documentation_fixture(aliases=False)
        self.assertEqual(proof["verifiedFileCount"], 5)
        self.assertEqual(proof["documentationCaseCollisions"], [])

    def test_documentation_tampering_fails_on_both_filesystems(self):
        for aliases in (False, True):
            with self.subTest(aliases=aliases):
                with self.assertRaisesRegex(ValueError, "differs from published archive"):
                    self.documentation_fixture(aliases=aliases, tampered=True)

    def test_documentation_hard_links_are_not_case_aliases(self):
        with self.assertRaisesRegex(ValueError, "hard-linked"):
            self.documentation_fixture(aliases=True, hard_links=True)

    def test_two_directory_entries_are_not_a_case_insensitive_alias(self):
        with self.assertRaisesRegex(ValueError, "hard-linked"):
            self.documentation_fixture(aliases=True, both_spellings=True)

    def test_rejects_duplicate_exact_archive_names(self):
        data, _ = self.archive()
        output = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as original:
            with tarfile.open(fileobj=output, mode="w:gz") as duplicate:
                for member in original:
                    contents = original.extractfile(member).read()
                    duplicate.addfile(member, io.BytesIO(contents))
                    if member.name.endswith("/src/lib.rs"):
                        duplicate.addfile(member, io.BytesIO(contents))
        data = output.getvalue()
        with self.assertRaisesRegex(ValueError, "duplicate published archive entry"):
            provenance.verify_archive(self.package, hashlib.sha256(data).hexdigest(), data)

    def test_rejects_symlinked_resolved_source(self):
        data, checksum = self.archive()
        source = self.root / "src/lib.rs"
        real = self.root / "original.rs"
        real.write_bytes(source.read_bytes())
        source.unlink()
        source.symlink_to(real)
        with self.assertRaisesRegex(ValueError, "Linked published source"):
            provenance.verify_archive(self.package, checksum, data)

    def test_non_documentation_aliases_never_get_the_last_entry_exception(self):
        data, checksum = self.archive(extra_files={"src/other.rs": b"other"})
        original_stat = Path.stat

        def stat(path, **kwargs):
            if path.parent == self.root / "src":
                return SimpleNamespace(st_dev=-1, st_ino=1, st_nlink=1, st_mode=0o100644)
            return original_stat(path, **kwargs)

        with patch.object(Path, "stat", stat):
            with self.assertRaisesRegex(ValueError, "Unexpected published source file alias"):
                provenance.verify_archive(self.package, checksum, data)


if __name__ == "__main__":
    unittest.main()
