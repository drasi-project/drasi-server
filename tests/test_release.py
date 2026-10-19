# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile


ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import release

spec = importlib.util.spec_from_file_location(
    "assert_api", ROOT / "tests/integration/hello-world/assert-api.py")
assert_api = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assert_api)


def success_jobs():
    return {job: {"result": "success"} for job in release.REQUIRED_JOBS}


def image_manifest(arch):
    index = release.ARCHES.index(arch)
    digest = "sha256:" + "ab"[index] * 64
    return {
        "digest": "sha256:" + str(index + 1) * 64,
        "manifests": [
            {"digest": digest, "platform": {"os": "linux", "architecture": arch}},
            {"digest": "sha256:" + "cd"[index] * 64,
             "platform": {"os": "unknown", "architecture": "unknown"},
             "annotations": {
                 "vnd.docker.reference.type": "attestation-manifest",
                 "vnd.docker.reference.digest": digest,
             }},
        ],
    }


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        environment = patch.dict(os.environ, GITHUB_RUN_ID="123", GITHUB_RUN_ATTEMPT="1")
        environment.start()
        self.addCleanup(environment.stop)
        self.old_cwd = Path.cwd()
        os.chdir(self.root)
        self.addCleanup(os.chdir, self.old_cwd)
        subprocess.run(["git", "init", "-q"], check=True)
        (self.root / ".gitattributes").write_bytes((ROOT / ".gitattributes").read_bytes())
        (self.root / "examples").mkdir()
        (self.root / "examples/README.md").write_text("Example documentation\n")
        (self.root / "examples/config.yaml").write_text("sources: []\n")
        (self.root / "Cargo.toml").write_text('[package]\nname="drasi-server"\nversion="0.2.4"\n')
        (self.root / "Cargo.lock").write_text(
            'version=4\n[[package]]\nname="drasi-host-sdk"\nversion="0.11.3"\n', newline="\n")
        (self.root / "examples/sse-cli").mkdir()
        (self.root / "examples/sse-cli/Cargo.lock").write_text("version=4\n", newline="\n")
        subprocess.run(["git", "add", "."], check=True)
        subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.com",
                        "commit", "-qm", "Test fixture"], check=True)
        self.candidate = {
            "sha": release.run("git", "rev-parse", "HEAD"), "kind": "release",
            "run_id": "123", "run_attempt": "1", "tag": "0.2.4",
            "reviewed_plugin_abi": "0.14.0",
            "lockfiles": {name: release.sha256(name)
                          for name in ("Cargo.lock", "examples/sse-cli/Cargo.lock")},
        }
        release.write_json("candidate.json", self.candidate)
        self.artifacts = self.root / "artifacts"
        self.artifacts.mkdir()
        for target in release.TARGETS:
            for cli in (False, True):
                name = release.binary_name(target, cli)
                directory = self.artifacts / name
                directory.mkdir()
                (directory / name).write_text(f"binary fixture: {name}")
        self.evidence = self.root / "evidence"
        self.evidence.mkdir()
        self.asset_args = SimpleNamespace(
            artifacts=self.artifacts, directory=self.root / "assets",
            candidate="candidate.json", output=self.evidence / "assets.json")
        self.stage = "ghcr.io/test/drasi-server-test"
        self.gate_args = SimpleNamespace(
            needs=json.dumps(success_jobs()), candidate="candidate.json",
            evidence=self.evidence, assets=self.root / "assets",
            staging_image=self.stage, output="approval.json")

    def prepare_evidence(self):
        release.assets(self.asset_args)
        for target in release.TARGETS:
            release.write_json(self.evidence / f"binary-{target}.json", {
                **release.identity(self.candidate), "type": "binary", "target": target,
                "binary_sha256": release.sha256(self.root / "assets" / release.binary_name(target)),
                "runtime": self.runtime(),
            })
        for i, arch in enumerate(release.ARCHES, 1):
            release.write_json(self.evidence / f"container-{arch}.json", {
                **release.identity(self.candidate), "type": "container", "target": arch,
                "image": self.stage, "digest": "sha256:" + str(i) * 64,
                "runtime": self.runtime(), "default_runtime": self.runtime(default_startup=True),
                "manifest": image_manifest(arch),
            })

    @staticmethod
    def runtime(default_startup=False):
        return {
            "checks": (["health", "default-components", "plugins"] if default_startup
                       else ["health", "source", "query", "results", "plugins"]),
            "plugins": [{"sdkVersion": "0.14.0", "pluginVersion": "0.2.12",
                         "id": "source/mock", "fileHash": "e" * 64}],
        }

    def test_candidate_records_exact_commit_dependencies_and_both_lockfiles(self):
        args = SimpleNamespace(kind="upgrade", base_sha="a" * 40, tag="", output="identity.json")
        with patch.dict(os.environ, GITHUB_RUN_ID="456", GITHUB_RUN_ATTEMPT="2",
                        GITHUB_OUTPUT=str(self.root / "outputs")):
            release.candidate(args)
        data = release.read_json("identity.json")
        self.assertEqual(data["sha"], self.candidate["sha"])
        self.assertEqual(data["base_sha"], "a" * 40)
        self.assertEqual(set(data["lockfiles"]), {"Cargo.lock", "examples/sse-cli/Cargo.lock"})
        self.assertEqual(data["drasi_dependencies"][0]["version"], "0.11.3")
        self.assertEqual(data["reviewed_plugin_abi"], "0.14.0")
        (self.root / "Cargo.toml").write_text("dirty")
        with self.assertRaisesRegex(ValueError, "uncommitted"):
            release.candidate(args)

    def test_lockfile_hashes_and_windows_binary_record_survive_git_eol_settings(self):
        with patch.dict(os.environ, GITHUB_OUTPUT=str(self.root / "outputs")):
            release.candidate(SimpleNamespace(
                kind="release", base_sha="", tag="", output="candidate.json"))
        locks = {name: (self.root / name).read_bytes() for name in self.candidate["lockfiles"]}
        release.write_json("runtime.json", self.runtime())
        binary = self.artifacts / "drasi-server-x86_64-windows-msvc.exe"
        args = SimpleNamespace(
            candidate="candidate.json", type="binary", target="x86_64-windows-msvc",
            binary=binary / binary.name, runtime="runtime.json", output="windows.json")

        subprocess.run(["git", "config", "core.eol", "crlf"], check=True)
        for autocrlf in ("true", "false", "input"):
            with self.subTest(autocrlf=autocrlf):
                subprocess.run(["git", "config", "core.autocrlf", autocrlf], check=True)
                for name in locks:
                    (self.root / name).unlink()
                subprocess.run(["git", "checkout-index", "--", *locks], check=True)
                for name, original in locks.items():
                    self.assertEqual((self.root / name).read_bytes(), original)
                    self.assertNotIn(b"\r", original)
                release.record(args)
                self.assertEqual(release.read_json("windows.json")["binary_sha256"],
                                 release.sha256(args.binary))

    def test_changed_lockfile_bytes_still_reject_candidate(self):
        for name in self.candidate["lockfiles"]:
            path = self.root / name
            original = path.read_bytes()
            for changed in (original.replace(b"\n", b"\r\n"), original + b"\n# changed\n"):
                with self.subTest(lockfile=name, changed=changed):
                    path.write_bytes(changed)
                    with self.assertRaisesRegex(ValueError, "Candidate dependency lockfiles changed"):
                        release.load_candidate("candidate.json")
                    path.write_bytes(original)
        self.assertEqual(release.load_candidate("candidate.json"), self.candidate)

    def test_assets_have_exact_names_and_committed_examples_only(self):
        (self.root / "examples/untracked-secret").write_text("must not ship")
        release.assets(self.asset_args)
        self.assertEqual(len(release.validate_assets(self.root / "assets")), 15)
        with zipfile.ZipFile(self.root / "assets/drasi-server-examples.zip") as archive:
            self.assertNotIn("examples/untracked-secret", archive.namelist())

    def test_missing_duplicate_and_empty_binaries_fail(self):
        name = release.binary_name(release.TARGETS[0])
        binary = self.artifacts / name / name
        binary.unlink()
        with self.assertRaisesRegex(ValueError, "Missing"):
            release.assets(self.asset_args)
        self.asset_args.directory.rmdir()
        binary.write_text("")
        with self.assertRaisesRegex(ValueError, "empty"):
            release.assets(self.asset_args)
        duplicate = self.artifacts / name / "duplicate"
        duplicate.write_text("unexpected")
        self.asset_args.directory = self.root / "duplicate-assets"
        with self.assertRaisesRegex(ValueError, "duplicate"):
            release.assets(self.asset_args)

    def test_malformed_examples_archives_fail(self):
        release.assets(self.asset_args)
        archive = self.root / "assets/drasi-server-examples.zip"
        cases = [
            {"examples/README.md": "Example documentation\n"},
            {"../outside": "unsafe"},
            {"examples/README.md": "", "examples/config.yaml": "sources: []\n"},
        ]
        for entries in cases:
            with self.subTest(entries=entries):
                with zipfile.ZipFile(archive, "w") as output:
                    for name, content in entries.items():
                        output.writestr(name, content)
                with self.assertRaises(ValueError):
                    release.validate_assets(self.root / "assets")
        archive.write_bytes(b"not a zip")
        with self.assertRaises(zipfile.BadZipFile):
            release.validate_assets(self.root / "assets")

    def test_every_required_failure_cancellation_or_skip_fails_closed(self):
        for job in release.REQUIRED_JOBS:
            for result in ("failure", "cancelled", "skipped", None):
                with self.subTest(job=job, result=result):
                    jobs = success_jobs()
                    jobs[job]["result"] = result
                    with self.assertRaisesRegex(ValueError, "did not succeed"):
                        release.check_jobs(jobs)
            jobs = success_jobs()
            del jobs[job]
            with self.assertRaisesRegex(ValueError, "Missing"):
                release.check_jobs(jobs)

    def test_complete_evidence_passes_but_missing_matrix_member_fails(self):
        self.prepare_evidence()
        release.gate(self.gate_args)
        approval = release.read_json("approval.json")
        self.assertEqual(set(approval["images"]), {"amd64", "arm64"})
        for path in self.evidence.glob("*.json"):
            original = path.read_text()
            path.unlink()
            with self.subTest(missing=path.name), self.assertRaisesRegex(ValueError, "Missing"):
                release.gate(self.gate_args)
            path.write_text(original)

    def test_unrelated_sha_run_attempt_and_runtime_do_not_satisfy_gate(self):
        self.prepare_evidence()
        path = self.evidence / "container-amd64.json"
        original = release.read_json(path)
        for key, value in (("sha", "b" * 40), ("run_id", "different"),
                           ("run_attempt", "2"), ("kind", "locked"),
                           ("image", "ghcr.io/test/drasi-server"),
                           ("digest", "sha256:" + "9" * 64),
                           ("runtime", {"checks": ["health"]}),
                           ("default_runtime", {"checks": []})):
            with self.subTest(key=key):
                release.write_json(path, {**original, key: value})
                with self.assertRaises(ValueError):
                    release.gate(self.gate_args)
        release.write_json(path, original)
        name = release.binary_name(release.TARGETS[0])
        (self.root / "assets" / name).write_text("replaced after testing")
        with self.assertRaisesRegex(ValueError, "changed"):
            release.gate(self.gate_args)

    def test_previous_attempt_cannot_reuse_its_approval(self):
        self.prepare_evidence()
        with patch.dict(os.environ, GITHUB_RUN_ATTEMPT="2"):
            with self.assertRaisesRegex(ValueError, "run/attempt"):
                release.gate(self.gate_args)

    def test_record_requires_default_startup_digest_and_reviewed_abi(self):
        runtime = self.runtime()
        release.write_json("runtime.json", runtime)
        release.write_json("default.json", self.runtime(default_startup=True))
        args = SimpleNamespace(
            candidate="candidate.json", runtime="runtime.json", type="container",
            target="amd64", image=self.stage + "@sha256:" + "1" * 64,
            default_runtime="default.json", output="record.json")
        with patch.object(release, "inspect_manifest", return_value=image_manifest("amd64")):
            release.record(args)
        args.image = self.stage + ":latest"
        with self.assertRaisesRegex(ValueError, "digest"):
            release.record(args)
        args.image = self.stage + "@sha256:" + "1" * 64
        runtime["plugins"][0]["sdkVersion"] = "wrong ABI"
        release.write_json("runtime.json", runtime)
        with self.assertRaisesRegex(ValueError, "ABI"):
            release.record(args)

    def test_injected_failures_prevent_every_official_write(self):
        self.prepare_evidence()
        release.gate(self.gate_args)
        original = release.read_json("approval.json")
        args = SimpleNamespace(
            candidate="candidate.json", approval="approval.json",
            staging_image=self.stage, image="ghcr.io/test/drasi-server", output="published.json")
        for job in ("test-binaries", "test-containers"):
            for result in ("failure", "cancelled", "skipped"):
                approval = copy.deepcopy(original)
                approval["jobs"][job]["result"] = result
                release.write_json("approval.json", approval)
                with patch.object(release, "run", return_value=self.candidate["sha"]), \
                        patch.object(release.subprocess, "run") as docker:
                    with self.assertRaises(ValueError):
                        release.promote(args)
                    docker.assert_not_called()
        release.write_json("approval.json", original)
        self.candidate["kind"] = "upgrade"
        release.write_json("candidate.json", self.candidate)
        with patch.object(release, "run", return_value=self.candidate["sha"]), \
                patch.object(release.subprocess, "run") as docker:
            with self.assertRaises(ValueError):
                release.promote(args)
            docker.assert_not_called()

    def test_promotion_uses_only_tested_digests_and_checks_multiarch_index(self):
        self.prepare_evidence()
        release.gate(self.gate_args)
        args = SimpleNamespace(
            candidate="candidate.json", approval="approval.json",
            staging_image=self.stage, image="ghcr.io/test/drasi-server", output="published.json")
        manifests = {"manifests": [
            entry for arch in release.ARCHES for entry in image_manifest(arch)["manifests"]
        ]}
        descriptors = [
            {"digest": "sha256:" + "1" * 64}, {"digest": "sha256:" + "2" * 64},
            {"digest": "sha256:" + "3" * 64}, {"digest": "sha256:" + "3" * 64},
        ]
        def read_command(*command):
            return self.candidate["sha"] if command[0] == "git" else json.dumps(manifests)

        with patch.object(release.subprocess, "run") as docker, \
                patch.object(release, "run", side_effect=read_command), \
                patch.object(release, "inspect_manifest", side_effect=descriptors):
            release.promote(args)
        commands = [call.args[0] for call in docker.call_args_list]
        self.assertEqual(len(commands), 4)
        self.assertTrue(all(command[:4] == ["docker", "buildx", "imagetools", "create"]
                            for command in commands))
        for i, command in enumerate(commands[:2], 1):
            self.assertEqual(command[-1], self.stage + "@sha256:" + str(i) * 64)
        self.assertEqual(set(release.read_json("published.json")["published"]), {"0.2.4", "latest"})
        manifests["manifests"].pop()
        with patch.object(release.subprocess, "run"), \
                patch.object(release, "run", side_effect=read_command), \
                patch.object(release, "inspect_manifest", side_effect=descriptors):
            with self.assertRaisesRegex(ValueError, "multiarch"):
                release.promote(args)


