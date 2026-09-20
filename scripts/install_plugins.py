#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

"""Install reviewed Trading or test plugin pins using the existing locked CLI."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
import tomllib

from plugin_origin import PLUGIN_ABI_VERSION, PluginOriginError, ROOT, resolve_origin


VERSIONS = {
    "source/http": "0.2.11",
    "source/postgres": "0.2.10",
    "bootstrap/postgres": "0.2.13",
    "bootstrap/scriptfile": "0.2.13",
    "reaction/sse": "0.3.6",
}
TEST_VERSIONS = {
    "source/mock": "0.2.10",
    "reaction/log": "0.2.7",
    "reaction/http": "0.3.3",
}
# The signed release's SDK crate differs from the host crate; both use ABI 0.13.
PLUGIN_SDK_VERSION = "0.11.1"
ISSUER = "https://token.actions.githubusercontent.com"
SUBJECT = (
    "https://github.com/drasi-project/drasi-core/"
    ".github/workflows/publish-plugins.yml@refs/heads/main"
)


def pinned_lock_path(system, machine, group="trading"):
    architectures = {"x86_64": "amd64", "AMD64": "amd64", "aarch64": "arm64", "arm64": "arm64"}
    architecture = architectures.get(machine)
    if system == "Linux" and architecture in ("amd64", "arm64"):
        filename = f"plugins-{architecture}.lock"
        target = f"linux/{architecture}"
    elif system == "Darwin" and architecture == "arm64":
        filename = "plugins-darwin-arm64.lock"
        target = "darwin/arm64"
    else:
        raise PluginOriginError(f"No reviewed plugin pins for {system}/{machine}")
    folder = "examples/trading/app/test/live" if group == "trading" else "tests/plugin-pins"
    return ROOT / folder / filename, target


def read_pins(path, target, versions=VERSIONS):
    lock = tomllib.loads(path.read_text())
    pins = lock["plugins"]
    if lock["version"] != 1 or len(pins) != len(versions):
        raise PluginOriginError(f"Expected the {len(versions)} reviewed plugin pins")
    kinds = set()
    for reference, pin in pins.items():
        kind = reference.split(":")[0]
        kinds.add(kind)
        expected_reference = f"ghcr.io/drasi-project/{kind}@{pin['digest']}"
        signature = pin.get("signature")
        if (
            kind not in versions
            or pin["version"] != versions[kind]
            or pin["sdk_version"] != PLUGIN_SDK_VERSION
            or pin["lib_version"] != "0.9.1"
            or pin["core_version"] != "0.5.8"
            or pin["platform"] != target
            or pin["reference"] != expected_reference
            or re.fullmatch(r"sha256:[a-f0-9]{64}", pin["digest"]) is None
            or re.fullmatch(r"[a-f0-9]{64}", pin["file_hash"]) is None
            or Path(pin["filename"]).name != pin["filename"]
            or "\\" in pin["filename"]
            or signature != {"verified": True, "issuer": ISSUER, "subject": SUBJECT}
        ):
            raise PluginOriginError(f"Unreviewed or invalid plugin pin: {reference}")
    if kinds != set(versions):
        raise PluginOriginError("Plugin pins must include every required kind")
    return pins


def group_pins(group, system, machine):
    trading_path, target = pinned_lock_path(system, machine)
    trading = read_pins(trading_path, target)
    if group == "trading":
        return trading
    if group not in ("test", "getting-started"):
        raise PluginOriginError(f"Unknown plugin group: {group}")
    test_path, _ = pinned_lock_path(system, machine, "test")
    pins = read_pins(test_path, target, TEST_VERSIONS)
    if group == "getting-started":
        required = {"source/postgres", "bootstrap/postgres", "reaction/log"}
        return {
            reference: pin for reference, pin in {**trading, **pins}.items()
            if reference.split(":")[0] in required
        }
    pins.update(
        (reference, pin) for reference, pin in trading.items()
        if reference.split(":")[0] == "bootstrap/scriptfile"
    )
    return pins


def verify_binary(path, expected):
    if path.is_symlink() or not path.is_file():
        raise PluginOriginError(f"Missing or non-regular pinned plugin: {path}")
    with path.open("rb") as binary:
        actual = hashlib.file_digest(binary, "sha256").hexdigest()
    if actual != expected:
        raise PluginOriginError(
            f"Pinned plugin hash mismatch: {path}. Refusing to use or overwrite it."
        )


def validate_loaded_plugins(plugins, pins):
    loaded = {plugin["id"]: plugin for plugin in plugins}
    if len(loaded) != len(plugins):
        raise PluginOriginError("Duplicate plugin identities in actual load metadata")
    for reference, pin in pins.items():
        kind_id = reference.split(":")[0]
        category, kind = kind_id.split("/")
        plugin = loaded.get(kind_id)
        if plugin is None:
            raise PluginOriginError(f"Required plugin was not loaded: {kind_id}")
        if (
            plugin["sdkVersion"] != PLUGIN_ABI_VERSION
            or plugin["pluginVersion"] != pin["version"]
            or plugin["fileHash"] != pin["file_hash"]
            or plugin["status"] not in ("Loaded", "Active")
            or not any(
                entry["category"].lower() == category and entry["kind"] == kind
                for entry in plugin["kinds"]
            )
        ):
            raise PluginOriginError(f"Incompatible actual plugin load metadata: {kind_id}")


def pin_toml(reference, pin):
    lines = [f"\n[plugins.{json.dumps(reference)}]"]
    for key, value in pin.items():
        if key != "signature":
            if not isinstance(value, str):
                raise PluginOriginError(f"Invalid {reference} lock field: {key}")
            lines.append(f"{key} = {json.dumps(value)}")
    lines.extend([
        f"\n[plugins.{json.dumps(reference)}.signature]",
        "verified = true",
        f"issuer = {json.dumps(pin['signature']['issuer'])}",
        f"subject = {json.dumps(pin['signature']['subject'])}",
    ])
    return "\n".join(lines) + "\n"


def prepare_lock(directory, pins):
    directory.mkdir(parents=True, exist_ok=True)
    lock_path = directory / "plugins.lock"
    if lock_path.is_symlink():
        raise PluginOriginError(f"Refusing to replace a symlink: {lock_path}")
    existed = lock_path.exists()
    original = lock_path.read_text() if existed else "version = 1\n"
    existing = tomllib.loads(original)
    if existing.get("version") != 1:
        raise PluginOriginError("Unsupported existing plugin lock format")
    entries = existing.get("plugins", {})
    additions = []
    for reference, pin in pins.items():
        if reference in entries and entries[reference] != pin:
            raise PluginOriginError(f"Existing plugin pin conflicts with required pin: {reference}")
        for other_reference, entry in entries.items():
            if entry["filename"] == pin["filename"] and entry != pin:
                raise PluginOriginError(f"Existing plugin pin conflicts with required pin: {other_reference}")
        binary = directory / pin["filename"]
        if binary.exists() or binary.is_symlink():
            verify_binary(binary, pin["file_hash"])
        if reference not in entries:
            additions.append(pin_toml(reference, pin))
    if not additions and lock_path.exists():
        return
    merged = original.rstrip() + "\n" + "".join(additions)
    if tomllib.loads(merged).get("plugins", {}) != {**entries, **pins}:
        raise PluginOriginError("Merged plugin lock does not preserve existing entries")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", dir=directory, prefix=".plugins-lock-", delete=False,
        ) as output:
            temporary = Path(output.name)
            output.write(merged)
        if (
            lock_path.is_symlink()
            or lock_path.exists() != existed
            or (existed and lock_path.read_text() != original)
        ):
            raise PluginOriginError("Plugin lock changed during preparation; retry without overwriting it")
        os.replace(temporary, lock_path)
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink()


def install(server, directory, pins, run=subprocess.run):
    prepare_lock(directory, pins)
    config = {
        "apiVersion": "drasi.io/v1",
        "verifyPlugins": True,
        "autoInstallPlugins": False,
        "plugins": [{"ref": reference} for reference in pins],
    }
    with tempfile.TemporaryDirectory(prefix="drasi-plugin-install-") as temporary:
        config_path = Path(temporary) / "install.json"
        config_path.write_text(json.dumps(config) + "\n")
        run([
            str(server), "--config", str(config_path),
            "--plugins-dir", str(directory),
            "plugin", "install", "--from-config", "--locked",
        ], check=True)
    # The existing CLI can return zero after reporting a per-plugin failure.
    for pin in pins.values():
        verify_binary(directory / pin["filename"], pin["file_hash"])
    actual = tomllib.loads((directory / "plugins.lock").read_text())["plugins"]
    for reference, pin in pins.items():
        if actual.get(reference) != pin:
            raise PluginOriginError(f"Installer changed the reviewed plugin pin: {reference}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--group", choices=("trading", "test", "getting-started"), default="trading",
    )
    parser.add_argument("--server-bin", type=Path, required=True)
    parser.add_argument("--plugins-dir", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        mode, _ = resolve_origin()
        if mode != "registry":
            raise PluginOriginError("Signed registry pins cannot replace matching local SDK plugins")
        pins = group_pins(arguments.group, platform.system(), platform.machine())
        install(arguments.server_bin.resolve(), arguments.plugins_dir.resolve(), pins)
        print(f"The {len(pins)} pinned {arguments.group} plugins are ready; signature policy remains enabled.")
    except (PluginOriginError, subprocess.CalledProcessError, OSError, ValueError, KeyError) as error:
        print(f"Plugin preparation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
