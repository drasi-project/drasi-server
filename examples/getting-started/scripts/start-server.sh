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
# Builds and starts Drasi Server with the getting-started configuration

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$SCRIPT_DIR/.."
SERVER_ROOT="$EXAMPLE_DIR/../.."
CONFIG_FILE="$EXAMPLE_DIR/server-config.yaml"

echo "=== Drasi Server Getting Started ==="
echo

# Check if config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo "Error: Configuration file not found: $CONFIG_FILE"
    exit 1
fi

# Check if PostgreSQL is running
if ! docker ps | grep -q getting-started-postgres; then
    echo "Warning: PostgreSQL container is not running"
    echo "Run ./setup-database.sh first"
    echo
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Build the server
echo "Building Drasi Server (release mode)..."
cd "$SERVER_ROOT"
cargo build --release

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
exec ./target/release/drasi-server --config "$CONFIG_FILE"
