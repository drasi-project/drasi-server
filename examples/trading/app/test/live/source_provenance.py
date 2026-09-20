# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

"""Verify the built server's pinned source using the shared prerequisite policy."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def selected_engine(metadata, core_root):
    expected = {
        "drasi-core": ("0.5.8", "core"),
        "drasi-query-ast": ("0.3.5", "query-ast"),
        "drasi-query-cypher": ("0.3.6", "query-cypher"),
        "drasi-index-rocksdb": ("0.6.1", None),
        "drasi-query-gql": ("0.3.6", None),
    }
    result = {}
    for name, (version, folder) in expected.items():
        matches = [package for package in metadata["packages"] if package["name"] == name]
        if len(matches) != 1:
            raise ValueError(f"Expected one selected {name}")
        package = matches[0]
        if package["version"] != version:
            raise ValueError(f"Unexpected selected {name} version")
        if folder is not None:
            if (
                package["source"] is not None
                or Path(package["manifest_path"]).resolve() != core_root / folder / "Cargo.toml"
            ):
                raise ValueError(f"{name} does not use the verified engine source")
        elif package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"{name} must remain registry-sourced")
        result[name] = {
            key: package[key] for key in ("version", "source", "manifest_path")
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--cargo-lock", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--plugin-lock", type=Path, required=True)
    arguments = parser.parse_args()
    root = arguments.source_root.resolve()
    sys.path.insert(0, str(root / "scripts"))
    import install_plugins
    import plugin_origin

    try:
        if plugin_origin.ROOT != root:
            raise ValueError("Shared plugin policy does not belong to the selected server source")
        subprocess.run(
            ["bash", str(root / "scripts/prepare-core.sh"), "--check"],
            check=True, stdout=sys.stderr,
        )
        revision = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "--verify", "HEAD^{commit}"],
            text=True,
        ).strip()
        if revision != arguments.revision:
            raise ValueError("Caller server revision differs from the checked-out source")
        if arguments.cargo_lock.read_bytes() != (root / "Cargo.lock").read_bytes():
            raise ValueError("Caller dependency lock differs from the checked-out server")
        metadata = plugin_origin.cargo_metadata(root / "Cargo.toml")
        mode, sdk = plugin_origin.classify(metadata)
        if mode != "registry":
            raise ValueError("The integrated live gate requires the reviewed registry SDK/ABI pins")
        core_root = (root.parent / "drasi-core").resolve()
        selected = selected_engine(metadata, core_root)
        selected.update({
            name: {key: package[key] for key in ("version", "source", "manifest_path")}
            for name, package in sdk.items()
        })
        approved_lock, target = install_plugins.pinned_lock_path(
            platform.system(), platform.machine(),
        )
        if arguments.plugin_lock.read_bytes() != approved_lock.read_bytes():
            raise ValueError("Caller Trading plugin lock differs from the shared reviewed pins")
        pins = install_plugins.read_pins(approved_lock, target)
        print(json.dumps({
            "classification": "checked-out integrated default source; not an overlay or released fix",
            "serverRevision": revision,
            "sourceRoot": str(root),
            "engineGit": (root / ".drasi-core-revision").read_text().strip(),
            "engineSourceRoot": str(core_root),
            "engineSourceVerifiedClean": True,
            "serverManifestSha256": digest(root / "Cargo.toml"),
            "serverCargoLockSha256": digest(root / "Cargo.lock"),
            "pluginOrigin": mode,
            "pluginLockSha256": digest(approved_lock),
            "selectedDependencies": selected,
            "pluginPins": pins,
        }))
    except (plugin_origin.PluginOriginError, subprocess.CalledProcessError, OSError, ValueError, KeyError) as error:
        print(f"Live source verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
