#!/usr/bin/env bash
# Copyright 2025 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

# This proof owns a fresh Podman container, slots, ports and working directory.
# It never calls the installed demo's setup, reset or cleanup scripts.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$SERVER_ROOT"
TARGET_DIR="${CARGO_TARGET_DIR:-$SERVER_ROOT/target}"
PROFILE="${PROFILE:-debug}"
EVIDENCE_DIR="${EVIDENCE_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/drasi-getting-started.XXXXXX")/proof}"
exec python3 examples/postgres/qualify.py \
    --server "${SERVER_BIN:-$TARGET_DIR/$PROFILE/drasi-server}" \
    --generator "${GENERATOR_BIN:-$TARGET_DIR/$PROFILE/examples/native-postgres}" \
    --plugins "${LOCAL_PLUGINS_DIR:-$TARGET_DIR/$PROFILE}" \
    --evidence "$EVIDENCE_DIR" \
    --case getting-started-server \
    --case getting-started-step-3 \
    --case getting-started-step-4 \
    --case getting-started-step-5 \
    --case getting-started-step-6
