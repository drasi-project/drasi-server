# Getting Started with Drasi Server

This tutorial helps you create your first change-driven solution using Drasi Server. In approximately 15-20 minutes, you'll have a complete working system with PostgreSQL Change Data Capture (CDC), continuous queries, and real-time streaming reactions.

This tutorial mirrors the [Drasi Platform getting-started tutorial](https://drasi.io/getting-started/) but demonstrates the standalone Drasi Server approach without Kubernetes.

## What You'll Build

A message tracking system that:
- Captures changes from a PostgreSQL database in real-time
- Runs continuous queries that automatically update when data changes
- Streams results to your browser via Server-Sent Events (SSE)
- Logs all changes to the console

## Architecture

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│  PostgreSQL DB  │────▶│    Drasi Server      │────▶│  Browser (SSE)  │
│  (Replication)  │ WAL │                      │ SSE │  viewer/index   │
│                 │     │  Source:             │     │     .html       │
│  Table:         │     │  - postgres-messages │     └─────────────────┘
│  • message      │     │                      │
│                 │     │  Queries:            │     ┌─────────────────┐
└─────────────────┘     │  - hello-world-from  │────▶│  Console (Log)  │
                        │  - message-count     │ Log │  reaction       │
                        │  - inactive-people   │     │                 │
                        │                      │     └─────────────────┘
                        │  Reactions:          │
                        │  - log-all-queries   │     ┌─────────────────┐
                        │  - sse-stream        │────▶│  REST API       │
                        │                      │     │  port 8080      │
                        └──────────────────────┘     └─────────────────┘
```

### Data Flow

1. **PostgreSQL Source** captures changes via logical replication (WAL)
2. **Continuous Queries** filter and aggregate the data
3. **Reactions** deliver results to console and browser in real-time

## Prerequisites

Before you begin, ensure you have:

- **Podman**: Used for isolated PostgreSQL qualification; manual setup also needs
  a Compose provider. `CONTAINER_CLI=docker` selects an existing Docker setup
  for the manual database wrapper instead.
- **Rust/Cargo**: Required to build Drasi Server
  - [Install Rust](https://rustup.rs/)
- **psql** (optional): For manual database queries
  - macOS: `brew install postgresql`
  - Ubuntu: `sudo apt-get install postgresql-client`

Build the matching Server, generator and native libraries using
[`../postgres/README.md`](../postgres/README.md). These configurations use
canonical `computation.definition`, generated from real factory metadata,
because PostgreSQL settings are secret configuration references. The editable
application recipes are in `../postgres/recipes/`; they are not runtime configs.

## Run the entire demo end-to-end (automated)

After building the matching local binaries/libraries, run the isolated proof:

```bash
cd scripts
./run-end-to-end.sh
```

It uses a fresh Podman database, unique configured slots, loopback ports and
new instance state. It exercises both this standalone configuration and numbered
steps 3–6, verifies initial snapshots, inserts a live message, and observes timed
inactivity where configured. All owned processes/database resources are cleaned
up; logs and result evidence remain. It never invokes the installed demo's
setup/reset/cleanup scripts or touches existing slots, volumes or `.env`.

`CARGO_TARGET_DIR` defaults to the Server's `target`, and `PROFILE` defaults to
`debug`. `SERVER_BIN`, `GENERATOR_BIN`, `LOCAL_PLUGINS_DIR` and `EVIDENCE_DIR`
override those artifact locations. The evidence path must be new. There is no
automatic rebuild, registry fallback or destructive `CLEANUP` mode.

## Run the Example Manually

The remainder of this README walks through the same flow manually so
you can learn what each step does.

### Step 1: Start PostgreSQL Database

Open a terminal in this directory and run:

```bash
cd scripts
./setup-database.sh
```

This starts PostgreSQL without removing its volume. When the standalone
`message` table, publication and slot are all absent, it provisions them and
their initial data. An existing complete installation is checked without
modification; a partial or incompatible installation is an explicit error.
It never drops/recreates a configured slot to make startup pass.

The numbered-step configurations use the separate quoted `"Message"` schema,
`drasi_pub` publication and `drasi_slot`. Do not point them at the standalone
lower-case fixture. The isolated proof creates the correct schema for each.

You'll see the initial messages:

| messageid | from            | message                 |
|-----------|-----------------|-------------------------|
| 1         | Buzz Lightyear  | To infinity and beyond! |
| 2         | Brian Kernighan | Hello World             |
| 3         | Antoninus       | I am Spartacus          |
| 4         | David           | I am Spartacus          |

### Step 2: Start Drasi Server

In a **new terminal**, run:

```bash
cd scripts
export POSTGRES_PASSWORD='drasi_password' # Default for a newly provisioned demo only.
./start-server.sh
```

The wrapper uses the already built matching local Server/generator/plugins.
It resolves native PostgreSQL settings into
`DRASI_PG_POSTGRES_MESSAGES_SETTINGS`, starts the generated configuration,
and connects one shared native source to all three real query owners.
`POSTGRES_HOST`, `POSTGRES_PORT`, `POSTGRES_DATABASE` and `POSTGRES_USER`
can override the recipe's defaults; existing credentials are never reset.

You'll see log output showing:
- Source connecting to PostgreSQL
- Queries starting and bootstrapping
- Initial results from the bootstrap phase

### Step 3: View Real-Time Results

**Option A: Browser (Recommended)**

In a **new terminal**, run:

```bash
cd scripts
./open-viewer.sh
```

This starts a local HTTP server and opens the viewer in your browser. The viewer connects to the SSE stream and displays:
- `hello-world-from`: Messages containing "Hello World"
- `message-count`: Message frequency counts
- Change log showing all changes

> **Note**: The viewer must be served via HTTP (not opened as a file) because browsers block cross-origin SSE requests from `file://` URLs.

**Option B: Terminal**

```bash
cd scripts
./stream-events.sh
```

This connects to the SSE endpoint and displays changes in the terminal.

**Option C: API**

```bash
cd scripts
./view-results.sh
```

This queries the REST API for current results.

### Step 4: Make Database Changes

Now let's see the system react to changes! In a **new terminal**:

**Add a "Hello World" message:**
```bash
podman exec getting-started-postgres psql -U drasi_user -d getting_started \
  -c "INSERT INTO message (\"from\",message) VALUES ('Alice','Hello World');"
```

Watch the browser/console - you'll see:
- `hello-world-from` gains a new entry (Alice)
- `message-count` updates the "Hello World" frequency

**Add a different message:**
```bash
podman exec getting-started-postgres psql -U drasi_user -d getting_started \
  -c "INSERT INTO message (\"from\",message) VALUES ('Bob','Goodbye World');"
```

Watch the results:
- `message-count` gains a new "Goodbye World" entry
- `hello-world-from` doesn't change (not "Hello World")

**Wait 20 seconds**, then watch:
- `inactive-people` starts showing users as they become inactive
- This demonstrates time-based continuous queries

**Delete a message:**
```bash
podman exec getting-started-postgres psql -U drasi_user -d getting_started \
  -c "DELETE FROM message WHERE messageid=2;"
```

Watch the results:
- `hello-world-from` loses Brian Kernighan
- `message-count` decreases "Hello World" frequency

### Step 5: Cleanup

Stop the Server with Ctrl+C. Stop the manually started database without removing
its volume when you no longer need it:

```bash
cd database
podman compose stop
```

Do not remove the volume, table, publication or slot as a recovery shortcut.
The automated proof cleans only its own fresh resources.

## Understanding the Components

### PostgreSQL Source

`drasi.postgres/shared-changes` reads the existing configured slot using logical
replication. Its `consumerProgress` resource names all three query progress
owners. Each query's `drasi.postgres/shared-row-bootstrap` provider uses a
temporary exported snapshot and publishes its real boundary only after complete
initialization. Feedback respects every required consumer; there is no aggregate
checkpoint or separate source per query.

Edit `../postgres/recipes/getting-started-server.yaml` and rerun
`examples/native-postgres.rs` to regenerate `server-config.yaml`. The generator
copies actual factory schemas and ports and preserves secret references; do not
hand-edit the expanded metadata.

Key features:
- **WAL Replication**: Zero-polling change detection
- **Bootstrap**: Loads existing data on startup
- **Tables**: Specifies which tables to monitor

### Continuous Queries

Queries run continuously and automatically update when source data changes.

#### Query 1: hello-world-from

Filters messages containing "Hello World":

```cypher
MATCH (m:message {message: 'Hello World'})
RETURN m.messageid AS MessageId, m.from AS MessageFrom
```

#### Query 2: message-count

Aggregates messages by content:

```cypher
MATCH (m:message)
RETURN m.message AS Message, count(m) AS Frequency
```

#### Query 3: inactive-people

Finds users inactive for 20+ seconds using time functions:

```cypher
MATCH (m:message)
WITH m.from AS MessageFrom, max(drasi.changeDateTime(m)) AS LastMessageTimestamp
WHERE LastMessageTimestamp <= datetime.realtime() - duration({ seconds: 20 })
  OR drasi.trueLater(
       LastMessageTimestamp <= datetime.realtime() - duration({ seconds: 20 }),
       LastMessageTimestamp + duration({ seconds: 20 })
     )
RETURN MessageFrom, LastMessageTimestamp
```

Key Cypher functions:
- `drasi.changeDateTime(m)`: Returns when the element was last changed
- `drasi.trueLater(condition, time)`: Triggers future evaluation

### Reactions

#### Log Reaction

Prints query results to the console with templates:

```yaml
reactions:
  - kind: log
    id: log-all-queries
    queries:
      - hello-world-from
      - message-count
      - inactive-people
    defaultTemplate:
      added:
        template: "[{{query_name}}] + {{after}}"
      updated:
        template: "[{{query_name}}] ~ {{before}} -> {{after}}"
      deleted:
        template: "[{{query_name}}] - {{before}}"
```

#### SSE Reaction

Streams results to browser via Server-Sent Events:

```yaml
  - kind: sse
    id: sse-stream
    queries:
      - hello-world-from
      - message-count
      - inactive-people
    host: 0.0.0.0
    port: 8081
    ssePath: /events
```

## API Reference

The server exposes a REST API on port 8080:

### Health & Documentation

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check |
| `GET /api/versions` | List available API versions |
| `GET /api/v1/docs/` | Interactive API documentation (Swagger UI) |
| `GET /api/v1/openapi.json` | OpenAPI specification |

### Sources

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/instances/getting-started-server/sources` | List all sources |
| `GET /api/v1/instances/getting-started-server/sources/{id}` | Get source status |
| `POST /api/v1/instances/getting-started-server/sources/{id}/start` | Start a source |
| `POST /api/v1/instances/getting-started-server/sources/{id}/stop` | Stop a source |

### Queries

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/instances/getting-started-server/queries` | List all queries |
| `GET /api/v1/instances/getting-started-server/queries/{id}` | Get query status |
| `GET /api/v1/instances/getting-started-server/queries/{id}/results` | Get current query results |
| `POST /api/v1/instances/getting-started-server/queries/{id}/start` | Start a query |
| `POST /api/v1/instances/getting-started-server/queries/{id}/stop` | Stop a query |

### Reactions

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/instances/getting-started-server/reactions` | List all reactions |
| `GET /api/v1/instances/getting-started-server/reactions/{id}` | Get reaction status |
| `POST /api/v1/instances/getting-started-server/reactions/{id}/start` | Start a reaction |
| `POST /api/v1/instances/getting-started-server/reactions/{id}/stop` | Stop a reaction |

See `requests.http` for examples using VSCode REST Client.

## Troubleshooting

### PostgreSQL Connection Failed

**Error**: `Connection refused` or `Cannot connect to PostgreSQL`

**Solution**:
1. Verify PostgreSQL is running: `podman ps --filter name=getting-started-postgres`
2. If not running, start it: `./scripts/setup-database.sh`
3. Check database logs: `podman logs getting-started-postgres`

### Replication Slot Issues

**Error**: `replication slot already exists` or replication errors

**Solution**:
Inspect the configured database/slot/publication binding, retained WAL and other
active readers. A missing or incompatible slot is not automatically recreated.
Stop the Server before an explicit operator-controlled recovery; never delete
retained data or reset feedback to suppress the error. Use the isolated proof
to distinguish an installation problem from the native implementation.

### SSE Connection Failed

**Error**: Browser shows "Disconnected" or events not appearing

**Solution**:
1. Verify Drasi Server is running
2. Check SSE reaction is started:
   ```bash
   curl http://localhost:8080/api/v1/instances/getting-started-server/reactions/sse-stream
   ```
3. Check browser console for CORS errors
4. Try the terminal stream: `./scripts/stream-events.sh`

### No Query Results

**Problem**: Queries show no data after startup

**Solution**:
1. Check if source is connected:
   ```bash
   curl http://localhost:8080/api/v1/instances/getting-started-server/sources/postgres-messages
   ```
2. Verify bootstrap completed in server logs
3. Check query status:
   ```bash
   curl http://localhost:8080/api/v1/instances/getting-started-server/queries/hello-world-from
   ```

### Port Already in Use

**Error**: `Address already in use (port 8080)` or `(port 8081)`

**Solution**:
1. Find and stop the conflicting process:
   ```bash
   lsof -i :8080
   kill <PID>
   ```
2. Or modify the ports in `server-config.yaml`

## Differences from Drasi Platform

| Aspect | Drasi Platform | Drasi Server |
|--------|----------------|--------------|
| Deployment | Kubernetes | Standalone binary |
| Configuration | YAML CRDs + drasi CLI | Single YAML file |
| Query Languages | Cypher + GQL | Cypher only |
| Debug Reaction | Built-in Web UI | SSE + custom viewer |
| Scaling | Kubernetes-native | Single instance |

## What's Next?

After completing this tutorial, explore:

1. **Advanced Queries**: Modify queries to:
   - Join multiple tables
   - Use complex WHERE clauses
   - Add more aggregations

2. **Different Reactions**: Add reactions like:
   - `http` for webhooks to external services
   - `grpc` for gRPC streaming

3. **Multiple Sources**: Add sources like:
   - `http` for REST API polling
   - `grpc` for gRPC streaming
   - `platform` for Redis Streams integration

4. **Other Examples**:
   - [Trading Demo](../trading/) - Multi-source with React UI
   - [Drasi Platform Integration](../drasi-platform/) - Redis Streams

## Files in This Example

```
examples/getting-started/
├── README.md                          # This file
├── server-config.yaml                 # Complete server configuration
├── requests.http                      # VSCode REST Client examples
├── database/
│   ├── docker-compose.yml            # PostgreSQL with WAL replication
│   └── init.sql                      # Schema and initial data
├── scripts/
│   ├── setup-database.sh             # Start PostgreSQL container
│   ├── start-server.sh               # Build and start Drasi Server
│   ├── open-viewer.sh                # Serve viewer via HTTP and open browser
│   ├── add-message.sh                # Insert new message
│   ├── delete-message.sh             # Delete a message
│   ├── view-results.sh               # Query API for results
│   ├── stream-events.sh              # Connect to SSE stream
│   └── cleanup.sh                    # Stop server and containers
└── viewer/
    └── index.html                    # Browser-based SSE viewer
```

## Learn More

- [Drasi Documentation](https://drasi.io/)
- [Drasi Platform Getting Started](https://drasi.io/getting-started/)
- [Cypher Query Language](https://neo4j.com/docs/cypher-manual/current/)
- [PostgreSQL Logical Replication](https://www.postgresql.org/docs/current/logical-replication.html)
- [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events)
