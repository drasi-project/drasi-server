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
PLUGINS = ROOT / "target/debug/plugins"
MANIFEST = STATE / "local-build.json"
CRATES = (
    "drasi-source-mock", "drasi-computation-transformer-query", "drasi-reaction-log",
)
REBUILD = "Run: bash examples/observability/demo.sh build-plugins"


def plugin_names():
    if sys.platform == "darwin":
        prefix, suffix = "lib", ".dylib"
    elif sys.platform.startswith("linux"):
        prefix, suffix = "lib", ".so"
    else:
        raise ValueError("This local walkthrough supports macOS/Linux only.")
    return tuple(prefix + name.replace("-", "_") + suffix for name in CRATES)


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


def bundle_state(root, core, server, plugins, toolchain):
    paths = [server, *(plugins / name for name in plugin_names())]
    if any(not path.is_file() or path.is_symlink() for path in paths):
        raise ValueError(f"Missing/non-regular local bundle binary. {REBUILD}")
    return {
        "format": 1,
        "serverRoot": str(root.resolve()),
        "coreRoot": str(core.resolve()),
        "serverSources": source_fingerprint(root),
        "coreSources": source_fingerprint(core),
        "toolchain": toolchain,
        "binaries": {str(path.resolve()): sha256(path) for path in paths},
    }


def verify_bundle(manifest, current):
    try:
        recorded = json.loads(manifest.read_text())
    except (OSError, ValueError) as error:
        raise ValueError(f"No valid trusted local build receipt. {REBUILD}") from error
    if recorded != current:
        raise ValueError(f"Local sources/toolchain/binaries changed since build. {REBUILD}")


def create_run(parent, plugins):
    parent.mkdir(parents=True, exist_ok=True)
    run = parent / ("run-" + secrets.token_hex(12))
    run.mkdir(mode=0o700)
    (run / "plugins").mkdir()
    for name in plugin_names():
        shutil.copyfile(plugins / name, run / "plugins" / name)
        if sha256(run / "plugins" / name) != sha256(plugins / name):
            raise ValueError("Plugin changed while copying; rebuild the local bundle.")
    (run / "demo-owner.json").write_text(json.dumps({
        "owner": "drasi-server/examples/observability",
        "instance": run.name,
        "note": "Unique demo WAL directory; launcher never deletes this state.",
    }) + "\n")
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("build", "prepare"))
    args = parser.parse_args()
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
            for crate in CRATES:
                command.extend(["-p", crate])
            command.extend(["--features", ",".join(crate + "/dynamic-plugin" for crate in CRATES)])
            subprocess.run(command, cwd=core, check=True, stdout=sys.stderr)
            PLUGINS.mkdir(parents=True, exist_ok=True)
            for name in plugin_names():
                destination = PLUGINS / name
                if destination.is_symlink():
                    raise ValueError("Refusing to overwrite a symlink in target/debug/plugins.")
                shutil.copyfile(target / "debug" / name, destination)
            state = bundle_state(ROOT, core, SERVER, PLUGINS, toolchain)
            if before != (state["serverSources"], state["coreSources"]):
                raise ValueError("Sources changed during build; rerun build-plugins.")
            STATE.mkdir(parents=True, exist_ok=True)
            MANIFEST.write_text(json.dumps(state, indent=2) + "\n")
            print("Matched local bundle built; no registry downloads or tests run.")
        else:
            current = bundle_state(ROOT, core, SERVER, PLUGINS, toolchain)
            verify_bundle(MANIFEST, current)
            run = create_run(STATE / "runs", PLUGINS)
            # Detect replacement while preparing, before the host opens libraries.
            verify_bundle(MANIFEST, bundle_state(ROOT, core, SERVER, PLUGINS, toolchain))
            for name in plugin_names():
                if sha256(run / "plugins" / name) != current["binaries"][str((PLUGINS / name).resolve())]:
                    raise ValueError("Copied plugin does not match trusted build receipt.")
            print(run)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Observability local bundle: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
