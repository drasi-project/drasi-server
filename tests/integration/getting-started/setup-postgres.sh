#!/bin/bash
# Setup PostgreSQL for integration tests
# Can be run in CI or locally

set -e

DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-getting_started}"
DB_USER="${DB_USER:-drasi_user}"
DB_PASSWORD="${DB_PASSWORD:-drasi_password}"

echo "Setting up PostgreSQL for integration tests..."
echo "  Host: $DB_HOST:$DB_PORT"
echo "  Database: $DB_NAME"
echo "  User: $DB_USER"

# Wait for PostgreSQL to be ready
echo "Waiting for PostgreSQL to be ready..."
until PGPASSWORD=$DB_PASSWORD pg_isready -h $DB_HOST -p $DB_PORT -U $DB_USER; do
  echo "  PostgreSQL not ready, retrying..."
  sleep 2
done
echo "PostgreSQL is ready!"

# Configure PostgreSQL for logical replication
echo "Configuring PostgreSQL for logical replication..."
PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -U $DB_USER -d postgres <<EOF
ALTER SYSTEM SET wal_level = logical;
SELECT pg_reload_conf();
EOF

# Always restart to apply wal_level change (it requires restart)
echo "Restarting PostgreSQL to apply wal_level change..."

if [ "$CI" = "true" ] && [ -n "$RESTART_CONTAINER" ]; then
  # In CI with service containers
  docker restart $(docker ps -q --filter ancestor=postgres:15)
else
  # Local development with Docker
  CONTAINER_NAME="${POSTGRES_CONTAINER:-drasi-test-postgres}"
  if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    docker restart "$CONTAINER_NAME"
  else
    echo "Warning: Container '$CONTAINER_NAME' not found"
    echo "Please restart PostgreSQL manually or set POSTGRES_CONTAINER env var"
    echo "Example: docker restart $CONTAINER_NAME"
    exit 1
  fi
fi

sleep 5

# Wait for PostgreSQL to be ready again
echo "Waiting for PostgreSQL to restart..."
until PGPASSWORD=$DB_PASSWORD pg_isready -h $DB_HOST -p $DB_PORT -U $DB_USER; do
  echo "  PostgreSQL not ready, retrying..."
  sleep 2
done
echo "PostgreSQL restarted successfully!"

# Verify wal_level is set correctly
echo "Verifying wal_level setting..."
PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -U $DB_USER -d postgres -c "SHOW wal_level;"

# Create the message table and insert test data
echo "Creating test schema and data..."
PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -U $DB_USER -d $DB_NAME <<EOF
-- Drop table if it exists (for local re-runs)
DROP TABLE IF EXISTS message CASCADE;
DROP PUBLICATION IF EXISTS drasi_getting_started_pub;

-- Create the message table
CREATE TABLE message (
  messageid SERIAL PRIMARY KEY,
  "from" VARCHAR(255) NOT NULL,
  message TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert test data
INSERT INTO message (messageid, "from", message) VALUES
  (1, 'Buzz Lightyear', 'To infinity and beyond!'),
  (2, 'Brian Kernighan', 'Hello World'),
  (3, 'Antoninus', 'I am Spartacus'),
  (4, 'David', 'I am Spartacus');

-- Fix the SERIAL sequence to start after the inserted IDs
SELECT setval('message_messageid_seq', (SELECT MAX(messageid) FROM message));

-- Set up replication publication
CREATE PUBLICATION drasi_getting_started_pub FOR TABLE message;

-- Verify data
SELECT COUNT(*) as row_count FROM message;
SELECT * FROM message ORDER BY messageid;
EOF

echo "PostgreSQL setup complete!"