class SmokeAssertionsTests(unittest.TestCase):
    def responses(self):
        return {
            "/health": {"status": "ok"},
            "/api/v1/sources": {"success": True, "data": [{"id": "test-source", "status": "Running"}]},
            "/api/v1/queries": {"success": True, "data": [{"id": "all-sensors"}]},
            "/api/v1/queries/all-sensors/results": {
                "success": True, "data": [{"SensorId": "sensor-1", "Temperature": 23.5, "Humidity": 40}]},
            "/api/v1/plugins": {"plugins": [
                {"kinds": [{"kind": kind, "category": category}], "sdkVersion": "0.14.0",
                 "pluginVersion": "0.2.12", "status": "Active"}
                for category, kind in (("Source", "mock"), ("Reaction", "log"))
            ]},
        }

    def test_real_response_shapes_pass(self):
        self.assertEqual(len(assert_api.check_api(self.responses().__getitem__)["checks"]), 5)

    def test_default_startup_requires_all_shipped_components_running(self):
        responses = self.responses()
        for category, ids in {
            "sources": ("vehicle-location-source", "order-status-source"),
            "queries": ("available-drivers-query", "pending-orders-query"),
            "reactions": ("driver-availability-logger", "order-notification-handler"),
        }.items():
            responses[f"/api/v1/{category}"] = {
                "success": True, "data": [{"id": name, "status": "Running"} for name in ids],
            }
        self.assertEqual(assert_api.check_api(responses.__getitem__, True)["checks"],
                         ["health", "default-components", "plugins"])
        responses["/api/v1/reactions"]["data"][0]["status"] = "Error"
        with self.assertRaisesRegex(AssertionError, "Default reactions"):
            assert_api.check_api(responses.__getitem__, True)

    def test_empty_results_failed_api_and_invalid_numeric_values_fail(self):
        for results in (
            {"success": True, "data": []},
            {"success": False, "data": [{"SensorId": "not enough"}]},
            {"success": True, "data": [{"SensorId": "x", "Temperature": "23", "Humidity": 40}]},
        ):
            responses = self.responses()
            responses["/api/v1/queries/all-sensors/results"] = results
            with self.assertRaises(AssertionError):
                assert_api.check_api(responses.__getitem__)
        responses = self.responses()
        responses["/api/v1/sources"]["data"][0]["status"] = "Error"
        with self.assertRaises(AssertionError):
            assert_api.check_api(responses.__getitem__)


