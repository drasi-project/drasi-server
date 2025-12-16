# Integration Tests

This directory contains integration tests for Drasi Server that can be run both locally and in CI.

## Files

- **`config.yaml`** - Test configuration for Drasi Server
- **`setup-postgres.sh`** - Sets up PostgreSQL database for testing
- **`run-integration-test.sh`** - Runs the integration test suite

## Running Tests Locally

### Prerequisites

1. PostgreSQL running (use Docker or local installation)
2. Drasi Server built in release mode

### Quick Start with Docker

```bash
# Start PostgreSQL with Docker
docker run -d \
  --name drasi-test-postgres \
  -e POSTGRES_DB=getting_started \
  -e POSTGRES_USER=drasi_user \
  -e POSTGRES_PASSWORD=drasi_password \
  -p 5432:5432 \
  postgres:15

# Setup the database
./setup-postgres.sh

# Build Drasi Server (from project root)
cd ../..
cargo build --release

# Run tests
cd tests/integration/getting-started
./run-integration-test.sh
```

### Configuration

All scripts support environment variables for customization:

**Database Configuration:**
- `DB_HOST` - PostgreSQL host (default: `localhost`)
- `DB_PORT` - PostgreSQL port (default: `5432`)
- `DB_NAME` - Database name (default: `getting_started`)
- `DB_USER` - Database user (default: `drasi_user`)
- `DB_PASSWORD` - Database password (default: `drasi_password`)

**Server Configuration:**
- `SERVER_BINARY` - Path to server binary (default: `../../target/release/drasi-server`)
- `CONFIG_FILE` - Path to config file (default: `./config.yaml`)
- `SERVER_PORT` - Server API port (default: `8080`)
- `SERVER_LOG` - Path to server log file (default: `./server.log`)

**Example with custom configuration:**

```bash
DB_HOST=192.168.1.100 \
DB_PORT=5433 \
SERVER_PORT=9090 \
./run-integration-test.sh
```

## Tests

The integration test suite includes:

1. **Health endpoint test** - Verifies server is responding
2. **Sources endpoint test** - Checks PostgreSQL source is registered
3. **Queries endpoint test** - Verifies queries are created
4. **Query status test (filter)** - Verifies filter query is running
5. **Query status test (aggregation)** - Verifies aggregation query is running
6. **Change detection test** - Verifies CDC is working by inserting new data
## CI Usage

The GitHub Actions workflow uses these scripts:

```yaml
- name: Setup PostgreSQL
  run: |
    export RESTART_CONTAINER=true
    ./tests/integration/setup-postgres.sh

- name: Build server
  run: cargo build --release

- name: Run integration tests
  run: ./tests/integration/run-integration-test.sh
```

## Cleanup

To clean up after testing:

```bash
# Stop and remove Docker container
docker stop drasi-test-postgres
docker rm drasi-test-postgres

# Remove server log
rm -f server.log
```

## Troubleshooting

**PostgreSQL connection issues:**
- Verify PostgreSQL is running: `docker ps | grep postgres`
- Check connection: `pg_isready -h localhost -p 5432 -U drasi_user`

**Server won't start:**
- Check server log: `cat server.log`
- Verify binary exists: `ls -la ../../target/release/drasi-server`
- Check port availability: `lsof -i :8080`

**Tests fail:**
- Review server logs in `server.log`
- Check PostgreSQL logs: `docker logs drasi-test-postgres`
- Verify database setup: `PGPASSWORD=drasi_password psql -h localhost -U drasi_user -d getting_started -c '\dt'`
