#!/usr/bin/env python3
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

"""Input for the observability showcase: a steady trickle or a burst.

Changes go through Server's instance-scoped source-data proxy to the HTTP
source named `observations`. A fixed set of reading ids keeps the query result
set bounded: the trickle inserts each id once, then only updates them.
"""

import argparse
import json
import random
import signal
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


SOURCE = "observations"
READINGS = 10
TIMEOUT_SECONDS = 5
START = "DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 bash examples/observability/demo.sh showcase"
# Loopback demo traffic must never be routed through an environment proxy.
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


class DemoUnavailable(Exception):
    """No running demo pipeline accepted the request."""


def port_number(value):
    try:
        port = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be a TCP port from 1 to 65535") from None
    if not 1 <= port <= 65535:
        raise argparse.ArgumentTypeError("must be a TCP port from 1 to 65535")
    return port


def change_count(value):
    try:
        count = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be a whole number from 1 to 1000") from None
    if not 1 <= count <= 1000:
        raise argparse.ArgumentTypeError("must be a whole number from 1 to 1000")
    return count


def api_base(port):
    return f"http://127.0.0.1:{port}/api/v1"


def call(url, body=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(url, data=data, method="GET" if data is None else "POST")
    if data is not None:
        request.add_header("Content-Type", "application/json")
    with OPENER.open(request, timeout=TIMEOUT_SECONDS) as response:
        return json.load(response)


def change(operation, index):
    """One reading change; the value always moves so the query emits a result."""
    return {
        "operation": operation,
        "element": {
            "type": "node",
            "id": f"reading-{index % READINGS + 1}",
            "labels": ["Reading"],
            "properties": {"value": round(random.uniform(15.0, 35.0), 2)},
        },
    }


def trickle_change(sequence):
    """Insert every reading once, then update the existing ids in turn."""
    return change("insert" if sequence < READINGS else "update", sequence)


def discover_instance(port):
    """The single running instance that has this demo's `observations` source."""
    base = api_base(port)
    try:
        listing = call(f"{base}/instances")
        matches = []
        for item in listing.get("data") or []:
            instance = str(item.get("id", ""))
            path = urllib.parse.quote(instance, safe="")
            sources = call(f"{base}/instances/{path}/sources").get("data") or []
            if any(source.get("name") == SOURCE for source in sources):
                matches.append(instance)
    except (OSError, ValueError, AttributeError) as error:
        raise DemoUnavailable(
            f"No running observability demo answered on 127.0.0.1:{port} ({error}).\n"
            f"Start the showcase first: {START}",
        ) from None
    if len(matches) != 1:
        raise DemoUnavailable(
            f"Expected one running instance with source '{SOURCE}' on 127.0.0.1:{port}; "
            f"found {len(matches)}.",
        )
    return matches[0]


def push(port, instance, body):
    path = urllib.parse.quote(instance, safe="")
    url = f"{api_base(port)}/instances/{path}/sources/{SOURCE}/push"
    try:
        reply = call(url, body)
    except urllib.error.HTTPError as error:
        raise DemoUnavailable(f"Server rejected the change with HTTP {error.code}.") from None
    except (OSError, ValueError) as error:
        raise DemoUnavailable(f"Server did not accept the change ({error}).") from None
    if not isinstance(reply, dict) or not reply.get("success"):
        raise DemoUnavailable("Server did not report success for the change.")


def send(port, count):
    instance = discover_instance(port)
    start = random.randrange(READINGS)
    for offset in range(count):
        try:
            push(port, instance, change("update", start + offset))
        except DemoUnavailable as error:
            raise DemoUnavailable(f"Sent {offset} of {count} changes. {error}") from None
    print(f"Sent {count} reading updates to instance {instance} (source {SOURCE}).")


def trickle(port, instance, interval):
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    print(f"Trickle: one change every {interval:g}s to instance {instance}.", flush=True)
    sequence = 0
    while True:
        try:
            push(port, instance, trickle_change(sequence))
            sequence += 1
        except DemoUnavailable as error:
            # Keep going: the demo may still be starting or briefly busy.
            print(f"{time.strftime('%H:%M:%S')} change not accepted: {error}", flush=True)
        time.sleep(interval)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    burst = commands.add_parser("send", help="send a burst of updates to the running demo")
    burst.add_argument("--port", type=port_number, required=True)
    burst.add_argument("--count", type=change_count, default=5)
    steady = commands.add_parser("trickle", help="send one change per interval until stopped")
    steady.add_argument("--port", type=port_number, required=True)
    steady.add_argument("--instance", required=True)
    steady.add_argument("--interval", type=float, default=2.0)
    args = parser.parse_args()
    try:
        if args.command == "send":
            send(args.port, args.count)
        else:
            trickle(args.port, args.instance, args.interval)
    except DemoUnavailable as error:
        print(error, file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
