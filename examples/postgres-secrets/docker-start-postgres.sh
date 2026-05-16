#!/usr/bin/env bash
# Start a PostgreSQL 16 container with logical replication enabled.
# The init-db.sql script creates the sample schema, data, and replication slot.

set -euo pipefail

CONTAINER_NAME="drasi-postgres-secrets"
PG_PASSWORD="Drasi@Pass123"
PG_PORT="${PG_PORT:-5432}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Stop any existing container with the same name
if docker ps -aq --filter "name=${CONTAINER_NAME}" | grep -q .; then
  echo "Removing existing container '${CONTAINER_NAME}'..."
  docker rm -f "${CONTAINER_NAME}" >/dev/null
fi

echo "Starting PostgreSQL 16 container '${CONTAINER_NAME}' on port ${PG_PORT}..."

docker run -d \
  --name "${CONTAINER_NAME}" \
  -e POSTGRES_PASSWORD="${PG_PASSWORD}" \
  -p "${PG_PORT}:5432" \
  -v "${SCRIPT_DIR}/init-db.sql:/docker-entrypoint-initdb.d/init-db.sql" \
  postgres:16 \
  -c wal_level=logical \
  -c max_replication_slots=4 \
  -c max_wal_senders=4

echo "Waiting for PostgreSQL to be ready..."
until docker exec "${CONTAINER_NAME}" pg_isready -U postgres >/dev/null 2>&1; do
  sleep 1
done

echo ""
echo "PostgreSQL is ready!"
echo "  Container : ${CONTAINER_NAME}"
echo "  Port      : ${PG_PORT}"
echo "  User      : postgres"
echo "  Password  : ${PG_PASSWORD}"
echo "  Database  : drasi_demo"
echo ""
echo "Connect with: psql -h localhost -p ${PG_PORT} -U postgres -d drasi_demo"
