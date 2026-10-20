#!/usr/bin/env python3
"""Exercise locally built GPU/native binaries against a new disposable PostgreSQL instance."""

import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import subprocess
import time
from urllib.error import HTTPError, URLError
from urllib.request import urlopen
from uuid import UUID, uuid4

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "/api/v1/instances/gpu-demo"


def loopback_ports(count):
    sockets = []
    try:
        for _ in range(count):
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            sockets.append(listener)
        return [listener.getsockname()[1] for listener in sockets]
    finally:
        for listener in sockets:
            listener.close()


class Qualification:
    def __init__(self, args):
        self.args = args
        for path in (args.runtime, args.control, args.native_plugin, args.sse_plugin):
            if not path.resolve().is_file():
                raise ValueError(f"Required local artifact is missing: {path}")
        self.evidence = args.evidence.resolve()
        self.evidence.mkdir(parents=True, exist_ok=False)
        self.work = self.evidence / "work"
        self.work.mkdir()
        self.state = self.evidence / "state"
        self.state.mkdir()
        self.password = secrets.token_hex(32)
        self.token = secrets.token_hex(32)
        self.container = None
        self.processes = {}
        self.observations = {}
        self.events = []
        self.api_port, self.sse_port, self.control_port = loopback_ports(3)
        self.api = f"http://127.0.0.1:{self.api_port}"
        self.control = f"http://127.0.0.1:{self.control_port}"
        self.runtime_env = None
        self.slot_name = None

    def run(self, command, *, data=None, env=None, timeout=120, check=True):
        result = subprocess.run(command, input=data, env=env, cwd=self.work,
                                text=True, capture_output=True, timeout=timeout)
        if check and result.returncode:
            error = result.stderr.replace(self.password, "[test-password]").replace(self.token, "[test-token]")
            raise RuntimeError(f"{Path(command[0]).name} failed ({result.returncode}): {error[-8000:]}")
        return result

    def sql(self, statement):
        if self.container is None:
            raise RuntimeError("The isolated PostgreSQL container is not running")
        return self.run([
            "podman", "exec", "-i", self.container, "psql", "-X", "-qAt",
            "-v", "ON_ERROR_STOP=1", "-U", "gpu_owner", "-d", "gpu_demo",
        ], data=statement).stdout.strip()

    def start(self, name, binary, env):
        if name in self.processes:
            raise RuntimeError(f"Refusing to replace a still-owned {name} process")
        log = (self.evidence / f"{name}.log").open("a")
        try:
            process = subprocess.Popen([str(binary.resolve())], cwd=self.work, env=env,
                                       stdout=log, stderr=subprocess.STDOUT)
        except OSError:
            log.close()
            raise
        self.processes[name] = (process, log)
        self.events.append({"event": "start", "process": name, "pid": process.pid})

    def stop(self, name, *, abrupt=False):
        owned = self.processes.get(name)
        if owned is None:
            return
        process, log = owned
        forced = False
        try:
            if process.poll() is None:
                process.send_signal(signal.SIGKILL if abrupt else signal.SIGTERM)
                try:
                    process.wait(timeout=90)
                except subprocess.TimeoutExpired:
                    forced = True
                    process.kill()
                    process.wait(timeout=10)
            self.events.append({"event": "stop", "process": name, "pid": process.pid,
                                "returncode": process.returncode, "abrupt": abrupt})
            if forced or (not abrupt and process.returncode != 0):
                raise RuntimeError(f"{name} did not complete graceful cleanup: {process.returncode}")
        finally:
            log.close()
            self.processes.pop(name)

    def alive(self):
        for name, (process, _) in self.processes.items():
            if process.poll() is not None:
                raise RuntimeError(f"{name} exited {process.returncode}; inspect {self.evidence / (name + '.log')}")

    def until(self, description, check, timeout=120):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.alive()
            try:
                if check():
                    self.events.append({"event": "pass", "check": description})
                    print(f"PASS {description}", flush=True)
                    return
            except (URLError, ConnectionError, TimeoutError) as error:
                self.observations["transport"] = str(error)
            time.sleep(0.1)
        raise AssertionError(f"Timed out: {description}; evidence: {self.evidence}")

    def get(self, base, path):
        try:
            with urlopen(base + path, timeout=5) as response:
                status, body = response.status, json.load(response)
        except HTTPError as error:
            status, body = error.code, json.load(error)
        self.observations[path] = {"status": status, "body": body}
        return status, body

    def pair(self, gpu):
        status, body = self.get(self.api, PREFIX + "/queries/input-configuration/results")
        if status != 200:
            return None
        assert body.get("success") is True, body
        rows = body["data"]
        if len(rows) != 1:
            return None
        records = rows[0]["records"]
        inventory = [row["value"] for row in records if row["table"] == "gpu_inventory"
                     and row["value"]["gpu_id"] == gpu]
        settings = [row["value"] for row in records if row["table"] == "gpu_telemetry"
                    and row["value"]["gpu_id"] == gpu]
        assert len(inventory) == len(settings) == 1, (inventory, settings)
        name, demand = inventory[0]["name"], settings[0]["background_compute_units"]
        if name.startswith("native-tx-"):
            assert int(name.removeprefix("native-tx-")) == demand, "torn multi-table configuration"
        return name, demand

    def ready(self):
        status, body = self.get(self.control, "/health/ready")
        return status == 200 and body.get("ready") is True

    def slot(self):
        names = self.sql("SELECT slot_name FROM pg_replication_slots WHERE database='gpu_demo' ORDER BY slot_name;").splitlines()
        assert len(names) == 1 and names[0].startswith("gpu_native_"), names
        UUID(names[0].removeprefix("gpu_native_"))
        return names[0]

    @staticmethod
    def mutation(gpu, number):
        return (
            f"BEGIN; UPDATE gpu_inventory SET name='native-tx-{number}' WHERE gpu_id='{UUID(gpu)}'; "
            f"UPDATE gpu_telemetry SET background_compute_units={number} WHERE gpu_id='{UUID(gpu)}'; COMMIT;\n"
        )

    def setup(self):
        name = f"gpu-native-qualification-{uuid4().hex[:12]}"
        result = self.run([
            "podman", "run", "--detach", "--name", name, "--label", f"drasi.qualification={name}",
            "--publish", "127.0.0.1::5432", "--tmpfs", "/var/lib/postgresql/data:rw",
            "--env", "POSTGRES_USER=gpu_owner", "--env", "POSTGRES_DB=gpu_demo",
            "--env", f"POSTGRES_PASSWORD={self.password}", "postgres:16-alpine",
            "-c", "wal_level=logical", "-c", "max_replication_slots=4",
            "-c", "max_wal_senders=8", "-c", "max_slot_wal_keep_size=-1",
        ])
        self.container = result.stdout.strip()
        if len(self.container) != 64 or any(char not in "0123456789abcdef" for char in self.container):
            raise RuntimeError("Podman did not return a concrete isolated container ID")
        address = self.run(["podman", "port", self.container, "5432/tcp"]).stdout.strip()
        assert address.startswith("127.0.0.1:"), address
        postgres_port = int(address.rsplit(":", 1)[1])
        self.until("fresh PostgreSQL", lambda: self.run([
            "podman", "exec", self.container, "pg_isready", "-U", "gpu_owner", "-d", "gpu_demo",
        ], check=False).returncode == 0, timeout=45)
        roles = (ROOT / "ops" / "roles.sql").read_text()
        self.run([
            "podman", "exec", "-i", "--env", f"CONFIG_PASSWORD={self.password}",
            "--env", f"PLAN_PASSWORD={self.password}", "--env", f"REPLICATION_PASSWORD={self.password}",
            self.container, "psql", "-X", "-q", "-v", "ON_ERROR_STOP=1", "-U", "gpu_owner", "-d", "gpu_demo",
        ], data=roles)
        url = lambda user: f"postgres://{user}:{self.password}@127.0.0.1:{postgres_port}/gpu_demo?sslmode=disable"
        env = os.environ.copy()
        for key in ("GPU_LAB_LOG_QUERIES", "GPU_LAB_TEST_PLAN_ENDPOINT"):
            env.pop(key, None)
        env.update({"RUST_LOG": "info", "GPU_LAB_DIAGNOSTICS": "0"})
        self.run([str(self.args.control.resolve()), "init"],
                 env={**env, "DEMO_OWNER_URL": url("gpu_owner"), "RESET_PASSWORD": self.password})
        self.start("control", self.args.control, {
            **env, "DATABASE_URL": url("gpu_config"), "PLAN_DATABASE_URL": url("gpu_plan"),
            "RESET_DATABASE_URL": url("gpu_reset"), "DRASI_API_URL": self.api,
            "DRASI_SSE_URL": f"http://127.0.0.1:{self.sse_port}/events",
            "PUBLIC_ORIGIN": self.control, "INTERNAL_TOKEN": self.token,
            "LISTEN_ADDRESS": f"127.0.0.1:{self.control_port}",
        })
        self.runtime_env = {
            **env, "PGHOST": "127.0.0.1", "PGPORT": str(postgres_port),
            "REPLICATION_PASSWORD": self.password, "INTERNAL_TOKEN": self.token,
            "PLAN_ENDPOINT": self.control + "/internal/placement-plans",
            "GPU_STATE_DIR": str(self.state),
            "GPU_NATIVE_PLUGIN": str(self.args.native_plugin.resolve()),
            "GPU_NETWORK_PLUGIN": str(self.args.sse_plugin.resolve()),
            "LISTEN_ADDRESS": f"127.0.0.1:{self.api_port}",
            "GPU_SSE_HOST": "127.0.0.1", "GPU_SSE_PORT": str(self.sse_port),
        }

    def exercise(self):
        self.setup()
        gpu = self.sql("SELECT gpu_id FROM gpu_inventory ORDER BY gpu_id LIMIT 1;")
        UUID(gpu)
        script = "".join(self.mutation(gpu, number) + "SELECT pg_sleep(0.1);\n" for number in range(1, 31))
        with (self.evidence / "concurrent-writes.log").open("w") as log:
            writer = subprocess.Popen([
                "podman", "exec", "-i", self.container, "psql", "-X", "-qAt",
                "-v", "ON_ERROR_STOP=1", "-U", "gpu_owner", "-d", "gpu_demo",
            ], stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT, text=True)
            try:
                writer.stdin.write(script)
                writer.stdin.close()
                self.start("runtime", self.args.runtime, self.runtime_env)
                self.until("native GPU snapshot and concurrent atomic CDC", lambda: self.pair(gpu) == ("native-tx-30", 30))
                assert writer.wait(timeout=30) == 0, "concurrent writer failed"
            finally:
                if writer.poll() is None:
                    writer.terminate()
                    writer.wait(timeout=10)
        self.until("GPU live query-backed readiness", self.ready)
        self.slot_name = self.slot()
        for number in range(31, 36):
            self.sql(self.mutation(gpu, number))
            self.until(f"atomic native transaction {number}", lambda n=number: self.pair(gpu) == (f"native-tx-{n}", n), timeout=30)
        self.sql(f"BEGIN; UPDATE gpu_inventory SET name='must-not-appear' WHERE gpu_id='{UUID(gpu)}'; ROLLBACK;")
        assert self.pair(gpu) == ("native-tx-35", 35), "rolled-back work appeared in the query"
        self.stop("runtime")
        self.sql(self.mutation(gpu, 36))
        self.start("runtime", self.args.runtime, self.runtime_env)
        self.until("offline CDC before restored readiness", lambda: self.pair(gpu) == ("native-tx-36", 36) and self.ready())
        assert self.slot() == self.slot_name, "graceful restart replaced the managed slot"
        self.stop("runtime", abrupt=True)
        self.start("runtime", self.args.runtime, self.runtime_env)
        self.until("abrupt runtime recovery", lambda: self.pair(gpu) == ("native-tx-36", 36) and self.ready())
        assert self.slot() == self.slot_name, "abrupt restart replaced the managed slot"
        self.events.append({"event": "qualified-slot", "slot": self.slot_name})

    def cleanup(self):
        errors = []
        for name in ("runtime", "control"):
            try:
                self.stop(name)
            except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
                errors.append(str(error))
        if self.container is not None:
            logs = self.run(["podman", "logs", self.container], check=False)
            (self.evidence / "postgres.log").write_text(logs.stdout + logs.stderr)
            self.run(["podman", "rm", "--force", self.container])
            self.container = None
        if not errors:
            shutil.rmtree(self.state)
        if errors:
            raise RuntimeError("Cleanup failed: " + "; ".join(errors))

    def save(self, passed):
        (self.evidence / "observations.json").write_text(json.dumps(self.observations, indent=2) + "\n")
        (self.evidence / "events.json").write_text(json.dumps(self.events, indent=2) + "\n")
        (self.evidence / "result.json").write_text(json.dumps({
            "passed": passed, "slot": self.slot_name, "api": self.api,
            "control": self.control, "container_removed": self.container is None,
        }, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--control", type=Path, required=True)
    parser.add_argument("--native-plugin", type=Path, required=True)
    parser.add_argument("--sse-plugin", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True,
                        help="New directory; existing paths are never overwritten")
    proof = Qualification(parser.parse_args())
    passed = False
    try:
        proof.exercise()
        passed = True
    finally:
        try:
            proof.cleanup()
        except (RuntimeError, OSError, subprocess.TimeoutExpired):
            passed = False
            raise
        finally:
            proof.save(passed)
    print(f"Native GPU PostgreSQL qualification passed: {proof.evidence}", flush=True)


if __name__ == "__main__":
    main()
