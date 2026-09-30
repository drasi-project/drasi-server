#!/usr/bin/env python3
"""Stage current checkout sources, including uncommitted fixes, for the Linux build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

EXAMPLE = Path(__file__).resolve().parents[1]
LAB = EXAMPLE.parent
WORKSPACE = LAB.parents[2]
DESTINATION = EXAMPLE / ".build" / "runtime-src"
EXCLUDED = {".git", ".build", "target", "node_modules", "vendor", "dist", "__pycache__"}
EXTENSIONS = {".rs", ".toml", ".lock", ".md", ".json", ".yaml", ".yml", ".proto",
              ".sql", ".rego", ".cypher", ".txt", ".tsv", ".h", ".c", ".sh", ".html",
              ".css", ".js", ".ts", ".tsx", ".svg"}


def is_source_file(path):
    return (
        path.suffix in EXTENSIONS
        or path.name in {"LICENSE", "NOTICE"}
        or path.name.startswith(("LICENSE-", "NOTICE-"))
    )


def copy_file(source, target, manifest, staging):
    if source.is_symlink():
        raise RuntimeError(f"Refusing to follow source symlink: {source}")
    data = source.read_bytes()
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    target.chmod(source.stat().st_mode & 0o777)
    manifest[str(target.relative_to(staging))] = hashlib.sha256(data).hexdigest()


def hosting_paths(hosting):
    if hosting not in ("embedded", "server"):
        raise ValueError(f"Unknown hosting layout: {hosting}")
    example = LAB / hosting
    return example, example / ".build" / "runtime-src"


def main(hosting="embedded"):
    example, destination = hosting_paths(hosting)
    example.joinpath(".build").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="runtime-export-", dir=example / ".build") as temporary:
        staging = Path(temporary)
        hashes = {}
        revisions = {}
        for name in ("drasi-core", "drasi-server"):
            repository = WORKSPACE / name
            revisions[name] = subprocess.check_output(
                ["git", "-C", str(repository), "rev-parse", "HEAD"], text=True).strip()
            files = subprocess.check_output([
                "git", "-C", str(repository), "ls-files", "--cached", "--others",
                "--exclude-standard", "-z",
            ]).split(b"\0")
            for raw in files:
                if not raw:
                    continue
                relative = Path(os.fsdecode(raw))
                if (set(relative.parts) & EXCLUDED or relative.name.startswith(".env")
                        or relative.parts[:2] == ("examples", "gpu-cluster-lab")):
                    continue
                source = repository / relative
                if not source.exists() or not is_source_file(source):
                    continue
                copy_file(source, staging / name / relative, hashes, staging)
            if hosting == "server" and name == "drasi-core":
                lock = repository / "Cargo.lock"
                if not lock.is_file():
                    raise RuntimeError(
                        "Stock plugin builds require drasi-core/Cargo.lock. "
                        "Run cargo generate-lockfile --manifest-path drasi-core/Cargo.toml first."
                    )
                copy_file(lock, staging / name / "Cargo.lock", hashes, staging)
        example_target = staging / "drasi-server" / "examples" / "gpu-cluster-lab"
        directories = ["shared/crates", "shared/queries", "shared/policies",
                       "shared/migrations", "embedded/src", "embedded/control"]
        if hosting == "server":
            directories.extend(("server/src", "shared/ui"))
        for directory in directories:
            for source in (LAB / directory).rglob("*"):
                relative = source.relative_to(LAB)
                if (source.is_file() and not set(relative.parts) & EXCLUDED
                        and not source.name.startswith(".env") and is_source_file(source)):
                    copy_file(source, example_target / relative, hashes, staging)
        manifests = ["shared/Cargo.toml", "shared/Cargo.lock", "rust-toolchain.toml"]
        if hosting == "server":
            manifests.extend(name for name in ("server/Cargo.toml", "server/Cargo.lock")
                             if (LAB / name).is_file())
        for name in manifests:
            copy_file(LAB / name, example_target / name, hashes, staging)
        (staging / "source-manifest.json").write_text(
            json.dumps({"heads": revisions, "files": hashes}, sort_keys=True, indent=2) + "\n")
        if destination.exists():
            if not (destination / "source-manifest.json").is_file():
                raise RuntimeError(f"Refusing to replace an unrecognized source directory: {destination}")
            shutil.rmtree(destination)
        shutil.copytree(staging, destination)
        print(f"Staged {len(hashes)} current source files in {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hosting", choices=("embedded", "server"), default="embedded")
    main(parser.parse_args().hosting)
