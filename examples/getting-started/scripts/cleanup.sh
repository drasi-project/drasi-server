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

# Cleanup Script
# Stops Drasi Server and removes the PostgreSQL container

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATABASE_DIR="$SCRIPT_DIR/../database"
REMOVE_VOLUMES="${1:-}"

echo "=== Drasi Server Getting Started - Cleanup ==="
echo

# Stop any running Drasi Server processes
echo "Stopping Drasi Server processes..."
pkill -f "drasi-server.*getting-started" 2>/dev/null || true
pkill -f "drasi-server --config.*getting-started" 2>/dev/null || true

# Check for docker-compose or docker compose
if command -v docker-compose &> /dev/null; then
    COMPOSE_CMD="docker-compose"
elif docker compose version &> /dev/null 2>&1; then
    COMPOSE_CMD="docker compose"
else
    echo "Warning: docker-compose not found, skipping container cleanup"
    exit 0
fi

# Stop and remove PostgreSQL container
echo "Stopping PostgreSQL container..."
cd "$DATABASE_DIR"

if [ "$REMOVE_VOLUMES" = "--volumes" ] || [ "$REMOVE_VOLUMES" = "-v" ]; then
    echo "Removing container and volumes..."
    $COMPOSE_CMD down -v
else
    echo "Removing container (keeping volumes)..."
    $COMPOSE_CMD down
fi

echo
echo "=== Cleanup complete! ==="
echo
echo "Options:"
echo "  $0           # Stop containers, keep data volumes"
echo "  $0 --volumes # Stop containers and remove data volumes"
