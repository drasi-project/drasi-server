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
        (scripts / "prepare-core.sh").write_text(
            '#!/bin/bash\nprintf \'["core","%s"]\\n\' "$*" >> "$POLICY_LOG"\n'
            'if [[ "${FAIL_CORE:-0}" == 1 ]]; then exit 17; fi\n'
            'if [[ "${1:-}" != "--check" ]]; then mkdir "$FIXTURE_CORE"; fi\n'
        )
        binary = self.directory / "bin"
        binary.mkdir()
        python = binary / "python3"
        python.write_text(f"#!{sys.executable}\n" + """
import json, os
from pathlib import Path
assert Path(os.environ["FIXTURE_CORE"]).is_dir(), "Cargo must not run before source preparation"
with open(os.environ["POLICY_LOG"], "a") as output:
    output.write(json.dumps(["origin"]) + "\\n")
print(os.environ["FIXTURE_MODE"])
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

    def test_missing_sibling_is_prepared_before_cargo_origin_probe(self):
        result = self.prepare("--allow-sudo")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "registry")
        self.assertEqual(self.calls(), [["core", "--allow-sudo"], ["origin"], ["core", "--check"]])

    def test_matching_local_sdk_mode_does_not_reset_or_require_the_registry_pin(self):
        self.core.mkdir()
        marker = self.core / "local-development"
        marker.write_text("preserve")
        result = self.prepare(FIXTURE_MODE="local")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [["origin"]])
        self.assertEqual(marker.read_text(), "preserve")

    def test_failed_preparation_stops_before_origin_probe(self):
        result = self.prepare(FAIL_CORE="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [["core", ""]])
        self.assertEqual(result.stdout, "")

    def test_existing_registry_source_must_pass_exact_pin_check(self):
        self.core.mkdir()
        result = self.prepare(FAIL_CORE="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [["origin"], ["core", "--check"]])

    def test_unknown_source_mode_and_invalid_arguments_fail(self):
        self.core.mkdir()
        result = self.prepare(FIXTURE_MODE="unreviewed")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported plugin dependency origin", result.stderr)
        result = self.prepare("--unexpected")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Usage:", result.stderr)

    def test_yaml_validation_prepares_sources_and_reports_preparation_failure(self):
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
        for tool, body in (
            ("sudo", "exit 0\n"),
            ("dpkg-architecture", "echo fixture-architecture\n"),
            ("cargo", 'printf \'["cargo","%s"]\\n\' "$*" >> "$POLICY_LOG"\n'),
        ):
            stub = self.directory / "bin" / tool
            stub.write_text("#!/bin/bash\n" + body)
            stub.chmod(0o755)

        results = self.directory / "results"
        build = build.replace("/tmp/gh-aw/agent", str(results))
        failure = failure.replace("/tmp/gh-aw/agent", str(results))
        for fail_core in ("0", "1"):
            with self.subTest(fail_core=fail_core):
                if self.core.exists():
                    self.core.rmdir()
                self.log.write_text("")
                environment = {**self.env, "FAIL_CORE": fail_core}
                result = subprocess.run(
                    ["bash", "-c", build], cwd=self.server, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.calls(), [
                    ["core", ""],
                    ["cargo", "test --test readme_examples_validation_test"],
                    ["cargo", "test --test example_configs_validation_test"],
                    ["cargo", "test --test config_parsing_failure_test"],
                ])
                report = (results / "validation-results.txt").read_text()
                self.assertIn("## prepare pinned core source\n", report)
                self.assertIn("exit_code=17" if fail_core == "1" else "exit_code=0", report)
                self.assertEqual(report.count("exit_code="), 5)
                gate = subprocess.run(
                    ["bash", "-c", failure], cwd=self.server, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                )
                self.assertEqual(gate.returncode, int(fail_core), gate.stderr)
                if fail_core == "1":
                    self.assertIn("One or more validation steps failed", gate.stderr)


if __name__ == "__main__":
    unittest.main()
