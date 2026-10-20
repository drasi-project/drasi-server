#!/usr/bin/env python3
"""Run generated PostgreSQL Server examples on fresh databases, slots and instance state."""

import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError, URLError
from urllib.request import urlopen
from uuid import uuid4

EXAMPLES = Path(__file__).resolve().parents[1]
CASES = {
    "postgres-cdc-complete": "configs/02-sources/postgres-cdc-complete.yaml",
    "order-exception-handling": "configs/06-real-world-scenarios/order-exception-handling.yaml",
    "getting-started-server": "getting-started/server-config.yaml",
    **{f"getting-started-step-{step}": f"getting-started/configs/getting-started-step-{step}.yaml"
       for step in range(3, 7)},
}


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def identifier(value):
    return ".".join('"' + part.replace('"', '""') + '"' for part in value.split("."))


class Qualification:
    def __init__(self, args, libraries=None):
        self.args = args
        self.evidence = args.evidence.resolve()
        self.evidence.mkdir(parents=True, exist_ok=False)
        self.password = secrets.token_hex(32)
        self.container = None
        self.server = None
        self.server_log = None
        self.results = []
        self.alerts = []
        self.http = None
        self.http_thread = None
        self.plugin_dir = self.evidence / "plugins"
        self.plugin_dir.mkdir()
        suffix = ".dylib" if os.uname().sysname == "Darwin" else ".so"
        for library in libraries or [
            "drasi_computation_source_postgres", "drasi_computation_bootstrap_postgres",
            "drasi_reaction_log", "drasi_reaction_sse", "drasi_reaction_http",
        ]:
            source = args.plugins.resolve() / f"lib{library}{suffix}"
            if not source.is_file():
                raise ValueError(f"Build the matching local library first: {source}")
            shutil.copy2(source, self.plugin_dir / source.name)

    def run(self, command, *, data=None, env=None, check=True):
        result = subprocess.run(command, input=data, text=True, env=env,
                                capture_output=True, timeout=120)
        if check and result.returncode:
            raise RuntimeError(f"{Path(command[0]).name} failed: "
                               + result.stderr.replace(self.password, "[test-password]")[-8000:])
        return result

    def inspect(self, path):
        return json.loads(self.run([str(self.args.generator.resolve()), "inspect", str(path)]).stdout)

    def sql(self, database, statement):
        return self.run([
            "podman", "exec", "-i", self.container, "psql", "-X", "-qAt",
            "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", database,
        ], data=statement).stdout.strip()

    def until(self, description, check, timeout=45):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.server is not None and self.server.poll() is not None:
                raise RuntimeError(f"Server exited {self.server.returncode}: {self.server_log.name}")
            try:
                if check():
                    print(f"PASS {description}", flush=True)
                    return
            except HTTPError:
                raise
            except (URLError, ConnectionError, TimeoutError):
                pass
            time.sleep(0.1)
        raise AssertionError(f"Timed out: {description}")

    def setup(self, capture=True):
        name = f"postgres-native-consumers-{uuid4().hex[:12]}"
        self.container = self.run([
            "podman", "run", "--detach", "--name", name,
            "--label", f"drasi.qualification={name}",
            "--publish", "127.0.0.1::5432", "--tmpfs", "/var/lib/postgresql/data:rw",
            "--env", f"POSTGRES_PASSWORD={self.password}", "postgres:16-alpine",
            "-c", "wal_level=logical", "-c", "max_replication_slots=32",
            "-c", "max_wal_senders=32", "-c", "max_slot_wal_keep_size=-1",
        ]).stdout.strip()
        if len(self.container) != 64 or any(char not in "0123456789abcdef" for char in self.container):
            raise RuntimeError("Podman did not return a concrete owned container ID")
        self.until("fresh consumer database", lambda: self.run([
            "podman", "exec", self.container, "pg_isready", "-h", "127.0.0.1", "-U", "postgres",
        ], check=False).returncode == 0)
        address = self.run(["podman", "port", self.container, "5432/tcp"]).stdout.strip()
        assert address.startswith("127.0.0.1:"), address
        self.postgres_port = int(address.rsplit(":", 1)[1])
        if not capture:
            return
        alerts = self.alerts

        class Capture(BaseHTTPRequestHandler):
            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                alerts.append({"path": self.path, "body": self.rfile.read(length).decode()})
                body = b'{"ok":true}'
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args):
                return

        self.http = ThreadingHTTPServer(("127.0.0.1", 0), Capture)
        self.http_thread = threading.Thread(target=self.http.serve_forever)
        self.http_thread.start()

    def response(self, path):
        try:
            with urlopen(self.api + path, timeout=5) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.load(error)

    def rows(self, query):
        status, body = self.response(f"/api/v1/instances/{self.instance}/queries/{query}/results")
        if status != 200:
            return None
        assert body.get("success") is True and isinstance(body["data"], list), body
        return body["data"]

    def stop_server(self):
        if self.server is None:
            return
        process = self.server
        forced = False
        try:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:
                    process.wait(timeout=60)
                except subprocess.TimeoutExpired:
                    forced = True
                    process.kill()
                    process.wait(timeout=10)
            if forced or process.returncode != 0:
                raise RuntimeError(f"Server cleanup failed: {process.returncode}; log: {self.server_log.name}")
        finally:
            self.server = None
            self.server_log.close()

    def schema(self, name):
        if name == "postgres-cdc-complete":
            return """
                CREATE TABLE users(id INTEGER PRIMARY KEY,email TEXT,created_at TIMESTAMPTZ);
                CREATE TABLE orders(order_id INTEGER PRIMARY KEY,customer_id INTEGER,total INTEGER);
                CREATE TABLE products(sku TEXT PRIMARY KEY,name TEXT,quantity INTEGER);
                INSERT INTO users VALUES(1,'before@example.invalid',now());
                INSERT INTO orders VALUES(1,1,200);
                INSERT INTO products VALUES('low','Native product',5);
            """
        if name == "order-exception-handling":
            return """
                CREATE TABLE orders(order_id INTEGER PRIMARY KEY,customer_id INTEGER,total_amount INTEGER,status TEXT,created_at TIMESTAMPTZ,is_stale BOOLEAN);
                CREATE TABLE order_items(item_id INTEGER PRIMARY KEY,order_id INTEGER,product_id INTEGER,quantity INTEGER);
                CREATE TABLE payments(payment_id INTEGER PRIMARY KEY,order_id INTEGER,amount INTEGER,status TEXT,error_code TEXT,error_message TEXT);
                CREATE TABLE inventory(product_id INTEGER PRIMARY KEY,stock_quantity INTEGER);
                INSERT INTO orders VALUES(1,1,15000,'pending_approval',now(),false),(2,2,50,'pending',now(),true);
                INSERT INTO order_items VALUES(1,1,1,10);
                INSERT INTO payments VALUES(1,1,15000,'failed','DECLINED','initial failure');
                INSERT INTO inventory VALUES(1,5);
            """
        if name == "getting-started-server":
            return """
                CREATE TABLE message(messageid INTEGER PRIMARY KEY,"from" TEXT,message TEXT);
                INSERT INTO message VALUES(1,'Initial Sender','Hello World'),(2,'Other Sender','Other');
            """
        return """
            CREATE TABLE "Message"("MessageId" INTEGER PRIMARY KEY,"From" TEXT,"Message" TEXT);
            INSERT INTO "Message" VALUES(1,'Initial Sender','Hello World'),(2,'Other Sender','Other');
        """

    def standalone_setup(self, directory):
        # Redirect only the wrapper's fixed demo container to this owned fixture.
        # Compose must not touch the installed demo or its volume.
        adapter = directory / "owned-container-cli.py"
        adapter.write_text(
            "#!/usr/bin/env python3\nimport subprocess,sys\n"
            "args=sys.argv[1:]\n"
            "if args == ['compose','up','-d']: sys.exit(0)\n"
            "if not args or args[0] != 'exec' or 'getting-started-postgres' not in args:\n"
            "    raise SystemExit('unexpected setup command')\n"
            f"args[args.index('getting-started-postgres')]={self.container!r}\n"
            "data=sys.stdin.buffer.read() if '-i' in args else None\n"
            "sys.exit(subprocess.run(['podman',*args],input=data).returncode)\n"
        )
        adapter.chmod(0o700)
        env = {**os.environ, "CONTAINER_CLI": str(adapter)}
        command = ["bash", str(EXAMPLES / "getting-started/scripts/setup-database.sh")]
        first = self.run(command, env=env, data="")
        retained = self.sql("getting_started", """
            SELECT confirmed_flush_lsn FROM pg_replication_slots WHERE slot_name='drasi_getting_started_slot';
            SELECT row_to_json(m) FROM message AS m ORDER BY messageid;
        """)
        second = self.run(command, env=env, data="")
        assert self.sql("getting_started", """
            SELECT confirmed_flush_lsn FROM pg_replication_slots WHERE slot_name='drasi_getting_started_slot';
            SELECT row_to_json(m) FROM message AS m ORDER BY messageid;
        """) == retained, "rerunning setup modified retained rows or slot feedback"
        self.sql("getting_started", "ALTER PUBLICATION drasi_getting_started_pub RENAME TO held_publication;")
        partial = self.run(command, env=env, data="", check=False)
        assert partial.returncode != 0, "partial installation must not be silently repaired"
        assert self.sql("getting_started", """
            SELECT confirmed_flush_lsn FROM pg_replication_slots WHERE slot_name='drasi_getting_started_slot';
            SELECT row_to_json(m) FROM message AS m ORDER BY messageid;
        """) == retained
        self.sql("getting_started", "ALTER PUBLICATION held_publication RENAME TO drasi_getting_started_pub;")
        (directory / "setup-wrapper.log").write_text(first.stdout + second.stdout + partial.stderr)
        adapter.unlink()

    def exercise(self, name):
        self.alerts.clear()
        directory = self.evidence / name
        directory.mkdir()
        work = directory / "work"
        work.mkdir()
        config = self.inspect(EXAMPLES / CASES[name])
        recipe_path = EXAMPLES / "postgres" / "recipes" / f"{name}.yaml"
        recipe = self.inspect(recipe_path)
        instance = config["instances"][0]
        self.instance = instance["id"]
        assert instance["sources"] == [] and instance["queries"] == []
        definition = instance["computation"]["definition"]
        sources = [node for node in definition["components"] if node["role"] == "Source"]
        queries = [node["descriptor"]["id"] for node in definition["components"] if node["role"] == "Query"]
        assert len(sources) == 1 and len(queries) >= 1
        assert sources[0]["construction"]["Factory"]["implementation"]["name"] == "drasi.postgres/shared-changes"
        source_id = sources[0]["descriptor"]["id"]
        env = {**os.environ, "POSTGRES_PASSWORD": self.password}
        settings = json.loads(self.run([
            str(self.args.generator.resolve()), "settings", str(recipe_path), source_id,
        ], env=env).stdout)
        database = "getting_started" if name == "getting-started-server" else "native_" + uuid4().hex[:16]
        self.sql("postgres", f"CREATE DATABASE {identifier(database)};")
        if name == "getting-started-server":
            self.standalone_setup(directory)
            slot, publication = "drasi_getting_started_slot", "drasi_getting_started_pub"
        else:
            self.sql(database, self.schema(name))
            slot = "native_" + uuid4().hex[:16]
            publication = "native_" + uuid4().hex[:16]
            tables = settings["connection"]["tables"]
            self.sql(database, " ".join(f"ALTER TABLE {identifier(table)} REPLICA IDENTITY FULL;" for table in tables))
            self.sql(database, f"CREATE PUBLICATION {identifier(publication)} FOR TABLE " + ",".join(map(identifier, tables)) + ";")
            self.sql(database, f"SELECT pg_create_logical_replication_slot('{slot}','pgoutput');")
        settings["connection"].update({
            "host": "127.0.0.1", "port": self.postgres_port, "database": database,
            "user": "postgres", "password": self.password,
            "slot_name": slot, "publication_name": publication, "ssl_mode": "disable",
        })
        key = sources[0]["construction"]["Factory"]["configuration"]["settings"]["Reference"]["key"]
        assert key.startswith("env-json:")
        env[key.removeprefix("env-json:")] = json.dumps(settings)
        api_port, sse_port = port(), port()
        self.api = f"http://127.0.0.1:{api_port}"
        config.update({"host": "127.0.0.1", "port": api_port, "enableUi": False})
        runtime = directory / "server.json"
        runtime.write_text(json.dumps(config))
        env.update({
            "RUST_LOG": "info", "SSE_HOST": "127.0.0.1", "SSE_PORT": str(sse_port),
            "ALERT_WEBHOOK_URL": f"http://127.0.0.1:{self.http.server_port}",
        })
        self.server_log = (directory / "server.log").open("w")
        self.server = subprocess.Popen([
            str(self.args.server.resolve()), "--config", str(runtime),
            "--plugins-dir", str(self.plugin_dir), "--skip-verification",
        ], cwd=work, env=env, stdout=self.server_log, stderr=subprocess.STDOUT)
        try:
            self.until(f"{name}: all native query owners ready", lambda: all(self.rows(query) is not None for query in queries))
            initial = {query: self.rows(query) for query in queries}
            assert any(initial.values()), f"{name}: initial PostgreSQL snapshot is empty"
            if name == "postgres-cdc-complete":
                self.sql(database, "BEGIN; UPDATE users SET email='after@example.invalid'; UPDATE orders SET total=299; UPDATE products SET quantity=3; COMMIT;")
                self.until("three real queries observe one source transaction", lambda:
                           self.rows("user-changes")[0]["Email"] == "after@example.invalid"
                           and self.rows("high-value-orders")[0]["Total"] == 299
                           and self.rows("low-inventory")[0]["Quantity"] == 3)
            elif name == "order-exception-handling":
                self.sql(database, """
                    BEGIN; UPDATE orders SET total_amount=20000 WHERE order_id=1;
                    UPDATE orders SET is_stale=false WHERE order_id=2;
                    INSERT INTO payments VALUES(2,2,50,'failed','NATIVE','live failure');
                    UPDATE order_items SET quantity=4; COMMIT;
                """)
                self.until("order exceptions follow live changes", lambda:
                           self.rows("high-value-orders")[0]["Amount"] == 20000
                           and self.rows("stuck-orders") == []
                           and self.rows("inventory-shortage") == []
                           and len(self.rows("payment-failures")) == 2)
                self.until("native query outlet reaches the existing HTTP reaction", lambda:
                           any("NATIVE" in alert["body"] for alert in self.alerts))
            else:
                lower = name == "getting-started-server"
                table = "message" if lower else '"Message"'
                row_id = 5 if lower else 3
                self.sql(database, f"INSERT INTO {table} VALUES({row_id},'Native Sender','Hello World');")
                query = "hello-world-from" if lower else "all-messages"
                self.until(f"{name}: native CDC reaches the public query API", lambda:
                           any(row.get("MessageId") == row_id for row in self.rows(query)))
                if lower or name.endswith("-6"):
                    inactive = "inactive-people" if lower else "inactive-senders"
                    self.until("scheduled inactivity still uses the real query", lambda:
                               any(row.get("MessageFrom") == "Native Sender" for row in self.rows(inactive)), timeout=35)
            final = {query: self.rows(query) for query in queries}
            status, inventory = self.response(f"/api/v1/instances/{self.instance}/computation")
            assert status == 200, inventory
            (directory / "observations.json").write_text(json.dumps({"initial": initial, "final": final, "inventory": inventory}, indent=2))
            (directory / "alerts.json").write_text(json.dumps(self.alerts, indent=2))
            self.results.append({"case": name, "queries": queries, "slot": slot, "passed": True})
        finally:
            self.stop_server()
        self.until("native PostgreSQL reader stopped", lambda: self.sql(
            database, f"SELECT active::text FROM pg_replication_slots WHERE slot_name='{slot}';") == "false")
        assert self.sql(database, "SELECT count(*) FROM pg_replication_slots WHERE temporary;") == "0"
        shutil.rmtree(work)

    def cleanup(self):
        errors = []
        try:
            self.stop_server()
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            errors.append(str(error))
        if self.http is not None:
            self.http.shutdown()
            self.http.server_close()
            self.http_thread.join(timeout=10)
        if self.container is not None:
            self.run(["podman", "rm", "--force", self.container])
            self.container = None
        (self.evidence / "results.json").write_text(json.dumps(self.results, indent=2) + "\n")
        if errors:
            raise RuntimeError("Cleanup failed: " + "; ".join(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--generator", type=Path, required=True)
    parser.add_argument("--plugins", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--case", choices=list(CASES), action="append")
    proof = Qualification(parser.parse_args())
    try:
        proof.setup()
        for name in proof.args.case or CASES:
            proof.exercise(name)
    except BaseException as primary:
        try:
            proof.cleanup()
        except Exception as cleanup:
            raise BaseExceptionGroup("PostgreSQL qualification and cleanup failed", [primary, cleanup]) from None
        raise
    else:
        proof.cleanup()
    print(f"Native PostgreSQL consumer qualification passed: {proof.evidence}", flush=True)


if __name__ == "__main__":
    main()
