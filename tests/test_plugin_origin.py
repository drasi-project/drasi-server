# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import copy
import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("plugin_origin", ROOT / "scripts/plugin_origin.py")
plugin_origin = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(plugin_origin)


def package(name, version, source=plugin_origin.REGISTRY, workspace="/fixture/core"):
    suffix = name.removeprefix("drasi-")
    manifest = (
        f"{workspace}/lib/Cargo.toml" if name == "drasi-lib"
        else f"{workspace}/components/{suffix}/Cargo.toml"
    )
    return {
        "name": name,
        "version": version,
        "source": source,
        "manifest_path": manifest,
        "id": f"{source or 'path+' + manifest}#{name}@{version}",
    }


def fixture():
    server = package("drasi-server", "0.2.3", None, "/fixture/server")
    packages = [
        server,
        *(package(name, version) for name, (version, _) in plugin_origin.REGISTRY_PACKAGES.items()),
    ]
    return {
        "workspace_members": [server["id"]],
        "workspace_root": "/fixture/server",
        "packages": packages,
        "resolve": {
            "nodes": [
                {
                    "id": entry["id"],
                    "deps": (
                        [{"pkg": dependency["id"]} for dependency in packages[1:]]
                        if entry is server else []
                    ),
                }
                for entry in packages
            ]
        },
    }


def replace_package(metadata, replacement):
    previous = next(
        package for package in metadata["packages"]
        if package["name"] == replacement["name"]
    )
    for node in metadata["resolve"]["nodes"]:
        if node["id"] == previous["id"]:
            node["id"] = replacement["id"]
        for dependency in node["deps"]:
            if dependency["pkg"] == previous["id"]:
                dependency["pkg"] = replacement["id"]
    metadata["packages"][metadata["packages"].index(previous)] = replacement


