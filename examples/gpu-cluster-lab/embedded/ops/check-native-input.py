#!/usr/bin/env python3
"""Qualify the native input owner in a fresh, disposable Compose project."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
from urllib.error import HTTPError, URLError
from urllib.request import urlopen
from uuid import UUID, uuid4

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "/api/v1/instances/gpu-demo"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser-script", type=Path)
    args = parser.parse_args()
    checksum = subprocess.check_output(["cksum"], input=str(ROOT.parent).encode()).split()[0].decode()
    source = f"gpu-cluster-lab-{checksum}"
    project = f"{source}-native-{uuid4().hex[:12]}"
    evidence = ROOT / ".build" / project
    evidence.mkdir(parents=True)
    env = os.environ.copy()
    env.pop("GPU_LAB_LOG_QUERIES", None)
    env["GPU_LAB_DIAGNOSTICS"] = "0"
    for service, key in [("control", "CONTROL"), ("drasi", "RUNTIME")]:
        env[f"GPU_LAB_TEST_{key}_IMAGE"] = subprocess.check_output(
            ["docker", "image", "inspect", f"{source}-{service}", "--format", "{{.Id}}"],
            text=True).strip()
    compose = ["docker", "compose", "--project-name", project, "--env-file", ".env",
               "-f", "compose.yaml", "-f", "ops/acceptance.compose.yaml",
               "-f", "ops/native-input.compose.yaml"]
    writer = None
    latest = {}

    def run(command, *, data=None, timeout=180):
        result = subprocess.run(command, cwd=ROOT, env=env, input=data, capture_output=True,
                                text=True, timeout=timeout)
        if result.returncode:
            raise RuntimeError(f"{command[:6]}: {result.stderr[-8000:]}")
        return result.stdout.strip()

    def sql(statement, **variables):
        command = [*compose, "exec", "-T", "postgres", "psql", "-X", "-qAt",
                   "-v", "ON_ERROR_STOP=1", "-U", "gpu_owner", "-d", "gpu_demo"]
        for key, value in variables.items():
            command.extend(["-v", f"{key}={value}"])
        return run(command, data=statement)

    def url(service, port):
        address = run([*compose, "port", service, str(port)])
        assert address.startswith("127.0.0.1:"), address
        return "http://" + address

    def get(base, path):
        try:
            with urlopen(base + path, timeout=10) as response:
                value = json.load(response)
        except HTTPError as error:
            value = json.load(error)
        latest[path] = value
        return value

    def until(description, check, timeout=120):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                if check():
                    print(f"PASS {description}", flush=True)
                    return
            except (URLError, ConnectionError, TimeoutError) as error:
                latest["transport_error"] = str(error)
            time.sleep(0.1)
        raise AssertionError(description)

    def slot():
        names = sql("SELECT slot_name FROM pg_replication_slots "
                    "WHERE database='gpu_demo' ORDER BY slot_name;").splitlines()
        assert len(names) == 1 and names[0].startswith("gpu_native_"), names
        UUID(names[0][len("gpu_native_"):])
        return names[0]

    def pair(drasi, gpu):
        body = get(drasi, PREFIX + "/queries/input-configuration/results")
        rows = body.get("data")
        if not isinstance(rows, list) or len(rows) != 1:
            return None
        records = rows[0]["records"]
        inventory = [r["value"] for r in records if r["table"] == "gpu_inventory"
                     and r["value"]["gpu_id"] == gpu]
        settings = [r["value"] for r in records if r["table"] == "gpu_telemetry"
                    and r["value"]["gpu_id"] == gpu]
        assert len(inventory) == len(settings) == 1
        name = inventory[0]["name"]
        demand = settings[0]["background_compute_units"]
        if name.startswith("native-tx-"):
            assert int(name[len("native-tx-"):]) == demand, (name, demand)
        return name, demand

    def mutation(gpu, number):
        return ("BEGIN; UPDATE gpu_inventory SET name='native-tx-" + str(number)
                + "' WHERE gpu_id='" + str(UUID(gpu)) + "'; "
                "UPDATE gpu_telemetry SET background_compute_units=" + str(number)
                + " WHERE gpu_id='" + str(UUID(gpu)) + "'; COMMIT;\n")

    try:
        run([*compose, "up", "--no-build", "--wait", "postgres"])
        run([*compose, "run", "--rm", "--no-deps", "migrate"])
        gpu = sql("SELECT gpu_id FROM gpu_inventory ORDER BY gpu_id LIMIT 1;")
        UUID(gpu)
        # Write across snapshot/live handover, then check the complete aggregate
        # while further multi-table transactions stream through it.
        script = "".join(mutation(gpu, n) + "SELECT pg_sleep(0.1);\n" for n in range(1, 31))
        with (evidence / "concurrent-writes.log").open("w") as log:
            writer = subprocess.Popen(
                [*compose, "exec", "-T", "postgres", "psql", "-X", "-qAt",
                 "-v", "ON_ERROR_STOP=1", "-U", "gpu_owner", "-d", "gpu_demo"],
                cwd=ROOT, env=env, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT,
                text=True)
            writer.stdin.write(script)
            writer.stdin.close()
            run([*compose, "up", "--no-build", "-d", "control"])
            drasi, control = url("drasi", 8080), url("control", 5400)
            until("cold snapshot and concurrent writes catch up",
                  lambda: pair(drasi, gpu) == ("native-tx-30", 30))
            assert writer.wait(timeout=30) == 0, "concurrent writer failed"
            writer = None
        until("native cold-start readiness", lambda: get(control, "/health/ready").get("ready") is True)
        original_slot = slot()
        for number in range(31, 36):
            sql(mutation(gpu, number))
            until(f"atomic multi-table transaction {number}",
                  lambda n=number: pair(drasi, gpu) == (f"native-tx-{n}", n), timeout=30)
        run([*compose, "stop", "drasi"])
        sql(mutation(gpu, 36))
        run([*compose, "up", "--no-build", "-d", "drasi"])
        drasi = url("drasi", 8080)
        until("offline writes recovered before readiness",
              lambda: get(control, "/health/ready").get("ready") is True
              and pair(drasi, gpu) == ("native-tx-36", 36))
        assert slot() == original_slot, "restart replaced or leaked the query-owned slot"
        run([*compose, "kill", "--signal", "SIGKILL", "drasi"])
        run([*compose, "up", "--no-build", "-d", "drasi"])
        drasi = url("drasi", 8080)
        until("abrupt process loss recovers committed state",
              lambda: get(control, "/health/ready").get("ready") is True
              and pair(drasi, gpu) == ("native-tx-36", 36))
        assert slot() == original_slot
        reset = run([*compose, "exec", "-T", "control", "sh", "-eu", "-c",
                     'curl --silent --show-error --fail-with-body --max-time 300 '
                     '-X POST -H "Authorization: Bearer $INTERNAL_TOKEN" '
                     'http://127.0.0.1:5400/api/demo/presets/baseline'], timeout=320)
        assert json.loads(reset)["status"] == "ready"
        assert slot() == original_slot, "reset replaced or leaked the native input owner"
        run([*compose, "run", "--rm", "--no-deps", "checks", "--smoke"])
        print(run([*compose, "run", "--rm", "--no-deps", "--entrypoint", "node",
                   "checks", "ops/check-admin-ui.mjs"]), flush=True)
        if args.browser_script:
            browser_env = {**env, "GPU_DEMO_URL": url("control", 5400)}
            subprocess.run(["node", str(args.browser_script.resolve())], env=browser_env,
                           cwd=ROOT, check=True, timeout=180)
        run([*compose, "stop", "drasi"])
        sql("SELECT pg_drop_replication_slot(:'slot');", slot=original_slot)
        run([*compose, "up", "--no-build", "-d", "drasi"])
        drasi = url("drasi", 8080)

        def missing_history_failed():
            logs = run([*compose, "logs", "--no-color", "--tail=150", "drasi"])
            if "native PostgreSQL source requires an existing slot in this database" not in logs:
                return False
            try:
                return get(drasi, "/health/ready").get("ready") is False
            except (URLError, ConnectionError, TimeoutError):
                return True

        until("missing retained slot fails closed", missing_history_failed)
        assert sql("SELECT count(*) FROM pg_replication_slots WHERE database='gpu_demo';") == "0"
        (evidence / "result.json").write_text(json.dumps({"passed": True, "slot": original_slot}) + "\n")
        print(f"Native input qualification passed: {evidence}", flush=True)
    finally:
        if writer is not None:
            try:
                writer.wait(timeout=30)
            except subprocess.TimeoutExpired:
                writer.terminate()
                writer.wait(timeout=10)
        (evidence / "observations.json").write_text(json.dumps(latest, indent=2) + "\n")
        try:
            (evidence / "services.log").write_text(run([*compose, "logs", "--no-color", "--tail=250"]))
        finally:
            run([*compose, "down", "--volumes"])


if __name__ == "__main__":
    main()
