#!/usr/bin/env bash
# Copyright 2025 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

# Provision the standalone tutorial only when its table/publication/slot are
# all absent. Existing complete installations are checked, never reset.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTAINER_CLI="${CONTAINER_CLI:-podman}"
command -v "$CONTAINER_CLI" >/dev/null
cd "$SCRIPT_DIR/../database"
"$CONTAINER_CLI" compose up -d
ready=false
for _ in $(seq 1 30); do
    if "$CONTAINER_CLI" exec getting-started-postgres pg_isready -h 127.0.0.1 -U postgres -d getting_started >/dev/null 2>&1; then
        ready=true
        break
    fi
    sleep 2
done
if [ "$ready" != true ]; then
    echo "PostgreSQL did not become ready. Existing containers and data were left intact." >&2
    exit 1
fi
psql_demo() {
    "$CONTAINER_CLI" exec -i getting-started-postgres psql -X -qAt -v ON_ERROR_STOP=1 \
        -U postgres -d getting_started "$@"
}
state="$(psql_demo -c "SELECT
    (to_regclass('public.message') IS NOT NULL)::int || ',' ||
    (EXISTS(SELECT 1 FROM pg_publication WHERE pubname='drasi_getting_started_pub'))::int || ',' ||
    (EXISTS(SELECT 1 FROM pg_replication_slots WHERE slot_name='drasi_getting_started_slot'))::int;")"
case "$state" in
    0,0,0)
        psql_demo <<'SQL'
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_user WHERE usename='drasi_user') THEN
        CREATE USER drasi_user WITH REPLICATION LOGIN PASSWORD 'drasi_password';
    END IF;
END $$;
GRANT CONNECT ON DATABASE getting_started TO drasi_user;
GRANT USAGE ON SCHEMA public TO drasi_user;
CREATE TABLE message (
    messageid SERIAL PRIMARY KEY,
    "from" VARCHAR(255) NOT NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
ALTER TABLE message REPLICA IDENTITY FULL;
ALTER TABLE message OWNER TO drasi_user;
INSERT INTO message ("from", message) VALUES
    ('Buzz Lightyear','To infinity and beyond!'),
    ('Brian Kernighan','Hello World'),
    ('Antoninus','I am Spartacus'),('David','I am Spartacus');
CREATE PUBLICATION drasi_getting_started_pub FOR TABLE message;
SQL
        # The publication must commit before the configured slot is created.
        psql_demo -c "SELECT pg_create_logical_replication_slot('drasi_getting_started_slot','pgoutput');"
        ;;
    1,1,1)
        echo "Existing tutorial objects found; checking without modifying them."
        ;;
    *)
        echo "Incomplete tutorial installation ($state). Inspect the existing table, publication and slot; this script will not replace or repair retained objects." >&2
        exit 1
        ;;
esac
valid="$(psql_demo -c "SELECT
    EXISTS(SELECT 1 FROM pg_replication_slots WHERE slot_name='drasi_getting_started_slot'
        AND plugin='pgoutput' AND database=current_database() AND restart_lsn IS NOT NULL)
    AND EXISTS(SELECT 1 FROM pg_publication_tables WHERE pubname='drasi_getting_started_pub'
        AND schemaname='public' AND tablename='message')
    AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname='drasi_user' AND rolcanlogin AND rolreplication);")"
if [ "$valid" != t ]; then
    echo "Existing replication configuration is incompatible. No table, publication or slot was dropped." >&2
    exit 1
fi
psql_demo -c 'SELECT messageid,"from",message FROM message ORDER BY messageid;'
echo "Standalone tutorial is ready. Export POSTGRES_PASSWORD for drasi_user before running start-server.sh."
echo "The separate numbered-step configurations use the quoted Message schema; the isolated end-to-end proof covers both."
