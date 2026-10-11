#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
"""Reuse backend source provenance and immutable plugin policy, never frontend code."""

import argparse
import json
from pathlib import Path
import platform
import subprocess
import sys
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source_root", type=Path)
parser.add_argument("loaded_metadata", type=Path, nargs="?")
parser.add_argument("--source", action="store_true")
arguments = parser.parse_args()
if arguments.source == bool(arguments.loaded_metadata):
    parser.error("Choose --source or a loaded-metadata file")

source_root = arguments.source_root.resolve(strict=True)
sys.path.insert(0, str(source_root / "scripts"))
from install_plugins import group_pins, pinned_lock_path, validate_loaded_plugins  # noqa: E402
from plugin_origin import PLUGIN_ABI_VERSION, ROOT  # noqa: E402

if ROOT != source_root:
    raise ValueError("The shared policy must belong to the selected backend checkout")

if arguments.source:
    revision = subprocess.check_output(
        ["git", "-C", str(source_root), "rev-parse", "--verify", "HEAD^{commit}"],
        text=True,
    ).strip()
    lock, _ = pinned_lock_path(platform.system(), platform.machine())
    verifier = source_root / "examples/trading/app/test/live/source_provenance.py"
    proof = json.loads(subprocess.check_output([
        sys.executable, str(verifier), "--source-root", str(source_root),
        "--cargo-lock", str(source_root / "Cargo.lock"), "--revision", revision,
        "--plugin-lock", str(lock),
    ], text=True, timeout=120))
    manifest = tomllib.loads((source_root / "Cargo.toml").read_text())["package"]
    packages = tomllib.loads((source_root / "Cargo.lock").read_text())["package"]
    servers = [package for package in packages if package["name"] == "drasi-server"]
    if manifest["name"] != "drasi-server" or len(servers) != 1 or servers[0]["version"] != manifest["version"]:
        raise ValueError("Server manifest and locked version disagree")
    proof["serverVersion"] = manifest["version"]
    proof["pluginAbiVersion"] = PLUGIN_ABI_VERSION
    proof["serverSourceDirty"] = bool(subprocess.check_output([
        "git", "-C", str(source_root), "status", "--porcelain", "--untracked-files=no",
    ], text=True).strip())
    print(json.dumps(proof))
else:
    plugins = json.loads(arguments.loaded_metadata.read_text())
    validate_loaded_plugins(
        plugins["plugins"], group_pins("trading", platform.system(), platform.machine())
    )
    print("Loaded plugins match the reviewed signed hashes, versions and ABI.")
