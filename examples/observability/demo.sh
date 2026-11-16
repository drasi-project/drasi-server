#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

set -euo pipefail

usage() {
    printf 'Usage: %s [start|validate|init|build-plugins|live [prometheus|otlp]]\n' "$0"
    printf '  start     Run the configuration-only Server in the foreground (default).\n'
    printf '  validate  Validate server-config.yaml without starting a Server.\n'
    printf '  init      Run the wizard, writing wizard-config.yaml without overwriting it.\n'
    printf '  build-plugins  Explicitly build only Server + matched local mock/query/log libraries.\n'
    printf '  live      Run the native pipeline, choosing exactly one metrics backend.\n'
}

if [[ $# -gt 2 ]]; then
    usage >&2
    exit 2
fi

action="${1:-start}"
case "$action" in
    start|validate|init|build-plugins)
        [[ $# -le 1 ]] || { usage >&2; exit 2; }
        ;;
    live)
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
    exec python3 "$SCRIPT_DIR/local_bundle.py" build
fi
if [[ "$action" == live ]]; then
    if [[ "${DRASI_DEMO_TRUST_LOCAL_PLUGINS:-}" != 1 ]]; then
        printf 'live requires DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 to trust the three receipt-checked, unsigned local demo plugins.\n' >&2
        exit 2
    fi
    RUN_DIR="$(python3 "$SCRIPT_DIR/local_bundle.py" prepare)"
    export DRASI_DEMO_INSTANCE="${RUN_DIR##*/}"
    export DRASI_DEMO_PROFILE="${DRASI_DEMO_PROFILE:-persistence}"
    case "$DRASI_DEMO_PROFILE" in
        basic|debug|persistence) ;;
        *) printf 'live requires basic, debug or persistence; use start for Off.\n' >&2; exit 2 ;;
    esac
    printf 'Live %s pipeline; instance=%s\nOwned WAL/state directory (retained): %s\n' \
        "$DRASI_DEMO_METRICS_BACKEND" "$DRASI_DEMO_INSTANCE" "$RUN_DIR"
    printf 'Management: http://127.0.0.1:%s ; Ctrl+C stops the foreground Server.\n' \
        "${DRASI_DEMO_PORT:-18081}"
    cd "$RUN_DIR"
    exec "$SERVER" --config "$SCRIPT_DIR/pipeline-config.yaml" \
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
