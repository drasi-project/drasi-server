import json
from pathlib import Path
import runpy
import subprocess
import sys
import unittest


SCRIPT = Path(__file__).with_name("check-stopped.py")
validate = runpy.run_path(str(SCRIPT))["validate"]
STOPPED = {
    "Status": "exited",
    "ExitCode": 0,
    "Error": "",
    "Running": False,
    "Paused": False,
    "Restarting": False,
    "OOMKilled": False,
    "Dead": False,
}


class StoppedTests(unittest.TestCase):
    def test_clean_exit(self):
        self.assertTrue(validate(STOPPED))

    def test_forced_kill_and_other_errors(self):
        for code in (1, 137, 143, None, False, 0.0):
            with self.subTest(code=code):
                self.assertFalse(validate({**STOPPED, "ExitCode": code}))
        self.assertFalse(validate({**STOPPED, "Error": "shutdown failed"}))

    def test_abnormal_flags_and_status(self):
        for flag in ("Running", "Paused", "Restarting", "OOMKilled", "Dead"):
            with self.subTest(flag=flag):
                self.assertFalse(validate({**STOPPED, flag: True}))
        for status in ("running", "created", "dead"):
            self.assertFalse(validate({**STOPPED, "Status": status}))

    def test_missing_or_invalid_state(self):
        for key in STOPPED:
            self.assertFalse(validate({k: v for k, v in STOPPED.items() if k != key}))
        for value in (None, [], "exited"):
            self.assertFalse(validate(value))

    def test_cli_reports_forced_kill_as_failure(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "drasi"],
            input=json.dumps({**STOPPED, "ExitCode": 137}),
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("drasi did not stop gracefully", result.stderr)
        self.assertIn("137", result.stderr)


if __name__ == "__main__":
    unittest.main()
