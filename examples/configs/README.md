# Drasi Server Configuration Examples

This directory contains a progressive learning collection of configuration examples that demonstrate the features and capabilities of Drasi Server.

## Learning Path

The examples are organized into numbered folders representing a progression from basic to advanced concepts:

### 01-fundamentals/

Start here if you're new to Drasi Server.

| File | Description |
|------|-------------|
| `hello-world.yaml` | Absolute minimum configuration - mock source, simple query, log reaction |
| `mock-with-logging.yaml` | Mock source with custom Handlebars log templates |
| `first-continuous-query.yaml` | Understanding query configuration options |

### 02-sources/

Learn about different data source types.

| File | Description |
|------|-------------|
| `http-webhook-receiver.yaml` | HTTP source for receiving webhook events |
| `grpc-streaming-source.yaml` | gRPC source for high-performance streaming |
| `postgres-cdc-complete.yaml` | PostgreSQL CDC with bootstrap and table configuration |

### 03-reactions/

Learn about different reaction types.

| File | Description |
|------|-------------|
| `log-with-templates.yaml` | Log reaction with advanced Handlebars templating |
| `http-webhook-sender.yaml` | HTTP reaction for outbound webhooks |
| `sse-browser-streaming.yaml` | SSE reaction for real-time browser updates |
| `grpc-streaming-reaction.yaml` | gRPC reaction for microservices integration |
| `profiler-performance.yaml` | Profiler reaction for performance analysis |

### 04-query-patterns/

Learn query patterns for continuous queries. These examples use Cypher syntax (explicit `queryLanguage: Cypher`), but the same patterns apply to GQL (the default query language).

| File | Description |
|------|-------------|
| `filter-and-projection.yaml` | WHERE clauses, RETURN projections, comparison operators |
| `aggregation-queries.yaml` | COUNT, SUM, AVG, MIN, MAX, GROUP BY patterns |
| `multi-source-queries.yaml` | Queries spanning multiple data sources (joins) |
| `time-based-triggers.yaml` | Time-based patterns for absence detection |

### 05-advanced-features/

Advanced configuration options for production use.

| File | Description |
|------|-------------|
| `adaptive-batching.yaml` | HTTP and gRPC adaptive batching for high throughput |
| `multi-instance.yaml` | Running multiple DrasiLib instances |
| `persistent-storage.yaml` | RocksDB indexing and REDB state stores |
| `capacity-tuning.yaml` | Queue and buffer capacity optimization |
| `read-only-deployment.yaml` | Immutable infrastructure patterns |

### 06-real-world-scenarios/

Complete real-world use case examples.

| File | Description |
|------|-------------|
| `iot-sensor-alerts.yaml` | IoT monitoring with threshold alerts |
| `order-exception-handling.yaml` | E-commerce order anomaly detection |
| `absence-of-change.yaml` | Detecting when expected changes don't occur |
| `real-time-dashboard.yaml` | Complete dashboard backend with SSE |

## Running Examples

Each example can be run with:

```bash
cargo run -- --config examples/configs/<folder>/<filename>.yaml
```

For example:

```bash
cargo run -- --config examples/configs/01-fundamentals/hello-world.yaml
```

## Configuration Schema Reference

### Server Settings

```yaml
apiVersion: drasi.io/v1
id: "server-id"                    # Unique server ID (auto-generated UUID if not specified)
host: "0.0.0.0"                    # Server bind address
port: 8080                         # Server port
logLevel: "info"                   # Log level: trace, debug, info, warn, error
persistConfig: true                # Save API changes to config file
persistIndex: false                # Use RocksDB for persistent indexes
stateStore:                        # Plugin state persistence
  kind: redb
  path: ./data/state.redb
defaultPriorityQueueCapacity: 10000    # Default queue capacity
defaultDispatchBufferCapacity: 1000    # Default buffer capacity
```

### Source Types

- `mock` - Testing source with configurable data generation
- `http` - HTTP endpoint for receiving events
- `grpc` - gRPC streaming source
- `postgres` - PostgreSQL CDC via logical replication

#### Common Source Settings

```yaml
sources:
  - kind: mock
    id: my-source
    autoStart: true              # Start source automatically (default: true)
    dataType: "sensor"           # Mock data type: sensor, counter, generic
    intervalMs: 1000             # Generation interval in milliseconds

  - kind: http
    id: http-source
    autoStart: true
    host: "0.0.0.0"
    port: 9000
    timeoutMs: 30000             # Request timeout in milliseconds

  - kind: postgres
    id: pg-source
    autoStart: true
    host: "${DB_HOST:-localhost}"
    port: 5432
    database: "mydb"
    user: "${DB_USER}"
    password: "${DB_PASSWORD}"
    sslMode: prefer              # SSL mode: disable, prefer, require
    tables: [users, orders]
    slotName: drasi_slot         # Logical replication slot name
    publicationName: drasi_pub   # Publication name
    tableKeys:                   # Primary key configuration
      - table: users
        keyColumns: [id]
    bootstrapProvider:           # Load existing data on startup
      kind: postgres
```

### Reaction Types

- `log` - Console logging with Handlebars templates
- `http` - HTTP webhooks with per-query routing
- `http-adaptive` - HTTP with intelligent batching
- `grpc` - gRPC streaming
- `grpc-adaptive` - gRPC with intelligent batching
- `sse` - Server-Sent Events for browsers
- `profiler` - Performance analysis

#### Common Reaction Settings

```yaml
reactions:
  - kind: log
    id: my-logger
    queries: [query-1, query-2]
    autoStart: true              # Start reaction automatically (default: true)
    defaultTemplate:             # Default templates for all queries
      added:
        template: "[ADD] {{after}}"
      updated:
        template: "[UPDATE] {{after}}"
      deleted:
        template: "[DELETE] {{before}}"
    routes:                      # Per-query template overrides
      query-1:
        added:
          template: "[SPECIAL] {{after}}"

  - kind: sse
    id: sse-stream
    queries: [query-1]
    autoStart: true
    host: "0.0.0.0"
    port: 8081
    ssePath: "/events"           # SSE endpoint path
    heartbeatIntervalMs: 15000   # Keep-alive interval

  - kind: http
    id: webhook
    queries: [query-1]
    autoStart: true
    baseUrl: "${WEBHOOK_URL}"    # Base URL for all requests
    timeoutMs: 10000             # Request timeout
    routes:
      query-1:
        added:
          url: "/webhook"
          method: "POST"
          body: '{"data": {{after}}}'

  - kind: profiler
    id: perf-profiler
    queries: [query-1]
    autoStart: true
    windowSize: 100              # Observation window size
    reportIntervalSecs: 60       # Seconds between reports
```

### Environment Variables

All configuration values support environment variable interpolation:

```yaml
host: "${SERVER_HOST:-0.0.0.0}"    # With default
password: "${DB_PASSWORD}"          # Required (fails if not set)
```

## API Access

Once running, access the server at:

- `GET /health` - Health check
- `GET /api/v1/openapi.json` - OpenAPI specification
- `GET /api/v1/docs/` - Swagger UI
- `GET /api/v1/sources` - List sources
- `GET /api/v1/queries` - List queries
- `GET /api/v1/reactions` - List reactions

## License

Copyright 2025 The Drasi Authors. Licensed under the Apache License, Version 2.0.