class WorkflowContractTests(unittest.TestCase):
    def test_workflow_gates_and_all_seven_targets_are_wired(self):
        workflow = (ROOT / ".github/workflows/release.yaml").read_text()
        job_text = workflow.split("\njobs:\n", 1)[1]
        jobs = dict(re.findall(r"^  ([\w-]+):\n(.*?)(?=^  [\w-]+:\n|\Z)",
                               job_text, re.MULTILINE | re.DOTALL))
        gate = jobs["release-gate"]
        needs = re.search(r"needs: \[(.*?)\]", gate).group(1)
        self.assertEqual(set(needs.split(", ")), release.REQUIRED_JOBS)
        self.assertIn("if: always()", gate)
        for name in ("create-manifest", "create-release"):
            self.assertIn("needs.release-gate.result == 'success'", jobs[name])
            self.assertIn("inputs.candidate_kind == ''", jobs[name])
            self.assertIn("inputs.candidate_sha == ''", jobs[name])
            self.assertIn("!inputs.dry_run", jobs[name])
            self.assertIn("github.event_name == 'workflow_dispatch'", jobs[name])
        targets = re.findall(r"artifact_suffix: ([\w-]+)", jobs["test-binaries"])
        self.assertEqual(set(targets), set(release.TARGETS))
        self.assertEqual(len(targets), 7)
        self.assertIn("fail-fast: false", jobs["test-binaries"])
        self.assertNotIn("make docker-build", jobs["create-manifest"])
        self.assertNotIn("IMAGE_PREFIX", jobs["build-docker"])
        self.assertNotIn("if:", jobs["assemble-assets"])
        for name, job in jobs.items():
            if name != "determine-version":
                self.assertIn("needs.determine-version.outputs.sha", job, name)
        for name in ("build-sse-cli-linux", "build-sse-cli-macos", "build-sse-cli-windows"):
            for line in jobs[name].splitlines():
                if " build --" in line:
                    self.assertIn("--locked", line)

    def test_nightly_and_real_bump_share_policy_without_official_branch_mutation(self):
        nightly = (ROOT / ".github/workflows/nightly.yml").read_text()
        bump = (ROOT / ".github/workflows/version-bump.yaml").read_text()
        for workflow in (nightly, bump):
            self.assertIn("bash scripts/update-release-dependencies.sh", workflow)
        self.assertIn("cron: '0 8 * * *'", nightly)
        self.assertIn(
            "ref: ${{ github.event_name == 'schedule' && 'main' || github.sha }}",
            nightly)
        self.assertIn("automation/release-rehearsal/${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}", nightly)
        self.assertNotIn("automation/version-bump", nightly)
        self.assertNotIn("continue-on-error", nightly)
        self.assertNotIn("gh release", nightly)
        self.assertNotIn("git tag", nightly)

    def test_nightly_has_one_candidate_and_calls_release_directly(self):
        nightly = (ROOT / ".github/workflows/nightly.yml").read_text()
        self.assertIn("name: Nightly\n", nightly)
        self.assertIn("workflow_dispatch:", nightly)
        self.assertNotIn("workflow_call:", nightly)
        jobs = dict(re.findall(r"^  ([\w-]+):\n(.*?)(?=^  [\w-]+:\n|\Z)",
                               nightly.split("\njobs:\n", 1)[1], re.MULTILINE | re.DOTALL))
        self.assertEqual(set(jobs), {"prepare-candidate", "release-checks", "summary"})
        self.assertIn("base_sha: ${{ steps.baseline.outputs.sha }}", jobs["prepare-candidate"])
        self.assertIn("sha: ${{ steps.commit.outputs.sha }}", jobs["prepare-candidate"])
        self.assertIn("needs: prepare-candidate", jobs["release-checks"])
        self.assertIn("candidate_sha: ${{ needs.prepare-candidate.outputs.sha }}", jobs["release-checks"])
        self.assertIn("base_sha: ${{ needs.prepare-candidate.outputs.base_sha }}", jobs["release-checks"])
        self.assertIn("candidate_kind: upgrade", jobs["release-checks"])
        self.assertIn("dry_run: true", jobs["release-checks"])
        self.assertEqual(re.findall(r"uses: \./\.github/workflows/([\w.-]+)", nightly), ["release.yaml"])
        self.assertNotIn("\n  push:", nightly)

    def test_release_is_a_standalone_entrypoint_without_nested_wrappers(self):
        workflow = (ROOT / ".github/workflows/release.yaml").read_text()
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("workflow_call:", workflow)
        self.assertNotIn("nightly_test", workflow)
        self.assertNotIn("publish_official", workflow)
        self.assertNotIn("uses: ./.github/workflows/", workflow)
        dispatch, reusable = workflow.split("  workflow_call:\n", 1)
        self.assertRegex(dispatch, r"dry_run:\n(?:.*\n)*?        default: false")
        self.assertRegex(reusable, r"dry_run:\n(?:.*\n)*?        default: true")
        for name in ("release-rehearsal.yaml", "release-candidate.yaml"):
            self.assertFalse((ROOT / ".github/workflows" / name).exists())

    def test_nightly_summary_rejects_failures_cancellations_and_skips(self):
        nightly = (ROOT / ".github/workflows/nightly.yml").read_text()
        summary = nightly.split("\n  summary:\n", 1)[1]
        self.assertIn("if: always()", summary)
        self.assertIn("needs: [prepare-candidate, release-checks]", summary)
        self.assertIn("recorded, not separately tested", summary)
        script = "\n".join(line[10:] for line in summary.split("run: |\n", 1)[1].splitlines())
        with tempfile.TemporaryDirectory() as temp:
            for preparation in ("success", "failure", "cancelled", "skipped", ""):
                for checks in ("success", "failure", "cancelled", "skipped", ""):
                    with self.subTest(preparation=preparation, checks=checks):
                        result = subprocess.run(
                            ["bash", "-euo", "pipefail", "-c", script],
                            env={**os.environ, "GITHUB_STEP_SUMMARY": str(Path(temp) / "summary"),
                                 "PREPARATION_RESULT": preparation, "CHECKS_RESULT": checks,
                                 "BASE_SHA": "a" * 40, "CANDIDATE_SHA": "b" * 40},
                            capture_output=True, text=True,
                        )
                        self.assertEqual(result.returncode == 0,
                                         preparation == checks == "success", result.stderr)
                        if result.returncode:
                            self.assertIn("::error::", result.stdout)

    def test_container_checks_do_not_repair_or_override_the_runtime(self):
        script = (ROOT / "scripts/smoke-release-container.sh").read_text()
        self.assertIn('docker pull --platform "linux/$ARCH" "$IMAGE"', script)
        self.assertIn('CONTAINER=$(docker run -d -p 127.0.0.1::8080 "$IMAGE")', script)
        self.assertIn('"$IMAGE" --config /smoke.yaml', script)
        for forbidden in ("--user", "--entrypoint", "apt-get", "apk add"):
            self.assertNotIn(forbidden, script)
        self.assertIn("org.opencontainers.image.revision", script)
        self.assertIn("assert-api.py", script)


