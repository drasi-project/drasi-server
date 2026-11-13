#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

set -euo pipefail

usage() {
    printf 'Usage: %s [start|validate|init]\n' "$0"
    printf '  start     Run the configuration-only Server in the foreground (default).\n'
    printf '  validate  Validate server-config.yaml without starting a Server.\n'
    printf '  init      Run the wizard, writing wizard-config.yaml without overwriting it.\n'
}

if [[ $# -gt 1 ]]; then
    usage >&2
    exit 2
fi

action="${1:-start}"
case "$action" in
    start|validate|init) ;;
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
cargo build --locked --bin drasi-server --target-dir "$TARGET_DIR"
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
