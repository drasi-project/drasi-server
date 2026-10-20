#!/bin/bash
# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Start Server Script
# Starts the matched local native Server/plugin build without resetting data.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$SCRIPT_DIR/.."
SERVER_ROOT="$(cd "$EXAMPLE_DIR/../.." && pwd)"
cd "$SERVER_ROOT"
CONFIG_FILE="$EXAMPLE_DIR/server-config.yaml"
TARGET_DIR="${CARGO_TARGET_DIR:-$SERVER_ROOT/target}"
PROFILE="${PROFILE:-debug}"
SERVER_BIN="${SERVER_BIN:-$TARGET_DIR/$PROFILE/drasi-server}"
GENERATOR_BIN="${GENERATOR_BIN:-$TARGET_DIR/$PROFILE/examples/native-postgres}"
LOCAL_PLUGINS_DIR="${LOCAL_PLUGINS_DIR:-$TARGET_DIR/$PROFILE}"

echo "=== Drasi Server Getting Started ==="
echo

# Check if config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Error: Configuration file not found: $CONFIG_FILE"
    exit 1
fi

for binary in "$SERVER_BIN" "$GENERATOR_BIN"; do
    if [ ! -x "$binary" ]; then
        echo "Build the matching Server, native-postgres example and native libraries first; see examples/postgres/README.md." >&2
        exit 1
    fi
done
: "${POSTGRES_PASSWORD:?Export the configured drasi_user password before starting}"
DRASI_PG_POSTGRES_MESSAGES_SETTINGS="$("$GENERATOR_BIN" settings \
    "$SERVER_ROOT/examples/postgres/recipes/getting-started-server.yaml" postgres-messages)"
export DRASI_PG_POSTGRES_MESSAGES_SETTINGS

echo
echo "Starting Drasi Server..."
echo "  Config: $CONFIG_FILE"
echo "  API: http://localhost:8080"
echo "  SSE: http://localhost:8081/events"
echo "  Swagger UI: http://localhost:8080/swagger-ui/"
echo
echo "Press Ctrl+C to stop the server"
echo "=============================================="
echo

# Run the server
cd "$SERVER_ROOT"
exec "$SERVER_BIN" --config "$CONFIG_FILE" --plugins-dir "$LOCAL_PLUGINS_DIR" --skip-verification
