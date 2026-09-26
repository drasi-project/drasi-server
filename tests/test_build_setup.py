# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent


class BuildPreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.server = self.directory / "server"
        self.core = self.directory / "drasi-core"
        scripts = self.server / "scripts"
        scripts.mkdir(parents=True)
        shutil.copyfile(ROOT / "scripts/prepare-build.sh", scripts / "prepare-build.sh")
        binary = self.directory / "bin"
        binary.mkdir()
        python = binary / "python3"
        python.write_text(f"#!{sys.executable}\n" + """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as output:
    output.write(json.dumps(["origin"]) + "\\n")
if os.environ.get("FAIL_ORIGIN") == "1":
    print("Unreviewed registry identity", file=sys.stderr)
    raise SystemExit(17)
print(os.environ["FIXTURE_CORE"] if sys.argv[-1] == "local-workspace" else os.environ["FIXTURE_MODE"])
""")
        python.chmod(0o755)
        self.log = self.directory / "calls.jsonl"
        self.env = {
            **os.environ, "PATH": str(binary) + os.pathsep + os.environ["PATH"],
            "POLICY_LOG": str(self.log), "FIXTURE_CORE": str(self.core),
            "FIXTURE_MODE": "registry",
        }

    def prepare(self, *arguments, **environment):
        return subprocess.run(
            ["bash", str(self.server / "scripts/prepare-build.sh"), *arguments],
            env={**self.env, **environment}, cwd=self.server,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_registry_mode_does_not_require_or_create_a_sibling(self):
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "registry")
        self.assertEqual(self.calls(), [["origin"]])
        self.assertFalse(self.core.exists())

    def test_foreign_dirty_or_broken_sibling_is_never_selected_or_changed(self):
        self.core.mkdir()
        marker = self.core / "unfinished-work"
        marker.write_text("preserve unrelated files\n")
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "registry")
        self.assertEqual(marker.read_text(), "preserve unrelated files\n")
        marker.unlink()
        self.core.rmdir()
        self.core.symlink_to(self.directory / "absent")
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.core.readlink(), self.directory / "absent")

    def test_matching_local_sdk_mode_does_not_reset_or_require_the_registry_pin(self):
        self.core.mkdir()
        marker = self.core / "local-development"
        marker.write_text("preserve")
        result = self.prepare(FIXTURE_MODE="local")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [["origin"]])
        self.assertEqual(marker.read_text(), "preserve")

    def test_failed_origin_check_does_not_report_success_or_create_source(self):
        result = self.prepare(FAIL_ORIGIN="1")
        self.assertEqual(result.returncode, 17)
        self.assertEqual(self.calls(), [["origin"]])
        self.assertEqual(result.stdout, "")
        self.assertIn("Unreviewed registry identity", result.stderr)
        self.assertFalse(self.core.exists())

    def test_unknown_source_mode_and_invalid_arguments_fail(self):
        result = self.prepare(FIXTURE_MODE="unreviewed")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported plugin dependency origin", result.stderr)
        for option in ("--unexpected", "--allow-sudo"):
            result = self.prepare(option)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Usage:", result.stderr)

    def test_cross_mounts_only_an_explicitly_selected_local_workspace(self):
        cross = self.directory / "bin/cross"
        cross.write_text(f"#!{sys.executable}\n" + """
import json, os, sys
with open(os.environ["POLICY_LOG"], "a") as output:
    output.write(json.dumps(["cross", sys.argv[1:],
        os.environ.get("CROSS_BUILD_ENV_VOLUMES"),
        os.environ.get("DRASI_CORE_WORKSPACE")]) + "\\n")
""")
        cross.chmod(0o755)
        for mode in ("registry", "local"):
            for target in ("build-cross", "build-cross-release"):
                with self.subTest(mode=mode, target=target):
                    self.log.write_text("")
                    environment = {**self.env, "FIXTURE_MODE": mode}
                    environment.pop("CROSS_BUILD_ENV_VOLUMES", None)
                    environment.pop("DRASI_CORE_WORKSPACE", None)
                    result = subprocess.run(
                        ["make", "-f", str(ROOT / "Makefile"), target,
                         "TARGET=aarch64-unknown-linux-gnu"],
                        cwd=self.server, env=environment, text=True,
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    command = self.calls()[-1]
                    self.assertEqual(command[0], "cross")
                    self.assertIn("--locked", command[1])
                    self.assertEqual("--release" in command[1], target == "build-cross-release")
                    self.assertEqual(command[2:], (
                        ["DRASI_CORE_WORKSPACE", str(self.core)] if mode == "local" else [None, None]
                    ))
                    self.assertFalse(self.core.exists())

    def test_yaml_validation_needs_no_source_preparation_and_reports_test_failure(self):
        def script(document, step):
            job = document.split("\n  build_validation:\n", 1)[1]
            body = job.split(f"      - name: {step}\n", 1)[1].split("        run: |\n", 1)[1]
            lines = []
            for line in body.splitlines():
                if line and not line.startswith("          "):
                    break
                lines.append(line[10:])
            return "\n".join(lines).rstrip("\n") + "\n"

        documents = [
            (ROOT / f".github/workflows/validate-yaml-snippets.{extension}").read_text()
            for extension in ("md", "lock.yml")
        ]
        build, compiled_build = [
            script(document, "Build server and run config validation tests")
            for document in documents
        ]
        failure, compiled_failure = [
            script(document, "Fail if any validation step failed") for document in documents
        ]
        self.assertEqual(build, compiled_build)
        self.assertEqual(failure, compiled_failure)
        self.assertNotIn("prepare-core", build)
        for tool, body in (
            ("sudo", "exit 0\n"),
            ("dpkg-architecture", "echo fixture-architecture\n"),
            ("cargo", 'printf \'["cargo","%s"]\\n\' "$*" >> "$POLICY_LOG"\n'
             'if [[ "${FAIL_TEST:-0}" == 1 && "$*" == *readme_examples_validation_test ]]; '
             'then exit 17; fi\n'),
        ):
            stub = self.directory / "bin" / tool
            stub.write_text("#!/bin/bash\n" + body)
            stub.chmod(0o755)

        results = self.directory / "results"
        build = build.replace("/tmp/gh-aw/agent", str(results))
        failure = failure.replace("/tmp/gh-aw/agent", str(results))
        for fail_test in ("0", "1"):
            with self.subTest(fail_test=fail_test):
                self.log.write_text("")
                environment = {**self.env, "FAIL_TEST": fail_test}
                result = subprocess.run(
                    ["bash", "-c", build], cwd=self.server, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.calls(), [
                    ["cargo", "test --test readme_examples_validation_test"],
                    ["cargo", "test --test example_configs_validation_test"],
                    ["cargo", "test --test config_parsing_failure_test"],
                ])
                report = (results / "validation-results.txt").read_text()
                self.assertNotIn("prepare pinned core", report)
                self.assertIn("exit_code=17" if fail_test == "1" else "exit_code=0", report)
                self.assertEqual(report.count("exit_code="), 4)
                self.assertFalse(self.core.exists())
                gate = subprocess.run(
                    ["bash", "-c", failure], cwd=self.server, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                self.assertEqual(gate.returncode, int(fail_test), gate.stderr)
                if fail_test == "1":
                    self.assertIn("One or more validation steps failed", gate.stderr)


if __name__ == "__main__":
    unittest.main()
