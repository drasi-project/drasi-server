#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

"""The same HTTP assertions for release binaries and unmodified runtime images."""

import argparse
import json
import math
from pathlib import Path
import sys
import time
from urllib.error import URLError
from urllib.request import urlopen


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def check_api(get, default_startup=False):
    health = get("/health")
    require(health.get("status") == "ok", f"Unhealthy server: {health}")
    if default_startup:
        expected = {
            "sources": {"vehicle-location-source", "order-status-source"},
            "queries": {"available-drivers-query", "pending-orders-query"},
            "reactions": {"driver-availability-logger", "order-notification-handler"},
        }
        components = {}
        for category, ids in expected.items():
            response = get(f"/api/v1/{category}")
            require(response.get("success") is True, f"Default {category} API failed")
            running = {item["id"] for item in response["data"] if item["status"] == "Running"}
            require(ids <= running, f"Default {category} are not running: {ids - running}")
            components[category] = response["data"]
        return {
            "checks": ["health", "default-components", "plugins"],
            "health": health, "components": components,
            "plugins": get("/api/v1/plugins")["plugins"],
        }
    sources = get("/api/v1/sources")
    require(sources.get("success") is True, f"Source API failed: {sources}")
    require(any(s.get("id") == "test-source" and s.get("status") == "Running"
                for s in sources["data"]), "Mock source is not running")
    queries = get("/api/v1/queries")
    require(queries.get("success") is True, f"Query API failed: {queries}")
    require(any(q.get("id") == "all-sensors" for q in queries["data"]),
            "Smoke query is missing")
    results = get("/api/v1/queries/all-sensors/results")
    require(results.get("success") is True and isinstance(results.get("data"), list),
            f"Results API failed: {results}")
    rows = results["data"]
    require(len(rows) > 0, "Smoke query has not produced any results")
    for row in rows:
        require(isinstance(row.get("SensorId"), str) and bool(row["SensorId"]),
                f"Invalid sensor identifier: {row}")
        for key in ("Temperature", "Humidity"):
            value = row.get(key)
            require(type(value) in (int, float) and math.isfinite(value),
                    f"Missing or invalid numeric {key}: {row}")
    plugins = get("/api/v1/plugins")["plugins"]
    for category, kind in (("Source", "mock"), ("Reaction", "log")):
        require(any(any(k.get("kind") == kind and k.get("category") == category for k in p["kinds"])
                    and p.get("status") in ("Loaded", "Active")
                    and p.get("sdkVersion") and p.get("pluginVersion") for p in plugins),
                f"Missing loaded {category}/{kind} plugin")
    return {
        "checks": ["health", "source", "query", "results", "plugins"],
        "health": health, "sources": sources["data"], "queries": queries["data"],
        "results": rows, "plugins": plugins,
    }


def probe(url, default_startup, timeout):
    def get(path):
        with urlopen(url.rstrip("/") + path, timeout=5) as response:
            return json.load(response)

    deadline = time.monotonic() + timeout
    while True:
        try:
            return check_api(get, default_startup)
        except (URLError, ConnectionError, TimeoutError, AssertionError) as error:
            if time.monotonic() >= deadline:
                raise
            print(f"Waiting for smoke assertions: {error}", file=sys.stderr)
            time.sleep(2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--default-startup", action="store_true")
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    result = probe(args.url, args.default_startup, args.timeout)
    Path(args.output).write_text(json.dumps(result, indent=2) + "\n")
    print("Passed: " + ", ".join(result["checks"]))


if __name__ == "__main__":
    main()
