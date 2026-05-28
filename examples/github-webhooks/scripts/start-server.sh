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
# Builds and starts Drasi Server with the GitHub webhooks configuration

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$SCRIPT_DIR/.."
SERVER_ROOT="$EXAMPLE_DIR/../.."
CONFIG_FILE="$EXAMPLE_DIR/server-config.yaml"

echo "=== Drasi Server GitHub Webhooks Example ==="
echo

# Check if config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Error: Configuration file not found: $CONFIG_FILE"
    exit 1
fi

# Load environment variables
if [ -f "$EXAMPLE_DIR/.env" ]; then
    echo "Loading environment from .env..."
    set -a
    source "$EXAMPLE_DIR/.env"
    set +a
fi

# Build the server
echo "Building Drasi Server (release mode)..."
cd "$SERVER_ROOT"
make build-release

echo
echo "Starting Drasi Server..."
echo "  Config: $CONFIG_FILE"
echo "  API: http://localhost:${SERVER_PORT:-8080}"
echo "  Webhook endpoint: http://localhost:${WEBHOOK_PORT:-9000}/github/events"
echo "  SSE stream: http://localhost:${SSE_PORT:-8081}/events"
echo "  Swagger UI: http://localhost:${SERVER_PORT:-8080}/swagger-ui/"
echo
echo "Press Ctrl+C to stop the server"
echo "=============================================="
echo

# Run the server
exec ./target/release/drasi-server --skip-verification --config "$CONFIG_FILE"
