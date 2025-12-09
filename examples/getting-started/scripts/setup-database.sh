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

# Setup Database Script
# Starts PostgreSQL with WAL replication enabled for CDC

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATABASE_DIR="$SCRIPT_DIR/../database"

echo "=== Drasi Server Getting Started - Database Setup ==="
echo

# Check for Docker
if ! command -v docker &> /dev/null; then
    echo "Error: Docker is not installed or not in PATH"
    echo "Please install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

# Check if Docker is running
if ! docker info &> /dev/null; then
    echo "Error: Docker daemon is not running"
    echo "Please start Docker and try again"
    exit 1
fi

# Check for docker-compose or docker compose
if command -v docker-compose &> /dev/null; then
    COMPOSE_CMD="docker-compose"
elif docker compose version &> /dev/null 2>&1; then
    COMPOSE_CMD="docker compose"
else
    echo "Error: docker-compose is not installed"
    echo "Please install Docker Compose: https://docs.docker.com/compose/install/"
    exit 1
fi

echo "Using: $COMPOSE_CMD"
echo

# Stop any existing container
echo "Stopping any existing PostgreSQL container..."
cd "$DATABASE_DIR"
$COMPOSE_CMD down -v 2>/dev/null || true

# Start PostgreSQL
echo "Starting PostgreSQL with WAL replication..."
$COMPOSE_CMD up -d

# Wait for PostgreSQL to be ready
echo "Waiting for PostgreSQL to be ready..."
MAX_RETRIES=30
RETRY_COUNT=0

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    if docker exec getting-started-postgres pg_isready -U drasi_user -d getting_started &> /dev/null; then
        echo "PostgreSQL is ready!"
        break
    fi
    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "  Waiting... ($RETRY_COUNT/$MAX_RETRIES)"
    sleep 2
done

if [ $RETRY_COUNT -eq $MAX_RETRIES ]; then
    echo "Error: PostgreSQL failed to start within the timeout"
    echo "Check logs with: docker logs getting-started-postgres"
    exit 1
fi

# Verify the setup
echo
echo "Verifying database setup..."

# Check publication exists
PUB_EXISTS=$(docker exec getting-started-postgres psql -U drasi_user -d getting_started -tAc \
    "SELECT 1 FROM pg_publication WHERE pubname = 'drasi_getting_started_pub';" 2>/dev/null || echo "0")

if [ "$PUB_EXISTS" = "1" ]; then
    echo "  Publication: drasi_getting_started_pub [OK]"
else
    echo "  Publication: drasi_getting_started_pub [MISSING]"
fi

# Check replication slot exists
SLOT_EXISTS=$(docker exec getting-started-postgres psql -U drasi_user -d getting_started -tAc \
    "SELECT 1 FROM pg_replication_slots WHERE slot_name = 'drasi_getting_started_slot';" 2>/dev/null || echo "0")

if [ "$SLOT_EXISTS" = "1" ]; then
    echo "  Replication slot: drasi_getting_started_slot [OK]"
else
    echo "  Replication slot: drasi_getting_started_slot [MISSING]"
fi

# Show initial data
echo
echo "Initial messages in database:"
docker exec getting-started-postgres psql -U drasi_user -d getting_started -c \
    "SELECT messageid, \"from\", message FROM message ORDER BY messageid;"

echo
echo "=== Database setup complete! ==="
echo
echo "Connection details:"
echo "  Host: localhost"
echo "  Port: 5432"
echo "  Database: getting_started"
echo "  User: drasi_user"
echo "  Password: drasi_password"
echo
echo "Next step: Run ./start-server.sh to start Drasi Server"
