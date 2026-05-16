# PostgreSQL with Secret Store Example

This example demonstrates how to use the **file-based secret store plugin** to
keep passwords out of your Drasi Server configuration file. Instead of
hardcoding `password: "Drasi@Pass123"`, the source config uses a `Secret`
envelope:

```yaml
password:
  kind: Secret
  name: DB_PASSWORD
```

At runtime, Drasi resolves `DB_PASSWORD` by looking it up in the configured
secret store — in this case, a flat JSON file (`secrets.json`).

## Prerequisites

- Docker (for the PostgreSQL container)
- A built Drasi Server binary (`cargo build`)
- The **file secret store plugin** (`libdrasi_secret_store_file.so`) in the
  server's `plugins/` directory. Build it from drasi-core:

  ```bash
  cd ../drasi-core
  make build-local-plugins
  ```

## Quick Start

### 1. Start PostgreSQL

```bash
./examples/postgres-secrets/docker-start-postgres.sh
```

This starts a PostgreSQL 16 container with:
- Logical replication enabled
- A `drasi_demo` database with a `sensors` table and sample data
- A replication slot (`drasi_slot`) and publication (`drasi_pub`)

### 2. Run Drasi Server

```bash
cargo run -- --skip-verification --config examples/postgres-secrets/server-config.yaml
```

The `--skip-verification` flag is needed because locally-built plugins are not
signed.

### 3. Observe

The server will:
1. Load the file secret store plugin and create a provider from `secrets.json`
2. Create the PostgreSQL replication source, resolving `DB_PASSWORD` from the
   secret store
3. Start the `high-temp` continuous query
4. The `log-temps` reaction will print change events for sensors with
   `temperature > 75` to stdout

### 4. Test live changes

In another terminal, connect to PostgreSQL and update a sensor:

```bash
docker exec -it drasi-postgres-secrets psql -U postgres -d drasi_demo

UPDATE sensors SET temperature = 90.0 WHERE name = 'sensor-1';
```

You should see the change event appear in the server's stdout via the log
reaction.

## Files

| File | Purpose |
| --- | --- |
| `docker-start-postgres.sh` | Starts the PostgreSQL Docker container |
| `init-db.sql` | Creates the database, tables, replication slot, and seed data |
| `secrets.json` | Flat JSON file containing the `DB_PASSWORD` secret |
| `server-config.yaml` | Drasi Server config using the secret store |

## How It Works

```
server-config.yaml          secrets.json
┌──────────────────┐       ┌──────────────────┐
│ secretStore:     │       │ {                │
│   kind: file     │──────▶│   "DB_PASSWORD": │
│   path: ...      │       │   "Drasi@Pass123"│
│                  │       │ }                │
│ sources:         │       └──────────────────┘
│   password:      │
│     kind: Secret │──── resolved at runtime ──▶ "Drasi@Pass123"
│     name:        │
│       DB_PASSWORD│
└──────────────────┘
```

## Cleanup

```bash
docker rm -f drasi-postgres-secrets
```
