#!/usr/bin/env python3
"""Replay real PostgreSQL changes in an isolated copy, retaining query logs and failures."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import time
from urllib.error import HTTPError, URLError
from urllib.request import urlopen
from uuid import uuid4

EXAMPLE = Path(__file__).resolve().parents[1]
INPUTS = {
    "input-clusters": "regional_clusters",
    "input-policies": "placement_policies",
    "input-data": "data_profiles",
    "input-gpus": "gpu_inventory",
    "input-settings": "gpu_telemetry",
    "input-workloads": "workload_requirements",
    "input-plan": "gpu_placements",
}
UI_KEYS = {
    "ui-gpus": "gpu_id", "ui-workloads": "workload_id",
    "ui-placements": "fleet_id", "ui-resilience": "fleet_id",
    "ui-decisions": "decision_id", "ui-status": "fleet_id",
    "ui-timeline": "event_id", "ui-clusters": "cluster_id", "ui-policy": "id",
}
QUERIES = [*INPUTS, "simulation-inputs", "scheduling-inputs", "plan-output",
           "runtime-context", *UI_KEYS]
TABLES = [*INPUTS.values(), "command_receipts", "demo_reset_state"]
PREFIX = "/api/v1/instances/gpu-demo"
ROLLBACK_NAME = "query-mutation-rollback-must-not-appear"


def same_rows(actual, expected):
    def ordered(rows):
        return sorted(json.dumps(row, sort_keys=True, separators=(",", ":")) for row in rows)
    return ordered(actual) == ordered(expected)


def unique_rows(query, rows):
    key = UI_KEYS.get(query)
    if key is not None:
        identities = [row.get(key) for row in rows]
        assert all(isinstance(value, str) and value for value in identities), (query, identities)
        assert len(set(identities)) == len(identities), f"{query}: duplicate {key}: {identities}"


def database_sql():
    entries = [
        f"'{table}', (SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text), '[]') "
        f"FROM {table} t)" for table in TABLES
    ]
    return "SELECT jsonb_build_object(" + ",".join(entries) + ");"


def recovery_readiness_regressed(before, after):
    components = {row["component_id"]: row for row in after["components"]}
    return (before["inputs_ready"] is True and after["inputs_ready"] is False
            and all(components.get(name, {}).get("status") == "running"
                    for name in ("postgres", *INPUTS))
            and components.get("simulator", {}).get("status") == "initialization-error")


class Probe:
    def __init__(self, args):
        checksum = subprocess.check_output(["cksum"], input=str(EXAMPLE).encode()).split()[0].decode()
        self.source = f"gpu-cluster-lab-{checksum}"
        self.project = f"{self.source}-mutations-{uuid4().hex[:12]}"
        self.evidence = (args.evidence or EXAMPLE / ".build" / self.project).resolve()
        self.evidence.mkdir(parents=True, exist_ok=True)
        if (self.evidence / "results.json").exists():
            raise RuntimeError(f"Refusing to overwrite an earlier run: {self.evidence}")
        self.env = os.environ.copy()
        self.env["GPU_LAB_DIAGNOSTICS"] = "0"
        self.env["GPU_LAB_TEST_PLAN_ENDPOINT"] = "http://control:5400/internal/placement-plans"
        self.env["GPU_LAB_TEST_CONTROL_IMAGE"] = self.image(f"{self.source}-control")
        self.env["GPU_LAB_TEST_RUNTIME_IMAGE"] = self.image(args.runtime_image or f"{self.source}-drasi")
        self.compose = [
            "docker", "compose", "--project-name", self.project, "--env-file", ".env",
            "-f", "compose.yaml", "-f", "ops/acceptance.compose.yaml",
            "-f", "ops/postgres-mutations.compose.yaml",
        ]
        self.results = []
        self.findings = []
        self.latest = {}
        self.started = False
        self.drasi = None
        self.control = None
        self.dump = args.snapshot

    def run(self, command, *, data=None, timeout=120):
        result = subprocess.run(command, cwd=EXAMPLE, env=self.env, input=data,
                                capture_output=True, timeout=timeout)
        if result.returncode:
            raise RuntimeError(f"{command[:8]} exited {result.returncode}: "
                               f"{result.stderr.decode(errors='replace')}")
        return result.stdout

    def image(self, name):
        return subprocess.check_output(
            ["docker", "image", "inspect", name, "--format", "{{.Id}}"], text=True).strip()

    def save(self, name, value):
        (self.evidence / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")

    def event(self, kind, **detail):
        with (self.evidence / "events.ndjson").open("a") as output:
            output.write(json.dumps({"time_ms": time.time_ns() // 1_000_000,
                                     "kind": kind, **detail}) + "\n")

    def sql(self, statement, *, variables=None, mutation=False, source=False):
        assert not (source and mutation), "The presenter is read-only"
        command = (["docker", "exec", "-i", f"{self.source}-postgres-1"] if source
                   else [*self.compose, "exec", "-T", "postgres"])
        command += ["psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1",
                    "-U", "gpu_config" if mutation else "gpu_owner", "-d", "gpu_demo"]
        for key, value in (variables or {}).items():
            command += ["-v", f"{key}={value}"]
        if mutation:
            self.event("sql", statement=statement, variables=variables or {})
        return self.run(command, data=statement.encode()).decode().strip()

    def database(self, *, source=False):
        return json.loads(self.sql(database_sql(), source=source))

    def response(self, path, *, control=False):
        base = self.control if control else self.drasi
        try:
            with urlopen(base + path, timeout=15) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.load(error)

    def rows(self, query):
        status, body = self.response(f"{PREFIX}/queries/{query}/results")
        assert status == 200 and body.get("success") is True, (query, status, body)
        rows = body["data"]
        assert isinstance(rows, list), (query, body)
        unique_rows(query, rows)
        self.latest[query] = rows
        return rows

    def until(self, description, check, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if check():
                return
            time.sleep(0.2)
        raise AssertionError(f"Timed out after {timeout}s: {description}")

    def mirrored(self):
        expected = self.database()
        for query, table in INPUTS.items():
            rows = self.rows(query)
            actual = [record for row in rows for record in row["records"]]
            projected = [{key: value for key, value in row.items()
                          if key not in ("updated_at", "committed_at")} for row in expected[table]]
            if not same_rows(actual, projected):
                self.latest["mirror_mismatch"] = {"query": query, "actual": actual,
                                                  "expected": projected}
                return False
        self.latest.pop("mirror_mismatch", None)
        return True

    def mirror(self):
        self.until("all seven input queries exactly match PostgreSQL rows", self.mirrored)

    def snapshot(self, label):
        self.event("snapshot", label=label)
        self.save(f"{label}-database.json", self.database())
        for query in QUERIES:
            try:
                status, body = self.response(f"{PREFIX}/queries/{query}/results")
                observation = {"http_status": status, **body}
            except URLError as error:
                observation = {"http_status": None, "transport_error": str(error)}
                self.event("snapshot-transport-error", label=label, query=query, error=str(error))
            self.save(f"{label}-{query}.json", observation)

    def confirmed(self, expected_count=None):
        def check():
            plans = self.rows("ui-placements")
            if len(plans) != 1:
                return False
            plan = plans[0]
            workloads = self.rows("ui-workloads")
            count = sum(row["replicas"] for row in workloads)
            return (plan["status"] == "confirmed"
                    and (expected_count is None or count == expected_count)
                    and plan["desired_plan_version"] == plan["applied_plan_version"]
                    == plan["confirmed_plan_version"]
                    and len(plan["desired"]) == count
                    and len({row["id"] for row in plan["desired"]}) == count
                    and all(row["ready_replicas"] == row["replicas"]
                            and row["running_replicas"] == row["replicas"] for row in workloads))
        self.until("saved/applied/confirmed plan and every replica count agree", check)
        self.mirror()

    def reset(self):
        self.event("isolated-reset", scenario="baseline")
        output = self.run([*self.compose, "exec", "-T", "control", "sh", "-eu", "-c",
                           'curl --silent --show-error --fail-with-body --max-time 300 '
                           '-X POST -H "Authorization: Bearer $INTERNAL_TOKEN" '
                           'http://127.0.0.1:5400/api/demo/presets/baseline'], timeout=320)
        assert json.loads(output)["status"] == "ready"
        self.confirmed(8)

    def target(self):
        return self.rows("ui-gpus")[0]["gpu_id"]

    def update_settings(self, gpu, assignments):
        self.sql(f"UPDATE gpu_telemetry SET {assignments} WHERE gpu_id=:'gpu';",
                 variables={"gpu": gpu}, mutation=True)
        self.mirror()

    def refresh_urls(self):
        for service, port, attribute in [("drasi", "8080", "drasi"), ("control", "5400", "control")]:
            address = self.run([*self.compose, "port", service, port]).decode().strip()
            assert address.startswith("127.0.0.1:"), address
            setattr(self, attribute, "http://" + address)
        self.event("endpoints", drasi=self.drasi, control=self.control)

    def boot(self):
        self.save("images.json", {key: self.env[key] for key in
                                 ("GPU_LAB_TEST_CONTROL_IMAGE", "GPU_LAB_TEST_RUNTIME_IMAGE")})
        self.save("presenter-before.json", self.database(source=True))
        if self.dump is None:
            self.dump = self.evidence / "presenter.dump"
            self.dump.write_bytes(self.run(
                ["docker", "exec", f"{self.source}-postgres-1", "pg_dump",
                 "-U", "gpu_owner", "-d", "gpu_demo", "--format=custom"]))
        self.started = True
        self.run([*self.compose, "up", "--no-build", "--wait", "postgres"])
        self.sql("CREATE ROLE gpu_reset NOLOGIN;")
        self.run([*self.compose, "exec", "-T", "postgres", "pg_restore",
                  "-U", "gpu_owner", "-d", "gpu_demo", "--clean", "--if-exists",
                  "--no-owner", "--exit-on-error"], data=self.dump.read_bytes())
        self.save("restored-before-start.json", self.database())
        self.run([*self.compose, "up", "--no-build", "-d", "control"])
        self.refresh_urls()

        def started():
            try:
                status, body = self.response(f"{PREFIX}/reactions")
            except URLError as error:
                self.event("startup-connection", error=str(error))
                return False
            return status == 200 and any(row["id"] == "gpu-query-log" and row["status"] == "Running"
                                        for row in body.get("data") or [])
        self.until("real gpu-query-log reaction is running", started, timeout=120)
        self.mirror()
        self.snapshot("restored")

    def case(self, name, action, *, reset=True):
        self.event("case-start", name=name)
        try:
            if reset:
                self.reset()
            action()
            self.mirror()
            for query in UI_KEYS:
                self.rows(query)
            self.snapshot(name)
        except AssertionError as error:
            self.results.append({"name": name, "status": "failed", "error": str(error)})
            self.save("results.json", self.results)
            self.save(f"{name}-last-observations.json", self.latest)
            self.snapshot(f"{name}-failure")
            print(f"FAIL {name}: {error}", flush=True)
        else:
            self.results.append({"name": name, "status": "passed"})
            print(f"PASS {name}", flush=True)
        self.event("case-end", **self.results[-1])
        self.save("results.json", self.results)

    def current_recovery(self):
        self.sql("UPDATE gpu_telemetry SET powered_on=true, reporting_enabled=true;", mutation=True)
        self.confirmed()

    def no_op_and_rollback(self):
        before = self.database()
        self.sql("UPDATE gpu_telemetry SET background_compute_units=background_compute_units;",
                 mutation=True)
        self.sql("BEGIN; UPDATE workload_requirements SET name=:'name' "
                 "WHERE workload_id=(SELECT workload_id FROM workload_requirements ORDER BY workload_id LIMIT 1); "
                 "SELECT pg_sleep(2); ROLLBACK;",
                 variables={"name": ROLLBACK_NAME}, mutation=True)
        after = self.database()
        assert after["gpu_telemetry"] == before["gpu_telemetry"], "No-op changed revisions/timestamps"
        assert after["workload_requirements"] == before["workload_requirements"], "Rollback leaked"
        self.confirmed()

    def workload_lifecycle(self):
        workload = str(uuid4())
        params = {"id": workload}
        self.sql("INSERT INTO workload_requirements(workload_id,name,model_ref,profile_id,data_profile_id,"
                 "purpose,replicas,memory_mib_per_replica,compute_units_per_replica,allowed_gpu_models,"
                 "spread_across_domains) VALUES (:'id','sql-probe','BAAI/bge-m3','embeddings-v1',"
                 "'demo-open','demo',1,4096,20,'[\"NVIDIA H100 NVL\"]',true);",
                 variables=params, mutation=True)
        self.confirmed(9)
        for assignments, count in [
            ("name='sql-probe-renamed', replicas=2", 10),
            ("profile_id='reranker-v1', model_ref='BAAI/bge-reranker-v2-m3', "
             "compute_units_per_replica=25", 10),
            ("replicas=0", 8),
        ]:
            self.sql(f"UPDATE workload_requirements SET {assignments} WHERE workload_id=:'id';",
                     variables=params, mutation=True)
            self.mirror()
            self.confirmed(count)
        self.sql("DELETE FROM workload_requirements WHERE workload_id=:'id';",
                 variables=params, mutation=True)
        self.confirmed(8)
        assert all(row["workload_id"] != workload for row in self.rows("ui-workloads"))

    def metadata_updates(self):
        state = self.database()
        for table, key, field in [
            ("regional_clusters", "cluster_id", "name"),
            ("gpu_inventory", "gpu_id", "name"),
            ("data_profiles", "data_profile_id", "authority_ref"),
        ]:
            row = state[table][0]
            for value in [row[field] + "-sql-probe", row[field]]:
                self.sql(f"UPDATE {table} SET {field}=:'value' WHERE {key}=:'id';",
                         variables={"id": row[key], "value": value}, mutation=True)
                self.mirror()
                self.confirmed(8)

    def reporting_expiry(self):
        gpu = self.target()
        self.update_settings(gpu, "reporting_enabled=false")
        self.until("a real five-second report deadline expires", lambda: any(
            row["gpu_id"] == gpu and row["health"] == "unreachable" for row in self.rows("ui-gpus")))
        expired = next(row for row in self.rows("ui-gpus") if row["gpu_id"] == gpu)
        assert time.time_ns() // 1_000_000 >= expired["report_time_ms"] + 5000, "Report expired early"
        time.sleep(1)
        assert next(row for row in self.rows("ui-gpus") if row["gpu_id"] == gpu)["report_time_ms"] == expired["report_time_ms"]
        self.update_settings(gpu, "reporting_enabled=true")
        self.until("fresh telemetry resumes", lambda: any(
            row["gpu_id"] == gpu and row["health"] == "healthy"
            and row["report_time_ms"] > expired["report_time_ms"] for row in self.rows("ui-gpus")))
        self.confirmed()

    def background_load(self):
        gpu = self.target()
        self.update_settings(gpu, "background_compute_units=21, background_memory_mib=1024")
        self.until("measurements reflect actual new settings and revisions", lambda: any(
            row["gpu_id"] == gpu and row["reported_background_compute_units"] == 21
            and row["reported_background_memory_requested_mib"] == 1024
            and row["sample_telemetry_revision"] == row["telemetry_revision"]
            for row in self.rows("ui-gpus")))
        self.confirmed()

    def power_cycle(self):
        gpu = self.target()
        self.update_settings(gpu, "powered_on=false")
        self.until("powered-off GPU has no execution", lambda: all(
            row["gpu_id"] != gpu for plan in self.rows("ui-placements") for row in plan["actual"]))
        self.confirmed()
        assert all(row["gpu_id"] != gpu for row in self.rows("ui-placements")[0]["desired"])
        self.update_settings(gpu, "powered_on=true")
        self.confirmed()
        for _ in range(3):
            self.update_settings(gpu, "powered_on=false")
            self.update_settings(gpu, "powered_on=true")
        self.until("final power-on produces current telemetry", lambda: any(
            row["gpu_id"] == gpu and row["powered_on"] and row["health"] == "healthy"
            and row["sample_telemetry_revision"] == row["telemetry_revision"]
            for row in self.rows("ui-gpus")))
        self.confirmed()

    def infeasible_restart(self):
        saved = self.database()["gpu_placements"]
        self.sql("UPDATE gpu_telemetry SET powered_on=false WHERE gpu_id IN "
                 "(SELECT gpu_id FROM gpu_inventory WHERE gpu_index=0);", mutation=True)
        self.mirror()
        self.until("infeasible placement is explicit", lambda: any(
            component["component_id"] == "placement" and component["status"] == "infeasible"
            for row in self.rows("ui-status") for component in row["components"]))
        assert self.database()["gpu_placements"] == saved, "Infeasibility saved a partial plan"
        self.snapshot("infeasible-before-restart")
        before = self.rows("ui-status")[0]
        old_epoch = before["observation_epoch"]
        self.run([*self.compose, "stop", "drasi"])
        self.check_stopped("drasi", "infeasible-restart")
        self.run([*self.compose, "start", "drasi"])
        self.refresh_urls()

        def restarted():
            try:
                status, body = self.response(f"{PREFIX}/queries/ui-status/results")
            except URLError as error:
                self.event("restart-connection", error=str(error))
                return False
            return (status == 200 and len(body.get("data") or []) == 1
                    and body["data"][0]["observation_epoch"] not in ("", old_epoch))
        self.until("fresh query epoch after invalid-plan restart", restarted, timeout=120)
        self.mirror()
        self.until("restarted simulator observes the infeasible saved plan", lambda: any(
            component["component_id"] == "simulator"
            and component["status"] in ("initialization-error", "application-rejected")
            and component["error"] == "ineligible target"
            for row in self.rows("ui-status") for component in row["components"]))
        self.snapshot("infeasible-after-restart")
        after = self.rows("ui-status")[0]
        if recovery_readiness_regressed(before, after):
            finding = {
                "id": "infeasible-restart-recovery-lockout",
                "detail": "Restarting with an infeasible saved plan changes inputs_ready from true "
                          "to false despite running PostgreSQL/input queries. The current UI uses "
                          "this flag to disable corrective power, workload and policy controls.",
                "reproduction": [
                    "Reset the isolated copy to baseline.",
                    "Set powered_on=false for gpu_index=0 on every VM and await infeasibility.",
                    "Restart only the Drasi runtime, preserving PostgreSQL and the saved plan.",
                    "Observe simulator initialization-error, inputs_ready=false and disabled recovery controls.",
                ],
                "before_epoch": before["observation_epoch"],
                "after_epoch": after["observation_epoch"],
                "sql_recovery": "UPDATE gpu_telemetry SET powered_on=true;",
            }
            self.findings.append(finding)
            self.save("findings.json", self.findings)
            self.event("finding", **finding)
            print(f"FINDING {finding['id']}: {finding['detail']}", flush=True)
        assert self.database()["gpu_placements"] == saved, "Restart replaced an infeasible plan"
        self.sql("UPDATE gpu_telemetry SET powered_on=true;", mutation=True)
        self.confirmed()

    def policy_reauthorization(self):
        saved = self.database()["gpu_placements"]
        self.sql("UPDATE placement_policies SET allowed_regions='[]' WHERE policy_id='demo-permissive';",
                 mutation=True)
        self.mirror()
        def fenced():
            workloads = self.rows("ui-workloads")
            return len(workloads) == 4 and all(
                row["running_replicas"] == 0 and row["fenced_replicas"] == row["replicas"]
                for row in workloads)
        self.until("policy revocation fences all running replicas", fenced)
        assert self.database()["gpu_placements"] == saved, "Denied placement overwrote the saved plan"
        self.sql("UPDATE placement_policies SET allowed_regions='[\"*\"]' "
                 "WHERE policy_id='demo-permissive';", mutation=True)
        self.confirmed()

    def check_stopped(self, service, label):
        container = self.run([*self.compose, "ps", "--all", "--quiet", service]).decode().strip()
        state = json.loads(self.run(["docker", "inspect", "--format", "{{json .State}}", container]))
        self.save(f"{label}-{service}-stop.json", state)
        spec = importlib.util.spec_from_file_location("check_stopped", EXAMPLE / "ops/check-stopped.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        assert module.validate(state), f"{service} did not exit cleanly: {state}"

    def finish(self):
        if not self.started:
            return
        try:
            self.run([*self.compose, "stop"], timeout=300)
            logs = self.run([*self.compose, "logs", "--no-color", "--timestamps"])
            (self.evidence / "services.log").write_bytes(logs)
            if self.drasi is not None:
                self.run([*self.compose, "cp", "drasi:/app/source-manifest.json",
                          str(self.evidence / "source-manifest.json")])
                for service in ("control", "drasi", "postgres"):
                    self.check_stopped(service, "final")
            text = logs.decode(errors="replace")
            observed = set(re.findall(r"\[gpu-query-log\] Query '([^']+)'", text))
            self.save("logged-queries.json", sorted(observed))
            if self.drasi is not None:
                assert set(QUERIES) <= observed, f"Missing real query log output: {set(QUERIES) - observed}"
            assert ROLLBACK_NAME not in text, "Uncommitted SQL update appeared in reaction logs"
        finally:
            self.run([*self.compose, "down", "--volumes"], timeout=180)
            self.save("presenter-after.json", self.database(source=True))
            before = json.loads((self.evidence / "presenter-before.json").read_text())
            after = json.loads((self.evidence / "presenter-after.json").read_text())
            self.save("presenter-comparison.json", {"equal": before == after,
                                                   "changed_tables": [table for table in TABLES
                                                                      if before[table] != after[table]]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-image", help="Locally built logging runtime; presenter is not redeployed")
    parser.add_argument("--snapshot", type=Path, help="Existing pg_dump custom archive to reproduce")
    parser.add_argument("--evidence", type=Path, help="Output directory; defaults to .build/<isolated-project>")
    args = parser.parse_args()
    os.umask(0o077)
    probe = Probe(args)
    print(f"Isolated PostgreSQL mutation project: {probe.project}\nEvidence: {probe.evidence}", flush=True)
    try:
        probe.boot()
        probe.case("current-power-recovery", probe.current_recovery, reset=False)
        for name, action in [
            ("no-op-and-rollback", probe.no_op_and_rollback),
            ("workload-lifecycle", probe.workload_lifecycle),
            ("metadata-updates", probe.metadata_updates),
            ("reporting-expiry", probe.reporting_expiry),
            ("background-load", probe.background_load),
            ("power-cycle", probe.power_cycle),
            ("infeasible-restart", probe.infeasible_restart),
            ("policy-reauthorization", probe.policy_reauthorization),
        ]:
            probe.case(name, action)
    finally:
        probe.finish()
    failed = sum(result["status"] == "failed" for result in probe.results)
    probe.save("findings.json", probe.findings)
    print(f"{len(probe.results) - failed} query/data cases passed, {failed} failed, "
          f"{len(probe.findings)} recovery findings. Evidence: {probe.evidence}", flush=True)
    return 1 if failed or probe.findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
