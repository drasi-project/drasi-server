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
import tomllib


ROOT = Path(__file__).resolve().parent.parent
SDK_PACKAGES = ("drasi-host-sdk", "drasi-plugin-sdk", "drasi-ffi-primitives")
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
REGISTRY_SDK_VERSION = "0.11.3"
REGISTRY_LIB_VERSION = "0.9.3"
PLUGIN_ABI_VERSION = "0.14.0"

# Official archive checksums for the reviewed release; Cargo verifies downloaded bytes.
# Source and reconstruction boundaries are documented in docs/main-runtime-integration.md.
REGISTRY_PACKAGES = {
    "drasi-core": ("0.5.10", "9e06737256dbabee9ff58a5a74ed267bf41f20437d0e9397755a3a8c73de5d63"),
    "drasi-lib": ("0.9.3", "90757ef1daf6f96da9d350d263b32942ccf0ee197f289f4673f558485ec88d4d"),
    "drasi-host-sdk": ("0.11.3", "f8b3640eab33216fa1f78c709951126dd81e2c96df2b29c71a324ef201d11090"),
    "drasi-plugin-sdk": ("0.11.3", "4f540dfeea3d9443c27f9a56da3627772bc16b52021d968d46223438de27cda5"),
    "drasi-ffi-primitives": ("0.11.3", "5104f2bd052d320f9f062ab24cc10e4bcabbf6ceb444e111fc07cdf62babc577"),
    "drasi-index-rocksdb": ("0.6.4", "a9d02d07abeb268d6d45fb10550a27c0c846a1e290dc204ef869479250a6b047"),
    "drasi-query-ast": ("0.3.5", "4f2621be8fade50d49f1ef475e0f5f7e4bd5f4ac60983fca5b4d46f2ea307087"),
    "drasi-query-cypher": ("0.3.6", "e45aa57f6e9e997a26b1266f92f69abf4f10ebe69d2f5ffc193a7ca2a6a416ae"),
    "drasi-query-gql": ("0.3.6", "023cb055fb27d8b8a033becf33811bb914c6151340e0efda56972367937885c8"),
    "drasi-functions-cypher": ("0.5.10", "d59d003512a6cf59fe510a7991dd05bf6edd114e0a5aafba9d085bc82f8d2415"),
    "drasi-functions-gql": ("0.5.10", "397da52359c1f97cc002c812f259c53a4699e41d093e1865192ecbcbc7cd3d7d"),
    "drasi-middleware": ("0.5.11", "3fcb1c840aac9d83a5bf3d8c3dea5de283ed5fadb3a830d49862193f3b4d9c7a"),
    "drasi-bootstrap-noop": ("0.2.15", "caf70366de10cbc4d2a84a7f52e7dfca3fc301ecef922cb776c6e4077231647e"),
    "drasi-bootstrap-application": ("0.2.15", "ef735ba980e68755ae677d0cd754198c652bc788c3effa59e51e001409be4cac"),
    "drasi-reaction-application": ("0.3.13", "f86d4698d4fe111e494439c65ceb6706fb1746a9954e098a2791d07d8e8adf7e"),
    "drasi-state-store-redb": ("0.2.8", "8d8570f07e3076d5c04880e21ca2b1374db6b4407562188b985ad49410f9bda4"),
    "drasi-wal-redb": ("0.2.10", "d0a220606c2dc3f9d6857ed14083a01864bec8c16ad62ddfd46af536ef3f93e2"),
}


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


def selected_packages(metadata, names=(*SDK_PACKAGES, "drasi-lib")):
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
    for name in names:
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
        if any(selected[name]["version"] != REGISTRY_SDK_VERSION for name in SDK_PACKAGES):
            raise PluginOriginError(
                f"Registry plugin pins require SDK/host/FFI crates {REGISTRY_SDK_VERSION}"
            )
        library = selected["drasi-lib"]
        if library["source"] != REGISTRY or library["version"] != REGISTRY_LIB_VERSION:
            raise PluginOriginError(
                f"Registry plugin pins require registry drasi-lib {REGISTRY_LIB_VERSION}"
            )
        released = selected_packages(metadata, REGISTRY_PACKAGES)
        for name, package in released.items():
            version, _ = REGISTRY_PACKAGES[name]
            if package["source"] != REGISTRY or package["version"] != version:
                raise PluginOriginError(
                    f"Released runtime requires registry {name} {version}; "
                    "local SDK development must select a matching local workspace"
                )
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


def validate_registry_lock(lock):
    if lock.get("patch", {}).get("unused"):
        raise PluginOriginError("Released runtime cannot contain unused source patches")
    for name, (version, checksum) in REGISTRY_PACKAGES.items():
        matches = [package for package in lock["package"] if package["name"] == name]
        if (
            len(matches) != 1
            or matches[0]["version"] != version
            or matches[0].get("source") != REGISTRY
            or matches[0].get("checksum") != checksum
        ):
            raise PluginOriginError(f"Unreviewed released package identity or checksum: {name}")


def resolve_origin():
    mode, selected = classify(cargo_metadata(ROOT / "Cargo.toml"))
    if mode == "registry":
        validate_registry_lock(tomllib.loads((ROOT / "Cargo.lock").read_text()))
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
