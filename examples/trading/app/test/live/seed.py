# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

"""Seed only the disposable PostgreSQL instance created by run.ts."""
import json
import os
import sys

import psycopg2

with open(sys.argv[1], encoding="utf-8") as source:
    seed = json.load(source)

connection = psycopg2.connect(
    host="127.0.0.1",
    port=int(os.environ["P1_POSTGRES_PORT"]),
    dbname="trading_demo",
    user="postgres",
    password=os.environ["P1_POSTGRES_PASSWORD"],
)
try:
    with connection, connection.cursor() as cursor:
        cursor.execute("ALTER USER drasi_user WITH PASSWORD %s", (os.environ["P1_POSTGRES_APP_PASSWORD"],))
        cursor.execute("TRUNCATE portfolio, watchlist, limit_orders, stocks RESTART IDENTITY CASCADE")
        cursor.executemany(
            "INSERT INTO stocks (symbol, name, sector, industry) VALUES (%s, %s, %s, %s)",
            [(row["symbol"], row["name"], row["sector"], row["industry"]) for row in seed["stocks"]],
        )
        cursor.executemany(
            "INSERT INTO portfolio (symbol, quantity, purchase_price, purchase_date) VALUES (%s, %s, %s, %s)",
            [(row["symbol"], row["quantity"], row["purchase_price"], row["purchase_date"]) for row in seed["positions"]],
        )
        cursor.executemany("INSERT INTO watchlist (symbol) VALUES (%s)", [(symbol,) for symbol in seed["watchlist"]])
        cursor.executemany(
            "INSERT INTO limit_orders (symbol, order_type, target_price, quantity, status, created_at, expires_at, stale_duration, expire_duration) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)",
            [(row["symbol"], row["order_type"], row["target_price"], row["quantity"], row["status"], row["created_at"], row["expires_at"], row["stale_duration"], row["expire_duration"]) for row in seed["orders"]],
        )
    # Start CDC after seeding; do not replay the production init.sql's old sample
    # inserts/TRUNCATE over the deliberately smaller bootstrap snapshot.
    connection.autocommit = True
    with connection.cursor() as cursor:
        for slot in ("drasi_trading_slot", "drasi_broker_slot"):
            cursor.execute("SELECT pg_drop_replication_slot(%s)", (slot,))
            cursor.execute("SELECT pg_create_logical_replication_slot(%s, %s)", (slot, "pgoutput"))
finally:
    connection.close()
