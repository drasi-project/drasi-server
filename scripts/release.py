#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

"""Candidate identity, asset validation, and fail-closed release promotion."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tomllib
import zipfile

from plugin_origin import PLUGIN_ABI_VERSION


TARGETS = (
    "aarch64-apple-darwin", "x86_64-apple-darwin",
    "aarch64-linux-gnu", "x86_64-linux-gnu",
    "aarch64-linux-musl", "x86_64-linux-musl", "x86_64-windows-msvc",
)
ARCHES = ("amd64", "arm64")
REQUIRED_JOBS = {
    "determine-version", "validate-candidate", "build-linux", "build-macos",
    "build-windows-msvc", "build-sse-cli-linux", "build-sse-cli-macos",
    "build-sse-cli-windows", "build-docker", "test-binaries",
    "test-containers", "assemble-assets",
}
IDENTITY_FIELDS = ("sha", "kind", "run_id", "run_attempt")


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity(candidate):
    return {key: candidate[key] for key in IDENTITY_FIELDS}


def load_candidate(path):
    data = read_json(path)
    require(data["sha"] == run("git", "rev-parse", "HEAD"), "Checkout differs from candidate SHA")
    require(data["run_id"] == os.environ["GITHUB_RUN_ID"]
            and data["run_attempt"] == os.environ["GITHUB_RUN_ATTEMPT"],
            "Candidate belongs to a different workflow run/attempt")
    require(data["lockfiles"] == {
        name: sha256(name) for name in ("Cargo.lock", "examples/sse-cli/Cargo.lock")
    }, "Candidate dependency lockfiles changed")
    return data


def binary_name(target, cli=False):
    if target == "x86_64-windows-msvc":
        return "drasi-sse-cli-x86_64-windows.exe" if cli else f"drasi-server-{target}.exe"
    return f"{'drasi-sse-cli' if cli else 'drasi-server'}-{target}"


def expected_assets():
    return {binary_name(t, cli) for t in TARGETS for cli in (False, True)} | {
        "drasi-server-examples.zip",
    }


def candidate(args):
    sha = run("git", "rev-parse", "HEAD")
    require(re.fullmatch(r"[0-9a-f]{40}", sha), "Expected a committed candidate SHA")
    require(not run("git", "status", "--porcelain", "--untracked-files=no"),
            "Candidate must have no uncommitted tracked changes")
    require(args.kind in ("release", "locked", "upgrade"), "Unknown candidate kind")
    version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
    tag = args.tag or version
    require(re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}", tag) and tag != "latest",
            "Version tag must be a Docker-compatible tag other than latest")
    locks = ("Cargo.lock", "examples/sse-cli/Cargo.lock")
    dependencies = [
        {k: p[k] for k in ("name", "version", "source", "checksum") if k in p}
        for p in tomllib.loads(Path("Cargo.lock").read_text())["package"]
        if p["name"].startswith("drasi-")
    ]
    data = {
        "sha": sha, "base_sha": args.base_sha or sha, "kind": args.kind,
        "run_id": os.environ["GITHUB_RUN_ID"],
        "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
        "version": version, "tag": tag,
        "lockfiles": {name: sha256(name) for name in locks},
        "drasi_dependencies": dependencies, "reviewed_plugin_abi": PLUGIN_ABI_VERSION,
    }
    write_json(args.output, data)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        for key in ("sha", "kind", "tag"):
            output.write(f"{key}={data[key]}\n")
        output.write(f"stage_tag=run-{data['run_id']}-{data['run_attempt']}-{args.kind}-{sha[:12]}\n")


def example_files():
    files = {}
    for entry in run("git", "ls-tree", "-rlz", "HEAD", "--", "examples/").split("\0"):
        if not entry:
            continue
        metadata, name = entry.split("\t", 1)
        mode, kind, _, size = metadata.split()
        require(kind == "blob" and mode in ("100644", "100755"),
                f"Unsupported examples entry: {name}")
        require(not {"target", "node_modules", ".git"}.intersection(PurePosixPath(name).parts),
                f"Build output in examples: {name}")
        files[name] = int(size)
    require("examples/README.md" in files and files["examples/README.md"] > 0,
            "Examples README is missing or empty")
    return files


def validate_assets(directory):
    directory = Path(directory)
    require({p.name for p in directory.iterdir()} == expected_assets(),
            "Release assets must contain exactly seven server binaries, seven CLI binaries and examples")
    for path in directory.iterdir():
        require(path.is_file() and not path.is_symlink() and path.stat().st_size > 0,
                f"Missing, empty or non-regular asset: {path.name}")
    expected = example_files()
    with zipfile.ZipFile(directory / "drasi-server-examples.zip") as archive:
        require(archive.testzip() is None, "Corrupt examples archive")
        names = archive.namelist()
        require(len(names) == len(set(names)), "Duplicate examples archive entries")
        for name in names:
            path = PurePosixPath(name)
            require(not path.is_absolute() and ".." not in path.parts and "\\" not in name
                    and path.parts[0] == "examples", f"Unsafe archive entry: {name}")
        actual = {i.filename: i.file_size for i in archive.infolist() if not i.is_dir()}
        require(actual == expected, "Examples archive does not match committed examples")
    return {p.name: sha256(p) for p in directory.iterdir()}


def assets(args):
    destination = Path(args.directory)
    destination.mkdir()
    names = expected_assets() - {"drasi-server-examples.zip"}
    files = [p for p in Path(args.artifacts).rglob("*") if p.is_file()]
    require(len(files) == len(names) and {p.name for p in files} == names,
            "Missing, unexpected or duplicate binary artifacts")
    for path in files:
        shutil.copyfile(path, destination / path.name)
    subprocess.run(["git", "archive", "--format=zip",
                    f"--output={destination / 'drasi-server-examples.zip'}",
                    "HEAD", "examples/"], check=True)
    data = load_candidate(args.candidate)
    write_json(args.output, {
        **identity(data), "type": "assets", "assets": validate_assets(destination),
    })


def record(args):
    data = load_candidate(args.candidate)
    runtime = read_json(args.runtime)
    validate_runtime(data, runtime)
    evidence = {**identity(data), "type": args.type, "target": args.target, "runtime": runtime}
    if args.type == "binary":
        require(args.target in TARGETS and args.binary, "Invalid binary target")
        evidence["binary_sha256"] = sha256(args.binary)
    else:
        require(args.target in ARCHES and args.image and args.default_runtime,
                "Container evidence requires image digest and default-startup results")
        image, separator, digest = args.image.partition("@")
        require(separator and re.fullmatch(r"sha256:[0-9a-f]{64}", digest),
                "Container must be tested by digest")
        default_runtime = read_json(args.default_runtime)
        validate_runtime(data, default_runtime, default_startup=True)
        evidence.update(image=image, digest=digest, default_runtime=default_runtime)
        evidence["manifest"] = inspect_manifest(args.image)
        image_descriptors(evidence)
    write_json(args.output, evidence)


def validate_runtime(candidate, runtime, default_startup=False):
    expected = (["health", "default-components", "plugins"] if default_startup
                else ["health", "source", "query", "results", "plugins"])
    require(runtime["checks"] == expected, "Incomplete runtime assertions")
    require(runtime["plugins"], "Loaded plugin identities are missing")
    for plugin in runtime["plugins"]:
        require(plugin["sdkVersion"] == candidate["reviewed_plugin_abi"],
                "Loaded plugin ABI differs from the reviewed candidate ABI")
        require(plugin["id"] and plugin["pluginVersion"]
                and re.fullmatch(r"[0-9a-f]{64}", plugin["fileHash"]),
                "Incomplete loaded plugin identity")


def image_descriptors(report):
    manifest = report["manifest"]
    require(manifest["digest"] == report["digest"], "Recorded image manifest digest mismatch")
    descriptors = manifest["manifests"]
    runtime = [entry for entry in descriptors if entry["platform"]["os"] == "linux"]
    require(len(runtime) == 1 and runtime[0]["platform"]["architecture"] == report["target"],
            "Staged image must contain exactly its tested architecture")
    attestations = [entry for entry in descriptors if entry not in runtime]
    require(len(attestations) == 1, "Expected the image's BuildKit provenance manifest")
    attestation = attestations[0]
    require(attestation["platform"] == {"architecture": "unknown", "os": "unknown"}
            and attestation["annotations"]["vnd.docker.reference.type"] == "attestation-manifest"
            and attestation["annotations"]["vnd.docker.reference.digest"] == runtime[0]["digest"],
            "Provenance does not reference the tested image manifest")
    return descriptors


def check_jobs(results):
    require(set(results) == REQUIRED_JOBS, "Missing or unexpected required release jobs")
    failures = {name: info.get("result") for name, info in results.items()
                if info.get("result") != "success"}
    require(not failures, f"Required release jobs did not succeed: {failures}")


def gate(args):
    check_jobs(json.loads(args.needs))
    data = load_candidate(args.candidate)
    reports = [read_json(path) for path in Path(args.evidence).glob("*.json")]
    expected = {("binary", t) for t in TARGETS} | {
        ("container", a) for a in ARCHES
    } | {("assets", None)}
    require(len(reports) == len(expected), "Missing or duplicate release evidence")
    by_key = {(r["type"], r.get("target")): r for r in reports}
    require(set(by_key) == expected, "Incomplete release evidence")
    for report in reports:
        require(identity(report) == identity(data), "Evidence belongs to a different candidate/run")
    asset_report = by_key[("assets", None)]
    require(asset_report["assets"] == validate_assets(args.assets),
            "Assets changed after assembly")
    for target in TARGETS:
        report = by_key[("binary", target)]
        require(report["binary_sha256"] == asset_report["assets"][binary_name(target)],
                f"Untested binary in release assets: {target}")
    containers = {arch: by_key[("container", arch)] for arch in ARCHES}
    for report in containers.values():
        require(report["image"] == args.staging_image, "Image is not from the dedicated staging package")
        require(re.fullmatch(r"sha256:[0-9a-f]{64}", report["digest"]), "Invalid image digest")
        validate_runtime(data, report["default_runtime"], default_startup=True)
        image_descriptors(report)
    for report in reports:
        if report["type"] != "assets":
            validate_runtime(data, report["runtime"])
    write_json(args.output, {
        **identity(data), "tag": data["tag"], "images": containers,
        "assets": asset_report["assets"], "jobs": json.loads(args.needs),
    })


def inspect_manifest(reference):
    return json.loads(run("docker", "buildx", "imagetools", "inspect",
                          "--format", "{{json .Manifest}}", reference))


def verify_assets(args):
    approved = read_json(args.approval)
    data = load_candidate(args.candidate)
    require(identity(approved) == identity(data) and data["kind"] == "release",
            "Approval is not for this production candidate")
    check_jobs(approved["jobs"])
    require(approved["assets"] == validate_assets(args.assets), "Approved release assets changed")


def promote(args):
    approved = read_json(args.approval)
    data = load_candidate(args.candidate)
    require(identity(approved) == identity(data), "Approval belongs to another candidate")
    require(data["kind"] == "release" and approved["tag"] == data["tag"],
            "Rehearsals cannot publish official images")
    require(re.fullmatch(r"[a-z0-9.-]+(?::[0-9]+)?/[a-z0-9_./-]+", args.image),
            "Invalid destination image")
    check_jobs(approved["jobs"])
    images = approved["images"]
    require(set(images) == set(ARCHES), "Both tested architectures are required")
    # Validate the entire approval before the first official registry write.
    for arch, report in images.items():
        require(identity(report) == identity(data) and report["target"] == arch,
                "Image approval identity mismatch")
        require(report["image"] == args.staging_image
                and re.fullmatch(r"sha256:[0-9a-f]{64}", report["digest"]),
                "Invalid staged image")
        image_descriptors(report)
    promoted = {}
    for arch in ARCHES:
        report = images[arch]
        destination = f"{args.image}:{data['tag']}-{arch}"
        subprocess.run(["docker", "buildx", "imagetools", "create", "--prefer-index=false",
                        "-t", destination, f"{report['image']}@{report['digest']}"], check=True)
        require(inspect_manifest(destination)["digest"] == report["digest"],
                "Promoted image digest differs from the tested image")
        promoted[arch] = report["digest"]
    references = [f"{args.image}@{promoted[arch]}" for arch in ARCHES]
    for tag in (data["tag"], "latest"):
        destination = f"{args.image}:{tag}"
        subprocess.run(["docker", "buildx", "imagetools", "create",
                        "-t", destination, *references], check=True)
        manifest = json.loads(run("docker", "buildx", "imagetools", "inspect", "--raw", destination))
        entries = manifest["manifests"]
        expected = [entry for arch in ARCHES for entry in image_descriptors(images[arch])]
        require(sorted(entries, key=lambda entry: entry["digest"])
                == sorted(expected, key=lambda entry: entry["digest"]),
                "Published multiarch index differs from the tested images")
        approved.setdefault("published", {})[tag] = inspect_manifest(destination)["digest"]
    write_json(args.output, approved)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("candidate")
    prepare.add_argument("--kind", required=True)
    prepare.add_argument("--tag", default="")
    prepare.add_argument("--base-sha", default="")
    assembly = commands.add_parser("assets")
    assembly.add_argument("--artifacts", required=True)
    assembly.add_argument("--directory", required=True)
    evidence = commands.add_parser("record")
    evidence.add_argument("--type", choices=("binary", "container"), required=True)
    evidence.add_argument("--target", required=True)
    evidence.add_argument("--runtime", required=True)
    evidence.add_argument("--binary")
    evidence.add_argument("--image")
    evidence.add_argument("--default-runtime")
    approval = commands.add_parser("gate")
    approval.add_argument("--needs", required=True)
    approval.add_argument("--evidence", required=True)
    approval.add_argument("--assets", required=True)
    approval.add_argument("--staging-image", required=True)
    publication = commands.add_parser("promote")
    publication.add_argument("--approval", required=True)
    publication.add_argument("--image", required=True)
    publication.add_argument("--staging-image", required=True)
    for command in (prepare, assembly, evidence, approval, publication):
        command.add_argument("--output", required=True)
        if command is not prepare:
            command.add_argument("--candidate", required=True)
    statuses = commands.add_parser("check-jobs")
    statuses.add_argument("--needs", required=True)
    verification = commands.add_parser("verify-assets")
    verification.add_argument("--candidate", required=True)
    verification.add_argument("--approval", required=True)
    verification.add_argument("--assets", required=True)
    args = parser.parse_args()
    if args.command == "check-jobs":
        check_jobs(json.loads(args.needs))
    else:
        {
            "candidate": candidate, "assets": assets, "record": record,
            "gate": gate, "promote": promote, "verify-assets": verify_assets,
        }[args.command](args)


if __name__ == "__main__":
    main()
