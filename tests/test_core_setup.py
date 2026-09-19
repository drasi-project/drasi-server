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


class CorePreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.server = self.root / "server"
        self.source = self.root / "managed-core"
        self.sibling = self.root / "drasi-core"
        (self.server / "scripts").mkdir(parents=True)
        shutil.copyfile(ROOT / "scripts/prepare-core.sh", self.server / "scripts/prepare-core.sh")
        self.source.mkdir()
        self.git("init", "--quiet", self.source)
        (self.source / "fixture.txt").write_text("immutable test fixture, not Drasi source\n")
        self.git("-C", self.source, "add", "fixture.txt")
        self.git(
            "-C", self.source, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", "commit", "--quiet",
            "-m", "Fixture\n\nSigned-off-by: Fixture <fixture@example.invalid>\n\n"
            "Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>",
        )
        self.revision = self.git("-C", self.source, "rev-parse", "HEAD").stdout.strip()
        (self.server / ".drasi-core-revision").write_text(self.revision + "\n")

    def git(self, *arguments):
        return subprocess.run(
            ["git", *(str(argument) for argument in arguments)], check=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )

    def prepare(self, *arguments, env=None):
        return subprocess.run(
            ["bash", str(self.server / "scripts/prepare-core.sh"), *arguments],
            cwd=self.server, env=env, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )

    def test_existing_managed_worktree_link_is_checked_without_rewriting_it(self):
        self.sibling.symlink_to(self.source, target_is_directory=True)
        result = self.prepare("--check")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(self.revision, result.stdout)
        self.assertEqual(self.sibling.readlink(), self.source)
        self.assertEqual(self.git("-C", self.source, "rev-parse", "HEAD").stdout.strip(), self.revision)

    def test_wrong_revision_and_dirty_source_are_never_replaced(self):
        self.sibling.symlink_to(self.source, target_is_directory=True)
        (self.server / ".drasi-core-revision").write_text("0" * 40 + "\n")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checkout was not modified", result.stderr)
        self.assertEqual(self.git("-C", self.source, "rev-parse", "HEAD").stdout.strip(), self.revision)
        (self.server / ".drasi-core-revision").write_text(self.revision + "\n")
        (self.source / "fixture.txt").write_text("owner's unfinished work\n")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("uncommitted", result.stderr)
        self.assertEqual((self.source / "fixture.txt").read_text(), "owner's unfinished work\n")

    def test_existing_unrelated_directory_is_not_overwritten(self):
        self.sibling.mkdir()
        marker = self.sibling / "owner.txt"
        marker.write_text("do not replace\n")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(marker.read_text(), "do not replace\n")
        self.assertFalse((self.sibling / ".git").exists())

    def test_broken_link_is_not_overwritten(self):
        self.sibling.symlink_to(self.root / "absent")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.sibling.readlink(), self.root / "absent")

    def test_check_only_never_creates_missing_source(self):
        result = self.prepare("--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing sibling", result.stderr)
        self.assertFalse(self.sibling.exists())

    def test_floating_or_missing_revision_is_rejected_before_creating_source(self):
        for revision in ("main\n", self.revision + "\nextra\n", "a" * 39 + "\n"):
            with self.subTest(revision=revision):
                (self.server / ".drasi-core-revision").write_text(revision)
                self.assertNotEqual(self.prepare().returncode, 0)
                self.assertFalse(self.sibling.exists())
        (self.server / ".drasi-core-revision").unlink()
        self.assertNotEqual(self.prepare().returncode, 0)
        self.assertFalse(self.sibling.exists())

    def git_transport(self):
        # Redirect the fixed upstream URL only inside this isolated Git transport fixture.
        real_git = shutil.which("git")
        binary = self.root / "bin"
        binary.mkdir()
        log = self.root / "git-commands.jsonl"
        log.touch()
        shim = binary / "git"
        shim.write_text(f"#!{sys.executable}\n" + """
import json, os, subprocess, sys
arguments = sys.argv[1:]
with open(os.environ["FIXTURE_GIT_LOG"], "a") as output:
    output.write(json.dumps(arguments) + "\\n")
if "remote" in arguments and "add" in arguments:
    assert arguments[-1] == "https://github.com/drasi-project/drasi-core.git"
    arguments[-1] = os.environ["FIXTURE_GIT_SOURCE"]
sys.exit(subprocess.run([os.environ["FIXTURE_REAL_GIT"], *arguments]).returncode)
""")
        shim.chmod(0o755)
        environment = {
            **os.environ, "PATH": str(binary) + os.pathsep + os.environ["PATH"],
            "FIXTURE_REAL_GIT": real_git, "FIXTURE_GIT_SOURCE": str(self.source),
            "FIXTURE_GIT_LOG": str(log),
        }
        return environment, log, binary

    def test_absent_sibling_fetches_only_the_exact_revision(self):
        environment, log, _ = self.git_transport()
        result = self.prepare(env=environment)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.git("-C", self.sibling, "rev-parse", "HEAD").stdout.strip(), self.revision)
        calls = [json.loads(line) for line in log.read_text().splitlines()]
        fetch = next(call for call in calls if "fetch" in call)
        self.assertEqual(fetch[-2:], ["origin", self.revision])
        checkout = next(call for call in calls if "checkout" in call)
        self.assertEqual(checkout[-2:], ["--detach", self.revision])
        self.assertFalse(any("reset" in call or "pull" in call for call in calls))

    def test_sudo_opt_in_never_changes_existing_directories_or_links(self):
        binary = self.root / "bin"
        binary.mkdir()
        log = self.root / "sudo-log"
        stub = binary / "sudo"
        stub.write_text(f"#!{sys.executable}\nfrom pathlib import Path\n"
                        f"Path({str(log)!r}).write_text('unexpected sudo')\nraise SystemExit(91)\n")
        stub.chmod(0o755)
        environment = {**os.environ, "PATH": str(binary) + os.pathsep + os.environ["PATH"]}
        self.sibling.mkdir()
        original = self.sibling.stat()
        self.assertNotEqual(self.prepare("--allow-sudo", env=environment).returncode, 0)
        self.assertFalse(log.exists())
        self.assertEqual(self.sibling.stat().st_uid, original.st_uid)
        self.assertEqual(self.sibling.stat().st_mode, original.st_mode)
        self.sibling.rmdir()
        self.sibling.symlink_to(self.source, target_is_directory=True)
        result = self.prepare("--allow-sudo", env=environment)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(log.exists())
        self.assertEqual(self.sibling.readlink(), self.source)

    @unittest.skipIf(os.geteuid() == 0, "Permission boundary requires an unprivileged test user")
    def test_readonly_parent_reserves_only_an_absent_sibling_with_explicit_opt_in(self):
        environment, _, binary = self.git_transport()
        log = self.root / "sudo-commands.jsonl"
        log.touch()
        sudo = binary / "sudo"
        sudo.write_text(f"#!{sys.executable}\n" + """
import json, os, subprocess, sys
from pathlib import Path
parent = Path(os.environ["FIXTURE_PARENT"])
sibling = parent / "drasi-core"
with open(os.environ["FIXTURE_SUDO_LOG"], "a") as output:
    output.write(json.dumps(sys.argv[1:]) + "\\n")
if sys.argv[1:] == ["-n", "mkdir", "--", str(sibling)]:
    parent.chmod(0o700)
    try:
        subprocess.run(["mkdir", "--", str(sibling)], check=True)
    finally:
        parent.chmod(0o500)
elif sys.argv[1:] != ["-n", "chown", "-h", f"{os.getuid()}:{os.getgid()}", str(sibling)]:
    raise SystemExit("Unexpected privilege operation")
""")
        sudo.chmod(0o755)
        environment.update({"FIXTURE_PARENT": str(self.root), "FIXTURE_SUDO_LOG": str(log)})
        self.root.chmod(0o500)
        try:
            result = self.prepare(env=environment)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.sibling.exists())
            self.assertEqual(log.read_text(), "")
            result = self.prepare("--allow-sudo", env=environment)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(self.git("-C", self.sibling, "rev-parse", "HEAD").stdout.strip(), self.revision)
            commands = [json.loads(line) for line in log.read_text().splitlines()]
            self.assertEqual(len(commands), 2)
            self.assertEqual(commands[0], ["-n", "mkdir", "--", str(self.sibling)])
            self.assertEqual(commands[1], [
                "-n", "chown", "-h", f"{os.getuid()}:{os.getgid()}", str(self.sibling),
            ])
            self.assertEqual(self.root.stat().st_mode & 0o777, 0o500)
        finally:
            self.root.chmod(0o700)


if __name__ == "__main__":
    unittest.main()
