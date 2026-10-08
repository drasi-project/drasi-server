#!/usr/bin/env python3
"""Stage current checkout sources, including uncommitted fixes, for the Linux build."""
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
    shutil.copystat(source, target, follow_symlinks=False)
    manifest[str(target.relative_to(staging))] = hashlib.sha256(data).hexdigest()


def main():
    EXAMPLE.joinpath(".build").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="runtime-export-", dir=EXAMPLE / ".build") as temporary:
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
            # Core is a library workspace and ignores its generated lockfile.
            # Preserve it when present so native plugin builds use the same resolution.
            lock = repository / "Cargo.lock"
            if name == "drasi-core" and lock.is_file():
                copy_file(lock, staging / name / "Cargo.lock", hashes, staging)
        example_target = staging / "drasi-server" / "examples" / "gpu-cluster-lab"
        for directory in ("shared/crates", "shared/queries", "shared/policies",
                          "shared/migrations", "embedded/src", "embedded/control"):
            for source in (LAB / directory).rglob("*"):
                relative = source.relative_to(LAB)
                if (source.is_file() and not set(relative.parts) & EXCLUDED
                        and not source.name.startswith(".env") and is_source_file(source)):
                    copy_file(source, example_target / relative, hashes, staging)
        for name in ("shared/Cargo.toml", "shared/Cargo.lock", "rust-toolchain.toml"):
            copy_file(LAB / name, example_target / name, hashes, staging)
        (staging / "source-manifest.json").write_text(
            json.dumps({"heads": revisions, "files": hashes}, sort_keys=True, indent=2) + "\n")
        if DESTINATION.exists():
            if not (DESTINATION / "source-manifest.json").is_file():
                raise RuntimeError(f"Refusing to replace an unrecognized source directory: {DESTINATION}")
            shutil.rmtree(DESTINATION)
        shutil.copytree(staging, DESTINATION)
        print(f"Staged {len(hashes)} current source files in {DESTINATION}")


if __name__ == "__main__":
    main()
