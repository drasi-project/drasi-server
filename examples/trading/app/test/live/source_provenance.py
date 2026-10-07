# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

"""Verify the built server's released dependencies using the shared registry policy."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import platform
import subprocess
import sys
import tarfile
import tomllib
from urllib.request import urlopen


RELEASE_REVISION = "22125bf1d66062533b832a166fe4a51079a23d6e"
PARSER_REVISION = "8f0ed49802ab0f2d62ce834aafe7fe7e5861ee76"
PARSERS = {"drasi-query-ast", "drasi-query-cypher", "drasi-query-gql"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def selected_registry(metadata, manifest, lock, policy):
    if manifest.get("patch") or manifest.get("replace"):
        raise ValueError("The published live gate does not allow source overrides")
    mode, _ = policy.classify(metadata)
    if mode != "registry":
        raise ValueError("The published live gate requires registry dependencies")
    policy.validate_registry_lock(lock)
    return policy.selected_packages(metadata, policy.REGISTRY_PACKAGES)


def archive_url(package):
    name, version = package["name"], package["version"]
    return f"https://static.crates.io/crates/{name}/{name}-{version}.crate"


def read_archive(package):
    source = Path(package["manifest_path"]).resolve().parent
    # Cargo's cached .crate is still checked against the official immutable hash.
    cache = source.parent.parent.parent / "cache" / source.parent.name / f"{source.name}.crate"
    if cache.is_file():
        return cache.read_bytes()
    with urlopen(archive_url(package), timeout=30) as response:
        return response.read()


def verify_archive(package, checksum, data):
    actual = hashlib.sha256(data).hexdigest()
    if actual != checksum:
        raise ValueError(f"Unverified published archive checksum: {package['name']}")
    source = Path(package["manifest_path"]).resolve().parent
    prefix = f"{package['name']}-{package['version']}"
    files = {}
    contents = {}
    vcs = None
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != prefix:
                raise ValueError(f"Invalid published archive path: {member.name}")
            if member.isdir():
                continue
            relative = PurePosixPath(*path.parts[1:]).as_posix()
            if not member.isfile() or relative in files or len(path.parts) < 2:
                raise ValueError(f"Invalid or duplicate published archive entry: {member.name}")
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError(f"Missing published archive file: {member.name}")
            content = stream.read()
            contents[relative] = content
            files[relative] = hashlib.sha256(content).hexdigest()
            if relative == ".cargo_vcs_info.json":
                vcs = json.loads(content)
    expected_revision = PARSER_REVISION if package["name"] in PARSERS else RELEASE_REVISION
    if not vcs or vcs.get("git", {}).get("sha1") != expected_revision or not vcs.get("path_in_vcs"):
        raise ValueError(f"Unexpected published archive VCS identity: {package['name']}")
    aliases = {}
    collisions = []
    readmes = ("README.md", "readme.md")
    if package["name"] == "drasi-middleware" and all(name in files for name in readmes):
        upper, lower = (source / name for name in readmes)
        if upper.is_symlink() or lower.is_symlink():
            raise ValueError("Published README source must not be a symlink")
        if upper.samefile(lower):
            spellings = [path.name for path in source.iterdir() if path.name in readmes]
            if len(spellings) != 1 or upper.stat().st_nlink != 1 or lower.stat().st_nlink != 1:
                raise ValueError("Ambiguous hard-linked published README source")
            # The published archive has this documentation-only case collision.
            # Cargo keeps the later entry only when the filesystem aliases them.
            retained = [name for name in files if name in readmes][-1]
            aliases = {name: retained for name in readmes}
            collisions.append({
                "archiveHashes": {name: files[name] for name in readmes},
                "retainedArchivePath": retained,
                "directorySpelling": spellings[0],
                "filesystemAliasVerified": True,
            })
    identities = {}
    for relative in files:
        resolved_file = source / relative
        identity = resolved_file.stat()
        if resolved_file.is_symlink() or identity.st_nlink != 1:
            raise ValueError(f"Linked published source file: {package['name']}/{relative}")
        key = (identity.st_dev, identity.st_ino)
        previous = identities.get(key)
        if previous is not None and (
            relative not in aliases or previous not in aliases
            or aliases[relative] != aliases[previous]
        ):
            raise ValueError(f"Unexpected published source file alias: {package['name']}/{relative}")
        identities[key] = relative
        if resolved_file.read_bytes() != contents[aliases.get(relative, relative)]:
            raise ValueError(f"Resolved source differs from published archive: {package['name']}/{relative}")
    return {
        "url": archive_url(package),
        "sha256": actual,
        "vcs": vcs,
        "resolvedSourceMatchesArchive": True,
        "archiveFileCount": len(files),
        "verifiedFileCount": len({aliases.get(name, name) for name in files}),
        "documentationCaseCollisions": collisions,
        "fileManifestSha256": hashlib.sha256(
            json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        "rustSourceSha256": {name: value for name, value in files.items() if name.endswith(".rs")},
    }


def published_provenance(metadata, manifest, lock, policy):
    selected = selected_registry(metadata, manifest, lock, policy)
    return {
        name: {
            **{key: package[key] for key in ("version", "source", "manifest_path")},
            "archive": verify_archive(package, policy.REGISTRY_PACKAGES[name][1], read_archive(package)),
        }
        for name, package in selected.items()
    }


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
        revision = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "--verify", "HEAD^{commit}"],
            text=True,
        ).strip()
        if revision != arguments.revision:
            raise ValueError("Caller server revision differs from the checked-out source")
        if arguments.cargo_lock.read_bytes() != (root / "Cargo.lock").read_bytes():
            raise ValueError("Caller dependency lock differs from the checked-out server")
        metadata = plugin_origin.cargo_metadata(root / "Cargo.toml")
        selected = published_provenance(
            metadata,
            tomllib.loads((root / "Cargo.toml").read_text()),
            tomllib.loads((root / "Cargo.lock").read_text()),
            plugin_origin,
        )
        approved_lock, target = install_plugins.pinned_lock_path(
            platform.system(), platform.machine(),
        )
        if arguments.plugin_lock.read_bytes() != approved_lock.read_bytes():
            raise ValueError("Caller Trading plugin lock differs from the shared reviewed pins")
        pins = install_plugins.read_pins(approved_lock, target)
        print(json.dumps({
            "classification": "checked-out server with verified published registry archives and resolved source contents",
            "serverRevision": revision,
            "sourceRoot": str(root),
            "dependencyOrigin": "published-registry",
            "publishedArchivesVerified": True,
            "serverManifestSha256": digest(root / "Cargo.toml"),
            "serverCargoLockSha256": digest(root / "Cargo.lock"),
            "pluginOrigin": "registry",
            "pluginLockSha256": digest(approved_lock),
            "selectedDependencies": selected,
            "pluginPins": pins,
        }))
    except (plugin_origin.PluginOriginError, subprocess.CalledProcessError, OSError, ValueError, KeyError, tarfile.TarError) as error:
        print(f"Live source verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
