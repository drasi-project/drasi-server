#!/usr/bin/env python3
"""Qualify the real MySQL native Server consumer using only an owned fresh database."""

import argparse
import hashlib
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
from uuid import uuid4

EXAMPLE = Path(__file__).resolve().parent
LIBRARIES = (
    "drasi_computation_source_mysql",
    "drasi_computation_bootstrap_mysql",
    "drasi_computation_sink_storedproc_mysql",
)


class Qualification:
    def __init__(self, args):
        self.args = args
        self.evidence = args.evidence.resolve()
        self.evidence.mkdir(parents=True, exist_ok=False)
        self.password = secrets.token_hex(32)
        self.container = None
        self.server = None
        self.server_log = None
        self.observations = {}
        self.plugins = self.evidence / "plugins"
        self.plugins.mkdir()
        suffix = ".dylib" if os.uname().sysname == "Darwin" else ".so"
        hashes = {}
        for name in LIBRARIES:
            library = args.plugins.resolve() / f"lib{name}{suffix}"
            if not library.is_file():
                raise ValueError(f"Build the matching native library first: {library}")
            shutil.copy2(library, self.plugins / library.name)
            hashes[library.name] = hashlib.sha256(library.read_bytes()).hexdigest()
        self.observations["libraries"] = hashes

    def run(self, command, *, data=None, check=True):
        result = subprocess.run(
            command, input=data, text=True, capture_output=True, timeout=120
        )
        if check and result.returncode:
            raise RuntimeError(
                f"{Path(command[0]).name} failed: "
                + result.stderr.replace(self.password, "[test-password]")[-8000:]
            )
        return result

    def sql(self, statement):
        return self.run(
            [
                "podman", "exec", "-i", "--env", f"MYSQL_PWD={self.password}",
                self.container, "mysql", "-uroot", "--batch", "--skip-column-names",
                "--raw", "test",
            ],
            data=statement,
        ).stdout.strip()

    def record_database_ownership(self):
        inspected = json.loads(self.run(["podman", "inspect", self.container]).stdout)[0]
        label = inspected.get("Config", {}).get("Labels", {}).get("drasi.qualification", "")
        if inspected["Id"] != self.container or not label.startswith("native-mysql-consumer-"):
            raise RuntimeError("MySQL fixture ownership changed; refusing cleanup")
        volumes = []
        for mount in inspected["Mounts"]:
            if mount["Type"] != "volume":
                continue
            name = mount["Name"]
            volume = json.loads(self.run(["podman", "volume", "inspect", name]).stdout)[0]
            if (len(name) != 64 or any(char not in "0123456789abcdef" for char in name)
                    or volume.get("Anonymous") is not True):
                raise RuntimeError("Unexpected non-anonymous MySQL fixture volume")
            volumes.append(name)
        if not volumes:
            raise RuntimeError("MySQL fixture has no recorded data volume")
        self.observations["database_ownership"] = {
            "container": self.container, "label": label,
            "mounts": inspected["Mounts"], "volumes": volumes, "removed": False,
        }
        self.write_database_ownership()

    def write_database_ownership(self):
        (self.evidence / "resource-ownership.json").write_text(
            json.dumps(self.observations["database_ownership"], indent=2) + "\n"
        )

    def until(self, description, check, timeout=45):
        deadline = time.monotonic() + timeout
        last_error = None
        while time.monotonic() < deadline:
            if self.server is not None and self.server.poll() is not None:
                raise RuntimeError(
                    f"Server exited {self.server.returncode}; see {self.server_log.name}"
                )
            try:
                if check():
                    print(f"PASS {description}", flush=True)
                    return
            except HTTPError:
                raise
            except (URLError, ConnectionError, TimeoutError) as error:
                last_error = error
            time.sleep(0.1)
        raise AssertionError(f"Timed out: {description}; last transport error: {last_error}")

    def setup(self):
        name = f"native-mysql-consumer-{uuid4().hex[:12]}"
        container = self.run([
            "podman", "run", "--detach", "--name", name,
            "--label", f"drasi.qualification={name}",
            "--publish", "127.0.0.1::3306",
            "--env", f"MYSQL_ROOT_PASSWORD={self.password}",
            "--env", "MYSQL_DATABASE=test",
            "mysql:8.4", "--log-bin=mysql-bin", "--binlog-format=ROW",
            "--binlog-row-image=FULL", "--binlog-row-metadata=FULL", "--server-id=1",
        ]).stdout.strip()
        if len(container) != 64 or any(char not in "0123456789abcdef" for char in container):
            raise RuntimeError("Podman did not return a concrete owned container ID")
        self.container = container
        self.record_database_ownership()
        self.until("fresh isolated MySQL", lambda: self.run([
            "podman", "exec", "--env", f"MYSQL_PWD={self.password}", self.container,
            "mysqladmin", "ping", "-h", "127.0.0.1", "-uroot",
        ], check=False).returncode == 0, timeout=90)
        address = self.run(["podman", "port", self.container, "3306/tcp"]).stdout.strip()
        if not address.startswith("127.0.0.1:"):
            raise RuntimeError(f"MySQL fixture is not loopback-bound: {address}")
        self.database_port = int(address.rsplit(":", 1)[1])
        self.sql("""
            CREATE TABLE items(id INT PRIMARY KEY,value VARCHAR(80) NOT NULL);
            INSERT INTO items VALUES(1,'initial'),(2,'selected');
            CREATE TABLE effects(query_id VARCHAR(64),id INT,value TEXT,
                PRIMARY KEY(query_id,id));
            CREATE TABLE audit(query_id VARCHAR(64),operation VARCHAR(16),id INT);
            DELIMITER //
            CREATE PROCEDURE effect(IN q VARCHAR(64),IN op VARCHAR(16),IN i INT,IN v TEXT)
            BEGIN
                INSERT INTO audit VALUES(q,op,i);
                IF op='DELETE' THEN DELETE FROM effects WHERE query_id=q AND id=i;
                ELSE INSERT INTO effects VALUES(q,i,v)
                    ON DUPLICATE KEY UPDATE value=v;
                END IF;
            END //
            DELIMITER ;
        """)
        generated = self.evidence / "generated.yaml"
        repeated = self.evidence / "repeated.yaml"
        for output in [generated, repeated]:
            self.run([
                str(self.args.generator.resolve()), "generate", str(self.plugins),
                str(EXAMPLE / "recipe.json"), str(output),
            ])
        if generated.read_bytes() != repeated.read_bytes():
            raise AssertionError("canonical MySQL generation is not reproducible")
        checked_in = EXAMPLE / "server.yaml"
        if generated.read_bytes() != checked_in.read_bytes():
            raise AssertionError("checked-in MySQL definition differs from current factory metadata")
        config = json.loads(self.run([
            str(self.args.generator.resolve()), "inspect", str(generated),
        ]).stdout)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            self.api_port = listener.getsockname()[1]
        config["port"] = self.api_port
        self.config = self.evidence / "server.json"
        self.config.write_text(json.dumps(config, indent=2) + "\n")
        self.api = f"http://127.0.0.1:{self.api_port}"
        self.observations["canonical_generation"] = "repeat and checked-in definition match"

    def environment(self):
        source = {
            "connection": {
                "host": "127.0.0.1", "port": self.database_port, "database": "test",
                "user": "root", "password": self.password, "ssl_mode": "disabled",
                "tls_ca_pem": None, "server_id": 812, "heartbeat_interval_ms": 100,
            },
            "tables": ["items"], "start": {"mode": "snapshot", "lock_timeout_ms": 5000},
            "output": "changes", "replay": "server",
            "transactions": {"max_changes": 64, "max_bytes": 1048576, "max_duration_ms": 10000},
            "max_protocol_bytes": 1048576, "io_timeout_ms": 10000,
        }
        templates = {}
        for name, row, value in [
            ("added", "after", "{{param after.value}}"),
            ("updated", "after", "{{param after.value}}"),
            ("deleted", "before", "NULL"),
        ]:
            templates[name] = {"template":
                "CALL effect({{param output.alias}},{{param operation}},"
                + "{{param " + row + ".id}}," + value + ")"}
        sink = {
            "hostname": "127.0.0.1", "port": self.database_port, "database": "test",
            "user": "root", "password": self.password,
            "outputs": {
                name: {"component": name, "port": "out"}
                for name in ("all-items", "selected-items")
            },
            "default_template": templates,
        }
        return {
            **os.environ,
            "DRASI_MYSQL_SOURCE_SETTINGS": json.dumps(source),
            "DRASI_MYSQL_SINK_SETTINGS": json.dumps(sink),
        }

    def start_server(self, phase):
        self.server_log = (self.evidence / f"server-{phase}.log").open("w")
        try:
            self.server = subprocess.Popen([
                str(self.args.server.resolve()), "--config", str(self.config),
                "--plugins-dir", str(self.plugins), "--skip-verification",
            ], cwd=self.evidence, env=self.environment(),
                stdout=self.server_log, stderr=subprocess.STDOUT)
        except OSError:
            self.server_log.close()
            raise

    def rows(self, query):
        url = f"{self.api}/api/v1/instances/mysql/queries/{query}/results"
        with urlopen(url, timeout=5) as response:
            body = json.load(response)
        if body.get("success") is not True or not isinstance(body.get("data"), list):
            raise AssertionError(body)
        return sorted(body["data"], key=lambda row: row["id"])

    def expect_rows(self, all_items, selected):
        return self.rows("all-items") == all_items and self.rows("selected-items") == selected

    def stop_server(self):
        if self.server is None:
            return
        server = self.server
        forced = False
        try:
            if server.poll() is None:
                server.send_signal(signal.SIGTERM)
                try:
                    server.wait(timeout=60)
                except subprocess.TimeoutExpired:
                    forced = True
                    server.kill()
                    server.wait(timeout=10)
            if forced or server.returncode != 0:
                raise RuntimeError(
                    f"Server cleanup failed ({server.returncode}); see {self.server_log.name}"
                )
        finally:
            self.server = None
            self.server_log.close()

    def prove(self):
        self.start_server("initial")
        initial = [{"id": 1, "value": "initial"}, {"id": 2, "value": "selected"}]
        self.until("one source initializes two real queries",
                   lambda: self.expect_rows(initial, initial[1:]))
        self.observations["initial"] = initial
        self.sql("""
            BEGIN;
            UPDATE items SET value='updated' WHERE id=1;
            INSERT INTO items VALUES(3,'third');
            DELETE FROM items WHERE id=2;
            COMMIT;
        """)
        live = [{"id": 1, "value": "updated"}, {"id": 3, "value": "third"}]
        self.until("multi-statement live update/insert/delete",
                   lambda: self.expect_rows(live, live[1:]))
        self.until("native parameterized procedure delivery",
                   lambda: self.sql("SELECT COUNT(*) FROM audit") == "5")
        assert self.sql(
            "SELECT query_id,id,value FROM effects ORDER BY query_id,id"
        ) == "all-items\t1\tupdated\nall-items\t3\tthird\nselected-items\t3\tthird"
        self.observations["live"] = live
        self.sql("""
            BEGIN; UPDATE items SET value='rolled-back' WHERE id=1;
            INSERT INTO items VALUES(4,'rolled-back'); ROLLBACK;
            UPDATE items SET value='barrier' WHERE id=3;
        """)
        barrier = [{"id": 1, "value": "updated"}, {"id": 3, "value": "barrier"}]
        self.until("rollback exclusion followed by committed barrier",
                   lambda: self.expect_rows(barrier, barrier[1:]))
        self.until("exact post-rollback procedure effects",
                   lambda: self.sql("SELECT COUNT(*) FROM audit") == "7")
        self.stop_server()
        self.until("server-observed session retirement", lambda: self.sql(
            "SELECT COUNT(*) FROM information_schema.PROCESSLIST "
            "WHERE USER='root' AND ID<>CONNECTION_ID()"
        ) == "0")
        self.sql("UPDATE items SET value='offline' WHERE id=1")
        self.start_server("restart")
        restarted = [{"id": 1, "value": "offline"}, {"id": 3, "value": "barrier"}]
        self.until("fresh volatile initialization after offline write",
                   lambda: self.expect_rows(restarted, restarted[1:]))
        self.sql("UPDATE items SET value='resumed' WHERE id=1")
        self.until("native delivery after restart",
                   lambda: self.sql("SELECT COUNT(*) FROM audit") == "8")
        assert self.sql(
            "SELECT value FROM effects WHERE query_id='all-items' AND id=1"
        ) == "resumed"
        self.observations["restart"] = restarted
        self.stop_server()
        self.until("server-observed restart cleanup", lambda: self.sql(
            "SELECT COUNT(*) FROM information_schema.PROCESSLIST "
            "WHERE USER='root' AND ID<>CONNECTION_ID()"
        ) == "0")
        self.observations["server_cleanup"] = "no owned MySQL sessions remain"

    def cleanup(self):
        errors = []
        try:
            self.stop_server()
        except (RuntimeError, OSError, subprocess.SubprocessError) as error:
            errors.append(str(error))
        if self.container is not None:
            container = self.container
            try:
                if "database_ownership" not in self.observations:
                    self.record_database_ownership()
                self.run(["podman", "rm", "--force", "--volumes", container])
                self.container = None
                ownership = self.observations["database_ownership"]
                for volume in ownership["volumes"]:
                    if self.run(["podman", "volume", "exists", volume], check=False).returncode != 1:
                        raise RuntimeError(
                            f"Owned MySQL volume {volume} remains; see resource-ownership.json"
                        )
                ownership["removed"] = True
                ownership["remaining_owned_volume_count"] = 0
                ownership["remaining_owned_volume_bytes"] = 0
                self.write_database_ownership()
                self.observations["container_cleanup"] = (
                    "removed only the owned fresh container and anonymous volumes"
                )
            except (RuntimeError, OSError, subprocess.SubprocessError) as error:
                errors.append(str(error))
        if errors:
            raise RuntimeError("; ".join(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--generator", type=Path, required=True)
    parser.add_argument("--plugins", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    qualification = Qualification(args)
    try:
        qualification.setup()
        qualification.prove()
    finally:
        qualification.cleanup()
        (qualification.evidence / "observations.json").write_text(
            json.dumps(qualification.observations, indent=2) + "\n"
        )


if __name__ == "__main__":
    main()