class PluginOriginTests(unittest.TestCase):
    def test_all_registry_graph_uses_pinned_plugins(self):
        mode, selected = plugin_origin.classify(fixture())
        self.assertEqual(mode, "registry")
        self.assertEqual(selected["drasi-plugin-sdk"]["version"], "0.11.3")
        self.assertEqual(selected["drasi-lib"]["version"], "0.9.3")

    def test_released_runtime_rejects_local_git_or_older_core_and_parsers(self):
        for name in ("drasi-core", "drasi-query-ast", "drasi-query-cypher"):
            for source in (None, "git+https://example.invalid/core"):
                with self.subTest(name=name, source=source):
                    metadata = fixture()
                    version = plugin_origin.REGISTRY_PACKAGES[name][0]
                    replace_package(metadata, package(name, version, source))
                    with self.assertRaisesRegex(plugin_origin.PluginOriginError, f"registry {name}"):
                        plugin_origin.classify(metadata)
        metadata = fixture()
        replace_package(metadata, package("drasi-core", "0.5.9"))
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "registry drasi-core 0.5.10"):
            plugin_origin.classify(metadata)

    def test_unselected_local_sdk_does_not_change_selection(self):
        for version in ("0.9.0", "0.11.3"):
            with self.subTest(version=version):
                metadata = fixture()
                metadata["packages"].extend(
                    package(name, version, None) for name in plugin_origin.SDK_PACKAGES
                )
                metadata["packages"].append(package("drasi-lib", "0.9.3", None))
                self.assertEqual(plugin_origin.classify(metadata)[0], "registry")

    def test_mixed_sdk_sources_fail_instead_of_building_unused_siblings(self):
        metadata = fixture()
        replace_package(metadata, package("drasi-host-sdk", "0.11.3", None))
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "Mixed"):
            plugin_origin.classify(metadata)

    def test_git_sdk_is_not_mistaken_for_registry_or_matching_local(self):
        metadata = fixture()
        for name in plugin_origin.SDK_PACKAGES:
            replace_package(metadata, package(name, "0.11.3", "git+https://example.invalid/sdk"))
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "unsupported"):
            plugin_origin.classify(metadata)

    def test_unapproved_registry_sdk_version_fails(self):
        metadata = fixture()
        replace_package(metadata, package("drasi-plugin-sdk", "0.12.0"))
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "crates 0.11.3"):
            plugin_origin.classify(metadata)

    def test_historical_registry_sdk_is_not_accepted_for_current_pins(self):
        for version in ("0.10.0", "0.11.0", "0.11.1", "0.11.2"):
            with self.subTest(version=version):
                metadata = fixture()
                for name in plugin_origin.SDK_PACKAGES:
                    replace_package(metadata, package(name, version))
                with self.assertRaisesRegex(plugin_origin.PluginOriginError, "crates 0.11.3"):
                    plugin_origin.classify(metadata)

    def test_changed_registry_library_fails(self):
        for version in ("0.9.1", "0.9.2", "0.9.4"):
            with self.subTest(version=version):
                metadata = fixture()
                replace_package(metadata, package("drasi-lib", version))
                with self.assertRaisesRegex(plugin_origin.PluginOriginError, "drasi-lib 0.9.3"):
                    plugin_origin.classify(metadata)

    def test_duplicate_reachable_sdk_identity_fails(self):
        metadata = fixture()
        duplicate = package("drasi-plugin-sdk", "0.12.0")
        metadata["packages"].append(duplicate)
        metadata["resolve"]["nodes"][0]["deps"].append({"pkg": duplicate["id"]})
        metadata["resolve"]["nodes"].append({"id": duplicate["id"], "deps": []})
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "found 2"):
            plugin_origin.classify(metadata)

    def test_release_checksums_and_full_graph_are_not_inferred_from_versions(self):
        lock = {"package": [
            {"name": name, "version": version, "source": plugin_origin.REGISTRY, "checksum": checksum}
            for name, (version, checksum) in plugin_origin.REGISTRY_PACKAGES.items()
        ]}
        plugin_origin.validate_registry_lock(lock)
        for name in plugin_origin.REGISTRY_PACKAGES:
            with self.subTest(name=name):
                changed = copy.deepcopy(lock)
                next(p for p in changed["package"] if p["name"] == name)["checksum"] = "0" * 64
                with self.assertRaisesRegex(plugin_origin.PluginOriginError, name):
                    plugin_origin.validate_registry_lock(changed)
        for changed in (
            {"package": lock["package"][1:]},
            {"package": lock["package"] + [lock["package"][0]]},
            {**lock, "patch": {"unused": [{"name": "drasi-core", "version": "0.5.9"}]}},
        ):
            with self.assertRaises(plugin_origin.PluginOriginError):
                plugin_origin.validate_registry_lock(changed)

    def test_local_mode_requires_the_actual_matching_build_workspace(self):
        metadata = fixture()
        for name in (*plugin_origin.SDK_PACKAGES, "drasi-lib"):
            version = "0.9.3" if name == "drasi-lib" else "0.11.3"
            replace_package(metadata, package(name, version, None))
        mode, selected = plugin_origin.classify(metadata)
        self.assertEqual(mode, "local")
        core = {
            "workspace_root": "/fixture/core",
            "workspace_members": [entry["id"] for entry in selected.values()],
            "packages": list(selected.values()),
        }
        self.assertEqual(
            plugin_origin.matching_local_workspace(selected, core),
            Path("/fixture/core").resolve(),
        )
        wrong_core = copy.deepcopy(core)
        wrong_core["packages"][0]["manifest_path"] = "/fixture/unrelated/host-sdk/Cargo.toml"
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "does not match"):
            plugin_origin.matching_local_workspace(selected, wrong_core)

    def test_local_sdks_with_registry_library_cannot_build_workspace_plugins(self):
        metadata = fixture()
        for name in plugin_origin.SDK_PACKAGES:
            replace_package(metadata, package(name, "0.11.3", None))
        _, selected = plugin_origin.classify(metadata)
        core_packages = [
            package(name, "0.9.3" if name == "drasi-lib" else "0.11.3", None)
            for name in selected
        ]
        core = {
            "workspace_root": "/fixture/core",
            "workspace_members": [entry["id"] for entry in core_packages],
            "packages": core_packages,
        }
        with self.assertRaisesRegex(plugin_origin.PluginOriginError, "drasi-lib"):
            plugin_origin.matching_local_workspace(selected, core)


if __name__ == "__main__":
    unittest.main()
