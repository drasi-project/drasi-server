#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

"""Select plugin tooling from the dependencies Cargo actually resolves."""

import argparse
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
SDK_PACKAGES = ("drasi-host-sdk", "drasi-plugin-sdk", "drasi-ffi-primitives")
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


class PluginOriginError(Exception):
    pass


def cargo_metadata(manifest, *, no_deps=False):
    command = [
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--manifest-path", str(manifest),
    ]
    if no_deps:
        command.append("--no-deps")
    result = subprocess.run(
        command, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True,
    )
    return json.loads(result.stdout)


def selected_packages(metadata):
    packages = {package["id"]: package for package in metadata["packages"]}
    servers = [
        package for package in packages.values()
        if package["name"] == "drasi-server"
        and package["id"] in metadata["workspace_members"]
    ]
    if len(servers) != 1:
        raise PluginOriginError("Expected exactly one drasi-server workspace package")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending = [servers[0]["id"]]
    reachable = set()
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"])

    selected = {}
    for name in (*SDK_PACKAGES, "drasi-lib"):
        matches = [packages[key] for key in reachable if packages[key]["name"] == name]
        if len(matches) != 1:
            raise PluginOriginError(
                f"Expected one resolved {name} identity, found {len(matches)}"
            )
        selected[name] = matches[0]
    return selected


def classify(metadata):
    selected = selected_packages(metadata)
    sources = [selected[name]["source"] for name in SDK_PACKAGES]
    if all(source == REGISTRY for source in sources):
        if any(selected[name]["version"] != "0.10.0" for name in SDK_PACKAGES):
            raise PluginOriginError("Registry plugin pins require SDK/host/FFI crates 0.10.0")
        library = selected["drasi-lib"]
        if library["source"] != REGISTRY or library["version"] != "0.8.9":
            raise PluginOriginError("Registry plugin pins require registry drasi-lib 0.8.9")
        return "registry", selected
    if all(source is None for source in sources):
        return "local", selected
    raise PluginOriginError(
        "Mixed or unsupported SDK/host/FFI sources; cannot select compatible plugins"
    )


def matching_local_workspace(selected, metadata):
    workspace = Path(metadata["workspace_root"]).resolve()
    members = {
        package["id"]: package for package in metadata["packages"]
        if package["id"] in metadata["workspace_members"]
    }
    for name, package in selected.items():
        matches = [member for member in members.values() if member["name"] == name]
        if len(matches) != 1:
            raise PluginOriginError(f"Local SDK workspace must contain exactly one {name}")
        member = matches[0]
        if (
            package["source"] is not None
            or member["source"] is not None
            or package["version"] != member["version"]
            or Path(package["manifest_path"]).resolve()
            != Path(member["manifest_path"]).resolve()
        ):
            raise PluginOriginError(
                f"Server's {name} does not match the local plugin-build workspace"
            )
    return workspace


def resolve_origin():
    mode, selected = classify(cargo_metadata(ROOT / "Cargo.toml"))
    if mode == "registry":
        return mode, None
    sdk_manifest = Path(selected["drasi-plugin-sdk"]["manifest_path"])
    workspace = matching_local_workspace(
        selected, cargo_metadata(sdk_manifest, no_deps=True),
    )
    return mode, workspace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", choices=("mode", "local-workspace"))
    arguments = parser.parse_args()
    try:
        mode, workspace = resolve_origin()
        if arguments.output == "local-workspace":
            if mode != "local":
                raise PluginOriginError(
                    "The server consumes registry SDKs, not local SDKs. "
                    "Use verified registry plugins; an engine-only path patch "
                    "does not authorize building plugins from unused siblings."
                )
            print(workspace)
        else:
            print(mode)
    except (PluginOriginError, subprocess.CalledProcessError, OSError, ValueError, KeyError) as error:
        print(f"Plugin dependency selection failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
