#!/usr/bin/env python3
"""Qualify the real Trading app's native group against a fresh isolated Server/database."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import threading
import time
from urllib.request import Request, urlopen
from uuid import uuid4

from qualify import EXAMPLES, Qualification, identifier, port


class Trading(Qualification):
    def __init__(self, args):
        super().__init__(args, [
            "drasi_computation_source_postgres", "drasi_computation_bootstrap_postgres",
            "drasi_computation_source_http", "drasi_computation_bootstrap_http", "drasi_reaction_sse",
        ])
        self.api_process = None
        self.api_log = None
        self.stream_thread = None
        self.stream_stop = threading.Event()
        self.stream_ready = threading.Event()
        self.stream_errors = []
        self.events = []

    def request(self, url, body=None, method=None):
        request = Request(url, data=None if body is None else json.dumps(body).encode(),
                          headers={"Content-Type": "application/json"}, method=method)
        with urlopen(request, timeout=10) as response:
            return json.load(response)

    def setup(self):
        super().setup(capture=False)
        self.database = "trading_" + uuid4().hex[:16]
        self.sql("postgres", f"CREATE DATABASE {identifier(self.database)};")
        self.sql(self.database, """
            CREATE TABLE stocks(id SERIAL PRIMARY KEY,symbol TEXT UNIQUE,name TEXT,sector TEXT,industry TEXT);
            CREATE TABLE portfolio(id SERIAL PRIMARY KEY,user_id TEXT,symbol TEXT,quantity INTEGER,purchase_price NUMERIC,purchase_date TIMESTAMPTZ);
            CREATE TABLE watchlist(id SERIAL PRIMARY KEY,user_id TEXT,symbol TEXT,added_at TIMESTAMPTZ DEFAULT now());
            CREATE TABLE limit_orders(id SERIAL PRIMARY KEY,user_id TEXT,symbol TEXT,order_type TEXT,target_price NUMERIC,quantity INTEGER,status TEXT,created_at TIMESTAMPTZ,expires_at TIMESTAMPTZ,stale_duration INTEGER,expire_duration INTEGER);
            INSERT INTO stocks(symbol,name,sector,industry) VALUES('AAPL','Apple','Technology','Devices'),('MSFT','Microsoft','Technology','Software');
            INSERT INTO portfolio(user_id,symbol,quantity,purchase_price,purchase_date) VALUES('demo_user','AAPL',10,100,now());
            INSERT INTO watchlist(user_id,symbol) VALUES('demo_user','AAPL');
            INSERT INTO limit_orders(user_id,symbol,order_type,target_price,quantity,status,created_at,expires_at,stale_duration,expire_duration)
                VALUES('demo_user','AAPL','buy',150,2,'filled',now(),now()+interval '1 minute',30,60);
            ALTER TABLE stocks REPLICA IDENTITY FULL; ALTER TABLE portfolio REPLICA IDENTITY FULL;
            ALTER TABLE watchlist REPLICA IDENTITY FULL; ALTER TABLE limit_orders REPLICA IDENTITY FULL;
        """)
        self.env = {**os.environ, "POSTGRES_HOST": "127.0.0.1", "POSTGRES_PORT": str(self.postgres_port),
                    "POSTGRES_DB": self.database, "POSTGRES_USER": "postgres", "POSTGRES_PASSWORD": self.password}
        self.slots = []
        for kind, tables in [("STOCKS", ["stocks", "portfolio", "watchlist"]), ("BROKER", ["limit_orders"])]:
            slot = "trading_" + uuid4().hex[:16]
            self.slots.append(slot)
            self.sql(self.database, f"CREATE PUBLICATION {identifier(slot)} FOR TABLE {','.join(map(identifier, tables))};")
            self.sql(self.database, f"SELECT pg_create_logical_replication_slot('{slot}','pgoutput');")
            self.env.update({f"TRADING_{kind}_SLOT": slot, f"TRADING_{kind}_PUBLICATION": slot})
        self.api_port, self.source_port, self.sse_port, self.server_port = [port() for _ in range(4)]
        self.env.update({
            "TRADING_API_PORT": str(self.api_port),
            "TRADING_API_HOST": "127.0.0.1",
            "TRADING_SNAPSHOT_URL": f"http://127.0.0.1:{self.api_port}/api/initial-prices",
            "DRASI_HTTP_PRICE_FEED_HOST": "127.0.0.1", "DRASI_HTTP_PRICE_FEED_PORT": str(self.source_port),
            "VITE_DRASI_SERVER_URL": f"http://127.0.0.1:{self.server_port}",
            "VITE_TRADING_SSE_PORT": str(self.sse_port),
            "VITE_TRADING_SSE_HOST": "127.0.0.1",
            "VITE_TRADING_SSE_URL": f"http://127.0.0.1:{self.sse_port}/events",
            "RUST_LOG": "info",
        })
        recipe = EXAMPLES / "trading/server/trading-native-recipe.json"
        for source, key in [
            ("postgres-stocks", "DRASI_PG_POSTGRES_STOCKS_SETTINGS"),
            ("postgres-broker", "DRASI_PG_POSTGRES_BROKER_SETTINGS"),
            ("price-feed", "DRASI_HTTP_PRICE_FEED_SETTINGS"),
        ]:
            self.env[key] = self.run([
                str(self.args.generator.resolve()), "settings", str(recipe), source,
            ], env=self.env).stdout.strip()
        self.api_log = (self.evidence / "trading-api.log").open("w")
        self.api_process = subprocess.Popen([
            self.args.python, str(EXAMPLES / "trading/mock-generator/trading_api.py"),
        ], env=self.env, stdout=self.api_log, stderr=subprocess.STDOUT)
        self.trading_api = f"http://127.0.0.1:{self.api_port}"
        self.until("real Trading API and original price seed", lambda:
                   self.request(self.trading_api + "/health")["status"] == "healthy"
                   and len(self.request(self.trading_api + "/api/initial-prices")["items"]) == 50)

        self.work = self.evidence / "work"
        self.work.mkdir()
        config = self.inspect(EXAMPLES / "trading/server/trading-sources-only.yaml")
        config.update({"host": "127.0.0.1", "port": self.server_port, "enableUi": False})
        config["instances"][0]["persistIndex"] = False
        runtime = self.evidence / "server.json"
        runtime.write_text(json.dumps(config))
        self.instance = "trading-server"
        self.api = f"http://127.0.0.1:{self.server_port}"
        self.server_log = (self.evidence / "server.log").open("w")
        self.server = subprocess.Popen([
            str(self.args.server.resolve()), "--config", str(runtime),
            "--plugins-dir", str(self.plugin_dir), "--skip-verification",
        ], cwd=self.work, env=self.env, stdout=self.server_log, stderr=subprocess.STDOUT)
        self.until("empty Server awaits actual app-owned group", lambda: self.response("/health")[0] == 200)

    def provision(self):
        result = subprocess.run([
            "node", "--import", "tsx", "scripts/qualify-native-setup.ts",
        ], cwd=EXAMPLES / "trading/app", env=self.env, capture_output=True, text=True, timeout=90)
        (self.evidence / "app-setup.log").write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f"Actual Trading app setup failed; see {self.evidence / 'app-setup.log'} and server.log")

    def start_stream(self):
        def read():
            try:
                with urlopen(f"http://127.0.0.1:{self.sse_port}/events", timeout=5) as stream:
                    self.stream_ready.set()
                    while not self.stream_stop.is_set():
                        line = stream.readline().decode().strip()
                        if line.startswith("data:"):
                            if len(self.events) >= 512:
                                raise RuntimeError("SSE evidence bound exceeded")
                            self.events.append(json.loads(line[5:]))
            except (OSError, ValueError, RuntimeError) as error:
                if not self.stream_stop.is_set():
                    self.stream_errors.append(str(error))
        self.stream_thread = threading.Thread(target=read)
        self.stream_thread.start()
        if not self.stream_ready.wait(timeout=10):
            raise RuntimeError(f"Real SSE stream did not connect: {self.stream_errors}")

    def exercise(self):
        self.provision()
        status, initial_inventory = self.response(f"/api/v1/instances/{self.instance}/computation")
        assert status == 200, initial_inventory
        components = initial_inventory["data"]["components"]
        sources = {node["id"]: node["implementation"]["name"] for node in components
                   if node["id"] in ("price-feed", "postgres-stocks", "postgres-broker")}
        assert sources == {"price-feed": "drasi.network/http-source",
                           "postgres-stocks": "drasi.postgres/shared-changes",
                           "postgres-broker": "drasi.postgres/shared-changes"}, sources
        queries = [node["id"] for node in components if node["role"] == "Query"]
        assert len(queries) == 11, queries
        self.until("PG and HTTP bootstrap jointly initialize Trading", lambda:
                   len(self.rows("price-ticker-query")) == 50
                   and self.rows("portfolio-query")[0]["currentPrice"] == 175.5
                   and self.rows("watchlist-query")[0]["name"] == "Apple"
                   and self.rows("active-orders-query")[0]["currentPrice"] == 175.5)
        initial = {query: self.rows(query) for query in queries}
        self.start_stream()
        result = self.request(self.trading_api + "/api/portfolio", {
            "symbol": "MSFT", "quantity": 3, "purchasePrice": 300,
        })
        assert result["success"] is True, result
        result = self.request(self.trading_api + "/api/watchlist", {"symbol": "MSFT"})
        assert result["success"] is True, result
        self.until("real Trading API writes reach shared native PG consumers", lambda:
                   len(self.rows("portfolio-query")) == 2 and len(self.rows("watchlist-query")) == 2)
        self.request(f"http://127.0.0.1:{self.source_port}/sources/price-feed/events", {
            "operation": "update", "timestamp": time.time_ns(),
            "element": {"type": "node", "id": "price_AAPL", "labels": ["stock_prices"],
                        "properties": {"symbol": "AAPL", "price": 210.0, "previous_close": 175.5, "volume": 25000000}},
        })
        self.sql(self.database, """BEGIN; UPDATE portfolio SET quantity=999 WHERE id=1; ROLLBACK;
            BEGIN; UPDATE portfolio SET quantity=11 WHERE id=1;
            UPDATE stocks SET name='Native Apple' WHERE symbol='AAPL'; COMMIT;""")
        self.until("one live price feeds the real PG joins and aggregations", lambda:
                   any(row["symbol"] == "AAPL" and row["quantity"] == 11 and row["currentPrice"] == 210
                       and row["name"] == "Native Apple" for row in self.rows("portfolio-query"))
                   and self.rows("portfolio-summary-query")[0]["totalValue"] == 3525.75
                   and self.rows("active-orders-query")[0]["currentPrice"] == 210
                   and any(row["symbol"] == "AAPL" and row["price"] == 210 for row in self.rows("watchlist-query")))
        self.until("actual native query outlet delivers Trading SSE changes", lambda:
                   any("Native Apple" in json.dumps(event) for event in self.events))
        assert not self.stream_errors, self.stream_errors
        assert not any('"quantity": 999' in json.dumps(event) for event in self.events), "rollback escaped into SSE"
        final = {query: self.rows(query) for query in queries}
        self.provision()
        status, final_inventory = self.response(f"/api/v1/instances/{self.instance}/computation")
        assert status == 200 and final_inventory["data"]["graph"]["revision"] == initial_inventory["data"]["graph"]["revision"]
        (self.evidence / "observations.json").write_text(json.dumps({
            "initial": initial, "final": final, "inventory": final_inventory,
        }, indent=2))
        self.stream_stop.set()
        self.stream_thread.join(timeout=10)
        assert not self.stream_thread.is_alive(), "SSE reader cleanup timed out"
        (self.evidence / "events.json").write_text(json.dumps(self.events, indent=2))
        self.stop_server()
        assert self.sql(self.database, "SELECT count(*) FROM pg_replication_slots WHERE active OR temporary;") == "0"
        assert self.sql(self.database, "SELECT count(*) FROM pg_replication_slots;") == "2"
        shutil.rmtree(self.work)
        self.results.append({"case": "trading", "queries": queries, "sources": sources,
                             "slots": self.slots, "app_setup": True, "sse": True, "passed": True})

    def cleanup(self):
        self.stream_stop.set()
        if self.stream_thread is not None:
            self.stream_thread.join(timeout=10)
        if self.api_process is not None:
            self.api_process.terminate()
            self.api_process.wait(timeout=15)
            self.api_process = None
            self.api_log.close()
        super().cleanup()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["server", "generator", "plugins", "evidence"]:
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--python", default=sys.executable, help="Python with Trading API requirements installed")
    proof = Trading(parser.parse_args())
    try:
        proof.setup()
        proof.exercise()
    except BaseException as primary:
        try:
            proof.cleanup()
        except Exception as cleanup:
            raise BaseExceptionGroup("Trading qualification and cleanup failed", [primary, cleanup]) from None
        raise
    else:
        proof.cleanup()
    print(f"Native Trading qualification passed: {proof.evidence}", flush=True)


if __name__ == "__main__":
    main()
