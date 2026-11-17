#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

set -euo pipefail

usage() {
    printf 'Usage: %s [start|validate|init|build-plugins [http|postgres]|live [prometheus|otlp]|live-postgres [prometheus|otlp]]\n' "$0"
    printf '  start          Run the configuration-only Server in the foreground (default).\n'
    printf '  validate       Validate server-config.yaml without starting a Server.\n'
    printf '  init           Run the wizard, writing wizard-config.yaml without overwriting it.\n'
    printf '  build-plugins  Explicitly build Server + one matched local scenario bundle (default http).\n'
    printf '  live           Run HTTP input -> native query -> log, choosing one metrics backend.\n'
    printf '  live-postgres  Run an existing PostgreSQL CDC source -> native query -> log.\n'
}

if [[ $# -gt 2 ]]; then
    usage >&2
    exit 2
fi

action="${1:-start}"
scenario=http
case "$action" in
    start|validate|init)
        [[ $# -le 1 ]] || { usage >&2; exit 2; }
        ;;
    build-plugins)
        case "${2:-http}" in
            http|postgres) scenario="${2:-http}" ;;
            *) usage >&2; exit 2 ;;
        esac
        ;;
    live|live-postgres)
        [[ "$action" == live-postgres ]] && scenario=postgres
        case "${2:-prometheus}" in
            prometheus|otlp) export DRASI_DEMO_METRICS_BACKEND="${2:-prometheus}" ;;
            *) usage >&2; exit 2 ;;
        esac
        ;;
    -h|--help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_DIR="$ROOT/target/observability-server"
PLUGINS_DIR="$ROOT/target/observability-example/empty-plugins"
SERVER="$TARGET_DIR/debug/drasi-server"
CONFIG="$SCRIPT_DIR/server-config.yaml"

cd "$ROOT"
if [[ "$action" == build-plugins ]]; then
    exec python3 "$SCRIPT_DIR/local_bundle.py" build "$scenario"
fi
if [[ "$action" == live || "$action" == live-postgres ]]; then
    if [[ "${DRASI_DEMO_TRUST_LOCAL_PLUGINS:-}" != 1 ]]; then
        printf '%s requires DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 to trust the three receipt-checked, unsigned local demo plugins.\n' "$action" >&2
        exit 2
    fi
    if [[ "$scenario" == postgres ]]; then
        # The launcher never starts or chooses a database; supply an isolated one.
        for name in DRASI_DEMO_PG_HOST DRASI_DEMO_PG_DATABASE DRASI_DEMO_PG_USER DRASI_DEMO_PG_PASSWORD; do
            if [[ -z "${!name:-}" ]]; then
                printf 'live-postgres requires %s for an existing, isolated PostgreSQL database.\n' "$name" >&2
                exit 2
            fi
        done
        CONFIG_FILE="$SCRIPT_DIR/postgres-config.yaml"
    else
        CONFIG_FILE="$SCRIPT_DIR/pipeline-config.yaml"
    fi
    RUN_DIR="$(python3 "$SCRIPT_DIR/local_bundle.py" prepare "$scenario")"
    export DRASI_DEMO_INSTANCE="${RUN_DIR##*/}"
    export DRASI_DEMO_PROFILE="${DRASI_DEMO_PROFILE:-persistence}"
    case "$DRASI_DEMO_PROFILE" in
        basic|debug|persistence) ;;
        *) printf 'live requires basic, debug or persistence; use start for Off.\n' >&2; exit 2 ;;
    esac
    # processMetrics requires a non-Off profile and a backend; never override a set value.
    if [[ -z "${DRASI_DEMO_PROCESS_METRICS+set}" ]]; then
        case "$DRASI_DEMO_PROFILE/$DRASI_DEMO_METRICS_BACKEND" in
            off/*|*/none) export DRASI_DEMO_PROCESS_METRICS=false ;;
        esac
    fi
    printf 'Live %s %s pipeline; instance=%s\nOwned WAL/state directory (retained): %s\n' \
        "$scenario" "$DRASI_DEMO_METRICS_BACKEND" "$DRASI_DEMO_INSTANCE" "$RUN_DIR"
    printf 'Management: http://127.0.0.1:%s ; Ctrl+C stops the foreground Server.\n' \
        "${DRASI_DEMO_PORT:-18081}"
    if [[ -n "${DRASI_DEMO_TRACE_ENDPOINT:-}" ]]; then
        printf 'Trace export: HTTP/protobuf to the configured receiver; sample ratio %s.\n' \
            "${DRASI_DEMO_SAMPLE_RATIO:-1.0}"
    else
        printf 'Trace export: disabled (set DRASI_DEMO_TRACE_ENDPOINT to a full /v1/traces URL).\n'
    fi
    cd "$RUN_DIR"
    exec "$SERVER" --config "$CONFIG_FILE" \
        --plugins-dir "$RUN_DIR/plugins" --disable-ui --skip-verification
fi
cargo build --offline --locked --bin drasi-server --target-dir "$TARGET_DIR"
mkdir -p "$PLUGINS_DIR"

case "$action" in
    start)
        printf 'Starting a configuration-only Server; no data pipeline or telemetry export. Ctrl+C stops it.\n'
        exec "$SERVER" --config "$CONFIG" --plugins-dir "$PLUGINS_DIR" --disable-ui
        ;;
    validate)
        exec "$SERVER" validate --config "$CONFIG" --plugins-dir "$PLUGINS_DIR"
        ;;
    init)
        exec "$SERVER" init --output "$SCRIPT_DIR/wizard-config.yaml" --plugins-dir "$PLUGINS_DIR"
        ;;
esac
