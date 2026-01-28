# Drasi Server

Drasi Server is a standalone server for the [Drasi](https://drasi.io) data change processing platform. It wraps the [DrasiLib](https://github.com/drasi-project/drasi-core/tree/main/lib) library with enterprise-ready features including a REST API, YAML-based configuration, and production lifecycle management.

## What is Drasi?

Drasi is an open-source Data Change Processing platform that simplifies the creation and operation of change-driven solutions. Rather than functioning as a generic event processor, Drasi specializes in detecting meaningful data modifications through continuous monitoring.

Traditional approaches require manual polling, parsing ambiguous payloads, filtering high-volume event streams, and maintaining external state—introducing brittleness and complexity. Drasi eliminates these overhead costs by letting you declaratively specify what changes matter to your solution through **continuous queries**.

### Core Concepts

- **Sources**: Data ingestion points that connect to your systems and stream changes
- **Continuous Queries**: Cypher or GQL queries that run perpetually, maintaining current results and generating notifications when they change
- **Reactions**: Automated responses triggered when query results change (webhooks, SSE streams, gRPC, logging)
- **Bootstrap Providers**: Pluggable components that deliver initial data to queries before streaming begins

## Getting Started

### Prerequisites

- Rust 1.70 or higher

### Quick Start

#### Using Pre-built Images from GHCR (Fastest)

```bash
# Start the full stack (Drasi Server + PostgreSQL)
docker compose up -d

# Or server only (bring your own database)
docker compose -f docker-compose-server-only.yml up -d

# View logs
docker compose logs -f drasi-server

# Check health
curl http://localhost:8080/health
```

By default, this uses the `ghcr.io/drasi-project/drasi-server:0.1.0` image.

To use a different version:
```bash
# Set image via environment variable
export DRASI_SERVER_IMAGE=ghcr.io/drasi-project/drasi-server:v1.0.0
docker compose up -d

# Or inline
DRASI_SERVER_IMAGE=ghcr.io/drasi-project/drasi-server:latest docker compose up -d
```

#### Building Locally from Source

```bash
# Clone the repository
git clone https://github.com/drasi-project/drasi-server.git
cd drasi-server

# Build the Docker image
make docker-build DOCKER_TAG_VERSION=local

# Update docker-compose to use local image
export DRASI_SERVER_IMAGE=ghcr.io/drasi-project/drasi-server:local
docker compose up -d
```

See [DOCKER.md](DOCKER.md) for detailed Docker deployment instructions.

### Option 3: Manual Setup

```bash
# Ensure Rust is installed (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository
git clone https://github.com/drasi-project/drasi-server.git
cd drasi-server

# Build the server
cargo build --release

# Create a minimal configuration interactively
cargo run -- init --output config/my-config.yaml

# Start the server
cargo run -- --config config/my-config.yaml
```

### Verify Installation

```bash
# Check server health
curl http://localhost:8080/health

# View API documentation
open http://localhost:8080/api/v1/docs/

# List configured queries
curl http://localhost:8080/api/v1/queries
```

### Minimal Configuration Example

```yaml
# config/server.yaml
host: 0.0.0.0
port: 8080
logLevel: info

sources:
  - kind: mock
    id: test-source
    autoStart: true

queries:
  - id: my-query
    query: "MATCH (n:Node) RETURN n"
    sources:
      - sourceId: test-source

reactions:
  - kind: log
    id: log-output
    queries: [my-query]
```

## Command Line Reference

### Synopsis

```
drasi-server [OPTIONS] [COMMAND]
```

### Global Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--config <PATH>` | `-c` | `config/server.yaml` | Path to the configuration file |
| `--port <PORT>` | `-p` | (from config) | Override the server port |
| `--help` | `-h` | | Print help information |
| `--version` | `-V` | | Print version information |

### Commands

#### `run` (default)

Run the server. This is the default command when no subcommand is specified.

```bash
# These are equivalent
drasi-server --config config/server.yaml
drasi-server run --config config/server.yaml
```

**Options:**
- `--config <PATH>`: Path to configuration file (default: `config/server.yaml`)
- `--port <PORT>`: Override the server port from config

#### `init`

Create a new configuration file interactively. Guides you through setting up sources, queries, and reactions.

```bash
drasi-server init --output config/my-config.yaml
drasi-server init --output config/server.yaml --force  # Overwrite existing
```

**Options:**
- `--output <PATH>`, `-o`: Output path for configuration (default: `config/server.yaml`)
- `--force`: Overwrite existing configuration file

#### `validate`

Validate a configuration file without starting the server. Useful for CI/CD pipelines.

```bash
drasi-server validate --config config/server.yaml
drasi-server validate --config config/server.yaml --show-resolved
```

**Options:**
- `--config <PATH>`: Path to configuration file to validate (default: `config/server.yaml`)
- `--show-resolved`: Display configuration with environment variables expanded

#### `doctor`

Check system dependencies and requirements.

```bash
drasi-server doctor
drasi-server doctor --all  # Include optional dependencies
```

**Options:**
- `--all`: Check for optional dependencies (Docker, etc.)

## Configuration Reference

Drasi Server uses YAML configuration files. All configuration values support environment variable interpolation using `${VAR}` or `${VAR:-default}` syntax.

### Server Settings

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | string | (auto-generated UUID) | Unique server identifier |
| `host` | string | `0.0.0.0` | Server bind address |
| `port` | integer | `8080` | Server port |
| `logLevel` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `persistConfig` | boolean | `true` | Enable saving API changes to config file |
| `persistIndex` | boolean | `false` | Use RocksDB for persistent query indexes |
| `stateStore` | object | (none) | State store provider for plugin state persistence |
| `defaultPriorityQueueCapacity` | integer | `10000` | Default capacity for query/reaction event queues |
| `defaultDispatchBufferCapacity` | integer | `1000` | Default buffer capacity for event dispatching |

**Example:**

```yaml
id: my-server
host: 0.0.0.0
port: 8080
logLevel: info
persistConfig: true
persistIndex: false

stateStore:
  kind: redb
  path: ./data/state.redb

sources: []
queries: []
reactions: []
```

### State Store Configuration

State stores allow plugins (Sources, Bootstrap Providers, Reactions) to persist runtime state that survives server restarts. If not configured, an in-memory state store is used (state is lost on restart).

#### REDB State Store

File-based persistent storage using the REDB embedded database.

```yaml
stateStore:
  kind: redb
  path: ./data/state.redb  # Supports ${ENV_VAR:-default}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `kind` | string | Yes | Must be `redb` |
| `path` | string | Yes | Path to the database file |

---

### Sources

Sources connect to data systems and stream changes to queries. Each source type has specific configuration fields.

#### Common Source Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `kind` | string | (required) | Source type: `postgres`, `http`, `grpc`, `mock`, `platform` |
| `id` | string | (required) | Unique source identifier |
| `autoStart` | boolean | `true` | Start source automatically on server startup |
| `bootstrapProvider` | object | (none) | Bootstrap provider configuration |

#### PostgreSQL Source (`postgres`)

Streams changes from PostgreSQL using logical replication (WAL).

```yaml
sources:
  - kind: postgres
    id: my-postgres
    autoStart: true
    host: localhost
    port: 5432
    database: mydb
    user: postgres
    password: ${DB_PASSWORD}
    tables: [orders, customers]
    slotName: drasi_slot
    publicationName: drasi_publication
    sslMode: prefer
    tableKeys:
      - table: orders
        keyColumns: [id]
    bootstrapProvider:
      kind: postgres
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `localhost` | Database host |
| `port` | integer | `5432` | Database port |
| `database` | string | (required) | Database name |
| `user` | string | (required) | Database user |
| `password` | string | `""` | Database password |
| `tables` | array | `[]` | Tables to monitor |
| `slotName` | string | `drasi_slot` | Replication slot name |
| `publicationName` | string | `drasi_publication` | Publication name |
| `sslMode` | string | `prefer` | SSL mode: `disable`, `prefer`, `require` |
| `tableKeys` | array | `[]` | Primary key definitions for tables |

#### HTTP Source (`http`)

Receives events via HTTP endpoints.

```yaml
sources:
  - kind: http
    id: my-http
    autoStart: true
    host: 0.0.0.0
    port: 9000
    timeoutMs: 10000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | (required) | Listen address |
| `port` | integer | (required) | Listen port |
| `endpoint` | string | (auto) | Custom endpoint path |
| `timeoutMs` | integer | `10000` | Request timeout in milliseconds |

#### gRPC Source (`grpc`)

Receives events via gRPC streaming.

```yaml
sources:
  - kind: grpc
    id: my-grpc
    autoStart: true
    host: 0.0.0.0
    port: 50051
    timeoutMs: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `0.0.0.0` | Listen address |
| `port` | integer | `50051` | Listen port |
| `timeoutMs` | integer | `5000` | Connection timeout in milliseconds |

#### Mock Source (`mock`)

Generates test data for development.

```yaml
sources:
  - kind: mock
    id: test-source
    autoStart: true
    dataType: sensor
    intervalMs: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `dataType` | string | `generic` | Type of mock data: `sensor` (SensorReading nodes), `counter` (Counter nodes), `generic` (Generic nodes) |
| `intervalMs` | integer | `5000` | Data generation interval in milliseconds |

#### Platform Source (`platform`)

Consumes events from Redis Streams for Drasi Platform integration.

```yaml
sources:
  - kind: platform
    id: platform-source
    autoStart: true
    redisUrl: redis://localhost:6379
    streamKey: my-stream
    consumerGroup: drasi-core
    batchSize: 100
    blockMs: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `redisUrl` | string | (required) | Redis connection URL |
| `streamKey` | string | (required) | Redis stream key to consume |
| `consumerGroup` | string | `drasi-core` | Consumer group name |
| `consumerName` | string | (auto) | Consumer name within group |
| `batchSize` | integer | `100` | Events to read per batch |
| `blockMs` | integer | `5000` | Block timeout in milliseconds |

---

### Bootstrap Providers

Bootstrap providers deliver initial data to queries before streaming begins. Any source can use any bootstrap provider.

#### PostgreSQL Bootstrap (`postgres`)

Loads initial data from PostgreSQL using the COPY protocol.

```yaml
bootstrapProvider:
  kind: postgres
  # Uses source connection details
```

#### Script File Bootstrap (`scriptfile`)

Loads initial data from JSONL files.

```yaml
bootstrapProvider:
  kind: scriptfile
  filePaths:
    - /data/initial_nodes.jsonl
    - /data/initial_relations.jsonl
```

#### Platform Bootstrap (`platform`)

Loads initial data from a remote Drasi Query API.

```yaml
bootstrapProvider:
  kind: platform
  queryApiUrl: http://remote-drasi:8080
  timeoutSeconds: 300
```

#### No-Op Bootstrap (`noop`)

Returns no initial data.

```yaml
bootstrapProvider:
  kind: noop
```

---

### Queries

Continuous queries process data changes and maintain materialized results.

```yaml
queries:
  - id: active-orders
    query: |
      MATCH (o:Order)
      WHERE o.status = 'active'
      RETURN o.id, o.customer_id, o.total
    queryLanguage: Cypher
    sources:
      - sourceId: orders-db
    autoStart: true
    enableBootstrap: true
    bootstrapBufferSize: 10000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | string | (required) | Unique query identifier |
| `query` | string | (required) | Query string (Cypher or GQL) |
| `queryLanguage` | string | `GQL` | Query language: `Cypher` or `GQL` |
| `sources` | array | (required) | Source subscriptions |
| `autoStart` | boolean | `false` | Start query automatically |
| `enableBootstrap` | boolean | `true` | Process initial data from sources |
| `bootstrapBufferSize` | integer | `10000` | Event buffer size during bootstrap |
| `priorityQueueCapacity` | integer | (global) | Override queue capacity for this query |
| `dispatchBufferCapacity` | integer | (global) | Override buffer capacity for this query |
| `joins` | array | (none) | Synthetic join definitions |

**Important Limitation**: `ORDER BY`, `TOP`, and `LIMIT` clauses are not supported in continuous queries.

#### Source Subscriptions

```yaml
sources:
  - sourceId: orders-db
    nodes: [Order, Customer]      # Optional: filter node labels
    relations: [PLACED_BY]        # Optional: filter relation labels
    pipeline: [decoder, mapper]   # Optional: middleware pipeline
```

#### Synthetic Joins

Create virtual relationships between nodes from different sources:

```yaml
queries:
  - id: order-customer-join
    query: |
      MATCH (o:Order)-[:CUSTOMER]->(c:Customer)
      RETURN o.id, c.name
    sources:
      - sourceId: orders-db
      - sourceId: customers-db
    joins:
      - id: CUSTOMER
        keys:
          - label: Order
            property: customer_id
          - label: Customer
            property: id
```

---

### Reactions

Reactions respond to query result changes.

#### Common Reaction Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `kind` | string | (required) | Reaction type |
| `id` | string | (required) | Unique reaction identifier |
| `queries` | array | (required) | Query IDs to subscribe to |
| `autoStart` | boolean | `true` | Start reaction automatically |

#### Log Reaction (`log`)

Writes query results to console output.

```yaml
reactions:
  - kind: log
    id: log-output
    queries: [my-query]
    autoStart: true
    defaultTemplate:
      added:
        template: "Added: {{json this}}"
      updated:
        template: "Updated: {{json this}}"
      deleted:
        template: "Deleted: {{json this}}"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `routes` | object | `{}` | Query-specific template configurations |
| `defaultTemplate` | object | (none) | Default template for all queries |

#### HTTP Reaction (`http`)

Sends query results to HTTP endpoints.

```yaml
reactions:
  - kind: http
    id: webhook
    queries: [my-query]
    baseUrl: https://api.example.com
    timeoutMs: 5000
    token: ${API_TOKEN}
    routes:
      my-query:
        added:
          url: /events
          method: POST
          headers:
            Content-Type: application/json
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `baseUrl` | string | `http://localhost` | Base URL for requests |
| `timeoutMs` | integer | `5000` | Request timeout in milliseconds |
| `token` | string | (none) | Bearer token for authorization |
| `routes` | object | `{}` | Query-specific endpoint configurations |

#### HTTP Adaptive Reaction (`http-adaptive`)

HTTP reaction with adaptive batching and retry logic.

```yaml
reactions:
  - kind: http-adaptive
    id: adaptive-webhook
    queries: [my-query]
    baseUrl: https://api.example.com
    timeoutMs: 5000
    adaptiveMinBatchSize: 1
    adaptiveMaxBatchSize: 1000
    adaptiveWindowSize: 100
    adaptiveBatchTimeoutMs: 1000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `adaptiveMinBatchSize` | integer | `1` | Minimum batch size |
| `adaptiveMaxBatchSize` | integer | `1000` | Maximum batch size |
| `adaptiveWindowSize` | integer | `100` | Window size for adaptive calculations |
| `adaptiveBatchTimeoutMs` | integer | `1000` | Batch timeout in milliseconds |

#### gRPC Reaction (`grpc`)

Streams query results via gRPC.

```yaml
reactions:
  - kind: grpc
    id: grpc-stream
    queries: [my-query]
    endpoint: grpc://localhost:50052
    timeoutMs: 5000
    batchSize: 100
    maxRetries: 3
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `endpoint` | string | `grpc://localhost:50052` | gRPC endpoint URL |
| `timeoutMs` | integer | `5000` | Connection timeout in milliseconds |
| `batchSize` | integer | `100` | Events per batch |
| `batchFlushTimeoutMs` | integer | `1000` | Batch flush timeout |
| `maxRetries` | integer | `3` | Maximum retry attempts |
| `connectionRetryAttempts` | integer | `5` | Connection retry attempts |
| `initialConnectionTimeoutMs` | integer | `10000` | Initial connection timeout |
| `metadata` | object | `{}` | Custom gRPC metadata key-value pairs |

#### gRPC Adaptive Reaction (`grpc-adaptive`)

gRPC reaction with adaptive batching.

```yaml
reactions:
  - kind: grpc-adaptive
    id: adaptive-grpc
    queries: [my-query]
    endpoint: grpc://localhost:50052
    adaptiveMinBatchSize: 1
    adaptiveMaxBatchSize: 1000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `endpoint` | string | `grpc://localhost:50052` | gRPC endpoint URL |
| `timeoutMs` | integer | `5000` | Connection timeout in milliseconds |
| `maxRetries` | integer | `3` | Maximum retry attempts |
| `connectionRetryAttempts` | integer | `5` | Connection retry attempts |
| `initialConnectionTimeoutMs` | integer | `10000` | Initial connection timeout |
| `metadata` | object | `{}` | Custom gRPC metadata key-value pairs |
| `adaptiveMinBatchSize` | integer | `1` | Minimum batch size |
| `adaptiveMaxBatchSize` | integer | `1000` | Maximum batch size |
| `adaptiveWindowSize` | integer | `100` | Window size for adaptive calculations |
| `adaptiveBatchTimeoutMs` | integer | `1000` | Batch timeout in milliseconds |

#### SSE Reaction (`sse`)

Streams query results via Server-Sent Events.

```yaml
reactions:
  - kind: sse
    id: sse-stream
    queries: [my-query]
    host: 0.0.0.0
    port: 8081
    ssePath: /events
    heartbeatIntervalMs: 30000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `0.0.0.0` | Listen address |
| `port` | integer | `8080` | Listen port |
| `ssePath` | string | `/events` | SSE endpoint path |
| `heartbeatIntervalMs` | integer | `30000` | Heartbeat interval in milliseconds |

#### Platform Reaction (`platform`)

Publishes query results to Redis Streams in CloudEvent format.

```yaml
reactions:
  - kind: platform
    id: redis-publisher
    queries: [my-query]
    redisUrl: redis://localhost:6379
    emitControlEvents: false
    batchEnabled: true
    batchMaxSize: 100
    batchMaxWaitMs: 100
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `redisUrl` | string | (required) | Redis connection URL |
| `pubsubName` | string | (auto) | Pub/sub channel name |
| `sourceName` | string | (auto) | Source identifier in events |
| `maxStreamLength` | integer | (unlimited) | Maximum stream length |
| `emitControlEvents` | boolean | `false` | Emit control events |
| `batchEnabled` | boolean | `false` | Enable batching |
| `batchMaxSize` | integer | `100` | Maximum batch size |
| `batchMaxWaitMs` | integer | `100` | Maximum wait time for batch |

#### Profiler Reaction (`profiler`)

Collects performance metrics for queries.

```yaml
reactions:
  - kind: profiler
    id: query-profiler
    queries: [my-query]
    windowSize: 100
    reportIntervalSecs: 60
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `windowSize` | integer | `100` | Metrics window size |
| `reportIntervalSecs` | integer | `60` | Report interval in seconds |

---

### Multi-Instance Configuration

For advanced use cases requiring isolated processing environments, configure multiple DrasiLib instances:

```yaml
host: 0.0.0.0
port: 8080
logLevel: info

instances:
  - id: analytics
    persistIndex: true
    stateStore:
      kind: redb
      path: ./data/analytics-state.redb
    sources:
      - kind: postgres
        id: analytics-db
        # ... source config
    queries:
      - id: high-value-orders
        query: "MATCH (o:Order) WHERE o.total > 1000 RETURN o"
        sources:
          - sourceId: analytics-db
    reactions:
      - kind: log
        id: analytics-log
        queries: [high-value-orders]

  - id: monitoring
    persistIndex: false
    sources:
      - kind: http
        id: metrics-api
        host: 0.0.0.0
        port: 9001
    queries:
      - id: alert-threshold
        query: "MATCH (m:Metric) WHERE m.value > m.threshold RETURN m"
        sources:
          - sourceId: metrics-api
    reactions:
      - kind: sse
        id: alert-stream
        queries: [alert-threshold]
        port: 8082
```

Each instance has:
- Its own isolated namespace for sources, queries, and reactions
- Optional separate state store and index persistence settings
- API access via `/api/v1/instances/{instanceId}/...`

---

### Environment Variable Interpolation

All configuration values support environment variable substitution:

```yaml
host: ${SERVER_HOST:-0.0.0.0}
port: ${SERVER_PORT:-8080}

sources:
  - kind: postgres
    id: production-db
    host: ${DB_HOST}
    password: ${DB_PASSWORD}  # Required - fails if not set
```

**Syntax:**
- `${VAR}` - Required variable, fails if not set
- `${VAR:-default}` - Optional variable with default value

## REST API

The server exposes a REST API at `http://localhost:8080` (default).

### API Versioning

- `GET /health` - Health check (unversioned)
- `GET /api/versions` - List available API versions
- `GET /api/v1/docs/` - Interactive Swagger UI
- `GET /api/v1/openapi.json` - OpenAPI specification

### Instances API

```bash
GET /api/v1/instances           # List all instances
```

### Sources API

```bash
GET    /api/v1/sources          # List sources (first instance)
POST   /api/v1/sources          # Create source
GET    /api/v1/sources/{id}     # Get source details
DELETE /api/v1/sources/{id}     # Delete source
POST   /api/v1/sources/{id}/start  # Start source
POST   /api/v1/sources/{id}/stop   # Stop source

# Instance-specific routes
GET    /api/v1/instances/{instanceId}/sources
```

### Queries API

```bash
GET    /api/v1/queries          # List queries
POST   /api/v1/queries          # Create query
GET    /api/v1/queries/{id}     # Get query details
DELETE /api/v1/queries/{id}     # Delete query
POST   /api/v1/queries/{id}/start   # Start query
POST   /api/v1/queries/{id}/stop    # Stop query
GET    /api/v1/queries/{id}/results # Get current results

# Instance-specific routes
GET    /api/v1/instances/{instanceId}/queries
```

### Reactions API

```bash
GET    /api/v1/reactions        # List reactions
POST   /api/v1/reactions        # Create reaction
GET    /api/v1/reactions/{id}   # Get reaction details
DELETE /api/v1/reactions/{id}   # Delete reaction
POST   /api/v1/reactions/{id}/start  # Start reaction
POST   /api/v1/reactions/{id}/stop   # Stop reaction

# Instance-specific routes
GET    /api/v1/instances/{instanceId}/reactions
```

### Response Format

```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

## Use Cases

### Real-Time Inventory Alerts

```yaml
queries:
  - id: low-stock-alert
    query: |
      MATCH (p:Product)
      WHERE p.quantity <= p.reorder_point
      RETURN p.sku, p.name, p.quantity, p.reorder_point
    sources:
      - sourceId: inventory-db

reactions:
  - kind: http
    id: reorder-webhook
    queries: [low-stock-alert]
    baseUrl: https://purchasing.example.com
    routes:
      low-stock-alert:
        added:
          url: /reorder
          method: POST
```

### Fraud Detection

```yaml
queries:
  - id: suspicious-transactions
    query: |
      MATCH (t:Transaction)
      WHERE t.amount > 10000
        AND t.country <> t.account_country
      RETURN t.id, t.account_id, t.amount, t.country
    sources:
      - sourceId: transactions-db

reactions:
  - kind: sse
    id: fraud-alerts
    queries: [suspicious-transactions]
    port: 8081
```

## Troubleshooting

### Port Already in Use

```bash
# Use a different port
cargo run -- --port 9090
```

### Query Not Receiving Data

1. Check source status: `GET /api/v1/sources/{id}`
2. Verify query subscription: `GET /api/v1/queries/{id}`
3. Enable debug logging: `RUST_LOG=debug cargo run`

### Debug Logging

```bash
RUST_LOG=debug cargo run
RUST_LOG=drasi_server=trace cargo run
```

## Building from Source

```bash
# Clone the repository
git clone https://github.com/drasi-project/drasi-server.git
cd drasi-server

# Build
cargo build --release

# Run tests
cargo test

# Format and lint
cargo fmt
cargo clippy
```

## License

Apache License 2.0. See [LICENSE](LICENSE) for details.

## Related Projects

- [DrasiLib](https://github.com/drasi-project/drasi-core/tree/main/lib) - Core event processing engine
- [Drasi](https://github.com/drasi-project) - Main Drasi project
- [Drasi Documentation](https://drasi.io/) - Complete documentation

## Support

- **Issues**: [GitHub Issues](https://github.com/drasi-project/drasi-server/issues)