class DependencyPolicyTests(unittest.TestCase):
    def test_shared_updater_preserves_the_scoped_version_bump_policy(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            binary_dir = directory / "bin"
            binary_dir.mkdir()
            calls = directory / "calls.jsonl"
            cargo = binary_dir / "cargo"
            cargo.write_text(f"#!{sys.executable}\n" + """
import json, os, sys
with open(os.environ["CALL_LOG"], "a") as output:
    output.write(json.dumps(sys.argv[1:]) + "\\n")
if sys.argv[1] == "metadata":
    print(json.dumps({"packages": [
        {"dependencies": [{"name": "drasi-lib"}, {"name": "serde"}]},
        {"dependencies": [{"name": "drasi-core"}, {"name": "drasi-lib"}]}
    ]}))
if sys.argv[1] == os.environ.get("FAIL_COMMAND"):
    raise SystemExit(17)
""")
            cargo.chmod(0o755)
            (directory / "Cargo.toml").write_text('[package]\nversion="0.2.5"\n')
            env = {**os.environ, "PATH": str(binary_dir) + os.pathsep + os.environ["PATH"],
                   "CALL_LOG": str(calls), "MODE": "bump", "BUMP": "patch",
                   "GITHUB_OUTPUT": str(directory / "output")}
            script = ["bash", str(ROOT / "scripts/update-release-dependencies.sh")]
            result = subprocess.run(script, cwd=directory, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            commands = [json.loads(line) for line in calls.read_text().splitlines()]
            self.assertEqual(commands[1], ["upgrade", "--incompatible", "--package",
                                           "drasi-core", "--package", "drasi-lib"])
            self.assertEqual(commands[2], ["update", "--package", "drasi-core",
                                           "--package", "drasi-lib"])
            self.assertEqual(commands[3], ["set-version", "--package", "drasi-server",
                                           "--bump", "patch"])
            self.assertEqual(commands[4], ["update", "--workspace"])
            calls.write_text("")
            result = subprocess.run(script, cwd=directory,
                                    env={**env, "FAIL_COMMAND": "upgrade"},
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 17)
            self.assertEqual(len(calls.read_text().splitlines()), 2)


if __name__ == "__main__":
    unittest.main()
