#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

"""Build/check this demo's trusted local bundle; never adopt arbitrary binaries."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / "target/observability-example"
SERVER_TARGET = ROOT / "target/observability-server"
SERVER = SERVER_TARGET / "debug/drasi-server"
RECEIPT_FORMAT = 2
# Query bootstrap is disabled in both scenarios, so no bootstrap library is bundled.
SCENARIOS = {
    "http": ("drasi-source-http", "drasi-computation-transformer-query", "drasi-reaction-log"),
    "postgres": (
        "drasi-source-postgres", "drasi-computation-transformer-query", "drasi-reaction-log",
    ),
}
REBUILD = "Run: bash examples/observability/demo.sh build-plugins"


def crates(scenario):
    try:
        return SCENARIOS[scenario]
    except KeyError:
        raise ValueError(f"Unknown observability scenario: {scenario}") from None


def plugin_names(scenario):
    if sys.platform == "darwin":
        prefix, suffix = "lib", ".dylib"
    elif sys.platform.startswith("linux"):
        prefix, suffix = "lib", ".so"
    else:
        raise ValueError("This local walkthrough supports macOS/Linux only.")
    return tuple(prefix + name.replace("-", "_") + suffix for name in crates(scenario))


def bundle_dir(scenario):
    """Example-owned copies; never a live or shared plugin directory."""
    crates(scenario)
    return STATE / "bundles" / scenario


def manifest_path(scenario):
    crates(scenario)
    return STATE / f"{scenario}-build.json"


def rebuild(scenario):
    return REBUILD if scenario == "http" else f"{REBUILD} {scenario}"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_fingerprint(root):
    files = subprocess.check_output([
        "git", "-C", str(root), "ls-files", "-z", "--cached", "--others",
        "--exclude-standard",
    ]).split(b"\0")
    digest = hashlib.sha256()
    for name in sorted(set(filter(None, files))):
        path = root / name.decode()
        # Include dirty/untracked source, locks, scripts and build-time assets,
        # but not documentation-only changes or the ignored build/run outputs.
        if path.suffix == ".md":
            continue
        value = sha256(path) if path.is_file() else "missing"
        digest.update(len(name).to_bytes(8, "big") + name + value.encode())
    return digest.hexdigest()


def bundle_state(root, core, server, plugins, toolchain, scenario):
    names = plugin_names(scenario)
    paths = [server, *(plugins / name for name in names)]
    if any(not path.is_file() or path.is_symlink() for path in paths):
        raise ValueError(f"Missing/non-regular local bundle binary. {rebuild(scenario)}")
    return {
        "format": RECEIPT_FORMAT,
        "scenario": scenario,
        "plugins": list(names),
        "serverRoot": str(root.resolve()),
        "coreRoot": str(core.resolve()),
        "serverSources": source_fingerprint(root),
        "coreSources": source_fingerprint(core),
        "toolchain": toolchain,
        "binaries": {str(path.resolve()): sha256(path) for path in paths},
    }


def verify_bundle(manifest, current):
    scenario = current["scenario"]
    try:
        recorded = json.loads(manifest.read_text())
    except (OSError, ValueError) as error:
        raise ValueError(f"No valid trusted local build receipt. {rebuild(scenario)}") from error
    if recorded.get("format") != RECEIPT_FORMAT or recorded.get("scenario") != scenario:
        raise ValueError(f"Receipt is for another format or scenario. {rebuild(scenario)}")
    if recorded != current:
        raise ValueError(f"Local sources/toolchain/binaries changed since build. {rebuild(scenario)}")


def create_run(parent, plugins, scenario):
    parent.mkdir(parents=True, exist_ok=True)
    run = parent / ("run-" + secrets.token_hex(12))
    run.mkdir(mode=0o700)
    (run / "plugins").mkdir()
    for name in plugin_names(scenario):
        shutil.copyfile(plugins / name, run / "plugins" / name)
        if sha256(run / "plugins" / name) != sha256(plugins / name):
            raise ValueError("Plugin changed while copying; rebuild the local bundle.")
    (run / "demo-owner.json").write_text(json.dumps({
        "owner": "drasi-server/examples/observability",
        "instance": run.name,
        "scenario": scenario,
        "note": "Unique demo WAL directory; launcher never deletes this state.",
    }) + "\n")
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "prepare"))
    parser.add_argument("scenario", nargs="?", default="http", choices=tuple(SCENARIOS))
    args = parser.parse_args()
    scenario = args.scenario
    plugins = bundle_dir(scenario)
    manifest = manifest_path(scenario)
    try:
        os.environ["CARGO_NET_OFFLINE"] = "true"
        core = Path(subprocess.check_output([
            sys.executable, str(ROOT / "scripts/plugin_origin.py"), "local-workspace",
        ], cwd=ROOT, text=True).strip())
        toolchain = subprocess.check_output(["rustc", "-vV"], text=True)
        if args.action == "build":
            before = (source_fingerprint(ROOT), source_fingerprint(core))
            subprocess.run([
                "cargo", "build", "--locked", "--bin", "drasi-server",
                "--target-dir", str(SERVER_TARGET),
            ], cwd=ROOT, check=True, stdout=sys.stderr)
            # Keep Core's locked graph separate from Server's locked graph.
            target = ROOT / "target/observability-plugins"
            command = [
                "cargo", "build", "--locked", "--lib", "--target-dir", str(target),
            ]
            for crate in crates(scenario):
                command.extend(["-p", crate])
            command.extend([
                "--features", ",".join(crate + "/dynamic-plugin" for crate in crates(scenario)),
            ])
            subprocess.run(command, cwd=core, check=True, stdout=sys.stderr)
            plugins.mkdir(parents=True, exist_ok=True)
            for name in plugin_names(scenario):
                destination = plugins / name
                if destination.is_symlink():
                    raise ValueError("Refusing to overwrite a symlink in the example bundle.")
                shutil.copyfile(target / "debug" / name, destination)
            state = bundle_state(ROOT, core, SERVER, plugins, toolchain, scenario)
            if before != (state["serverSources"], state["coreSources"]):
                raise ValueError("Sources changed during build; rerun build-plugins.")
            STATE.mkdir(parents=True, exist_ok=True)
            manifest.write_text(json.dumps(state, indent=2) + "\n")
            print(f"Matched local {scenario} bundle built; no registry downloads or tests run.")
        else:
            current = bundle_state(ROOT, core, SERVER, plugins, toolchain, scenario)
            verify_bundle(manifest, current)
            run = create_run(STATE / "runs", plugins, scenario)
            # Detect replacement while preparing, before the host opens libraries.
            verify_bundle(manifest, bundle_state(ROOT, core, SERVER, plugins, toolchain, scenario))
            for name in plugin_names(scenario):
                if sha256(run / "plugins" / name) != current["binaries"][str((plugins / name).resolve())]:
                    raise ValueError("Copied plugin does not match trusted build receipt.")
            print(run)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Observability local bundle: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
