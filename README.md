# Drasi Server

A production-ready standalone server for building change-driven solutions with Microsoft Drasi. DrasiServer provides a REST API, configuration management, and lifecycle control around the powerful [DrasiLib](https://github.com/drasi-project/drasi-lib/blob/main/README.md) event processing engine.

## Overview

DrasiServer enables you to build intelligent, event-driven applications that continuously detect and react to critical changes in your data systems - without polling, without delays, and without complexity. It wraps the DrasiLib library with enterprise-ready features including:

- **REST API with OpenAPI/Swagger documentation** for programmatic control
- **YAML-based configuration management** with hot-reload support
- **Production lifecycle management** (graceful shutdown, health checks, monitoring)
- **Builder pattern** for embedding in your applications
- **Read-only mode** support for secure deployments

### What is Change-Driven Architecture?

Traditional applications struggle to identify and respond to meaningful changes across complex, distributed systems. They resort to inefficient polling, complex event processing pipelines, or miss critical state transitions entirely. 

Drasi solves this by providing **continuous queries** that watch for specific patterns of change across multiple data sources, automatically triggering **reactions** when those patterns are detected. This change-driven approach enables:

- **Real-time fraud detection** across payment systems
- **Instant inventory alerts** when stock levels hit thresholds
- **Automated compliance monitoring** for regulatory changes
- **Dynamic resource scaling** based on usage patterns
- **Intelligent alert correlation** across monitoring systems

## Quick Start

Get DrasiServer running in under 5 minutes:

### 1. Install Prerequisites

```bash
# Ensure Rust is installed (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository with all submodules (including nested ones)
git clone --recurse-submodules https://github.com/drasi-project/drasi-server.git
cd drasi-server

# If you already cloned without submodules, initialize them:
git submodule update --init --recursive
```

### 2. Create a Configuration

```yaml
# config/server.yaml
server:
  host: 0.0.0.0
  port: 8080
  log_level: info
  disable_persistence: false

sources:
  - id: inventory-db
    source_type: postgres
    auto_start: true
    # PostgreSQL source configuration (flattened typed fields)
    host: localhost
    port: 5432
    database: inventory
    user: postgres
    password: postgres
    tables: [products]
    slot_name: drasi_inventory_slot
    publication_name: drasi_inventory_pub
    ssl_mode: prefer

queries:
  - id: low-stock-detector
    query: |
      MATCH (p:Product)
      WHERE p.quantity < p.reorder_threshold
      RETURN p.id, p.name, p.quantity, p.reorder_threshold
    queryLanguage: Cypher
    sources: [inventory-db]
    auto_start: true
    enableBootstrap: true
    bootstrapBufferSize: 10000

reactions:
  - id: alert-webhook
    reaction_type: http
    auto_start: true
    queries: [low-stock-detector]
    # HTTP reaction configuration (flattened typed fields)
    base_url: https://alerts.example.com
    timeout_ms: 5000
    routes:
      low-stock-detector:
        path: /webhook
        method: POST
```

### 3. Run the Server

```bash
# Build and run
cargo run

# Or with custom config
cargo run -- --config my-config.yaml --port 9000
```

### 4. Verify It's Working

```bash
# Check health
curl http://localhost:8080/health

# View API documentation
open http://localhost:8080/docs

# List running queries
curl http://localhost:8080/queries
```

## Core Concepts

DrasiServer orchestrates three types of components that work together to create change-driven solutions:

### Sources
Data ingestion points that connect to your systems:
- **PostgreSQL** (`postgres`) - Monitor table changes via WAL replication (formerly `postgres_replication`)
- **HTTP Endpoints** (`http`) - Poll REST APIs for updates
- **gRPC Streams** (`grpc`) - Subscribe to real-time data feeds
- **Platform** (`platform`) - Redis Streams integration for Drasi Platform
- **Mock** (`mock`) - Test data generation
- **Application** (`application`) - Programmatically inject events in embedded usage

**Note:** Source configurations use strongly-typed fields that are flattened at the source level. Each source type has its own specific configuration fields. See the [Configuration](#configuration) section for details.

### Continuous Queries
Cypher-based queries that continuously evaluate incoming changes:
```cypher
// Detect correlated events across systems
MATCH (order:Order)-[:CONTAINS]->(item:Item)
WHERE order.status = 'pending' 
  AND item.inventory_count < item.quantity
RETURN order.id, item.sku, item.quantity - item.inventory_count as shortage
```

### Reactions
Automated responses triggered by query results:
- **HTTP Webhooks** (`http`) - Call external APIs
- **HTTP Adaptive** (`http_adaptive` or `adaptive_http`) - Adaptive HTTP webhooks with retry logic
- **Server-Sent Events** (`sse`) - Stream to browsers
- **gRPC Streams** (`grpc`) - Push to services
- **gRPC Adaptive** (`grpc_adaptive` or `adaptive_grpc`) - Adaptive gRPC streams with retry logic
- **Log** (`log`) - Console logging for debugging
- **Platform** (`platform`) - Redis Streams publishing with CloudEvent format
- **Profiler** (`profiler`) - Performance profiling for queries
- **Application** (`application`) - Custom code handlers for embedded usage

## Building from Source

### Prerequisites
- Rust 1.70 or higher
- Git with submodule support

### Build Steps

#### Important: Submodule Initialization
This project uses nested Git submodules (drasi-lib contains drasi-core as a submodule).
You must initialize all submodules recursively for the build to work.

```bash
# Method 1: Clone with all submodules in one command
git clone --recurse-submodules https://github.com/drasi-project/drasi-server.git
cd drasi-server

# Method 2: If you already cloned without submodules
git clone https://github.com/drasi-project/drasi-server.git
cd drasi-server
git submodule update --init --recursive

# Verify submodules are initialized (should show drasi-lib and drasi-lib/drasi-core)
git submodule status --recursive

# Build debug version
cargo build

# Build optimized release version
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

## Environment Variable Interpolation

### Security Best Practices

**Never hardcode sensitive data** like passwords, API keys, or tokens in configuration files. DrasiServer supports POSIX-style environment variable interpolation to inject secrets at runtime.

### Syntax

```yaml
# Required variable - fails if not set
password: ${DB_PASSWORD}

# Variable with default value
port: ${DB_PORT:-5432}
host: ${DB_HOST:-localhost}
```

### Example Configuration

```yaml
server:
  host: "${SERVER_HOST:-0.0.0.0}"
  port: "${SERVER_PORT:-8080}"
  log_level: "${LOG_LEVEL:-info}"

sources:
  - kind: postgres
    id: production-db
    auto_start: true
    
    # Use environment variables for sensitive data
    host: "${DB_HOST}"
    port: "${DB_PORT:-5432}"
    database: "${DB_NAME}"
    user: "${DB_USER}"
    password: "${DB_PASSWORD}"  # Never hardcode!
    
queries:
  - id: critical-alerts
    query: "MATCH (e:Event) WHERE e.severity = 'critical' RETURN e"
    source_subscriptions:
      - source_id: production-db

reactions:
  - kind: http
    id: webhook-notifier
    queries: [critical-alerts]
    base_url: "${WEBHOOK_URL}"
    # Optional authentication
    headers:
      Authorization: "Bearer ${API_TOKEN}"
```

### Running with Environment Variables

```bash
# Set environment variables
export DB_HOST=db.production.example.com
export DB_PORT=5432
export DB_NAME=production_db
export DB_USER=drasi_user
export DB_PASSWORD=$(vault read -field=password secret/db/drasi)
export WEBHOOK_URL=https://api.example.com/webhooks
export API_TOKEN=$(vault read -field=token secret/api/webhook)

# Run server - variables are automatically interpolated
cargo run -- --config config/server.yaml
```

### Features

- ✅ **Transparent** - Works automatically when loading any config file
- ✅ **Type-safe** - Environment variables work with any config field type (strings, numbers, booleans)
- ✅ **Default values** - Use `${VAR:-default}` syntax for optional configuration
- ✅ **Validation** - Clear error messages if required variables are missing
- ✅ **Backward compatible** - Existing configs without `${...}` work unchanged

### Security Notes

1. **Never commit secrets** to version control
2. **Use secret management** tools (HashiCorp Vault, AWS Secrets Manager, etc.)
3. **Limit permissions** on config files containing `${...}` references
4. **Audit access** to environment variables in production
5. **Rotate secrets** regularly

See `config/server-with-env-vars.yaml` for a comprehensive example.

## Configuration

### Configuration File Structure

DrasiServer uses YAML configuration files with the following structure:

```yaml
# Server settings
server:
  host: 0.0.0.0              # Bind address
  port: 8080                 # API port
  log_level: info            # Log level (trace, debug, info, warn, error)
  disable_persistence: false # Disable automatic config file persistence

# Server core settings (optional)
server_core:
  id: my-server-id                      # Unique server ID (auto-generated if not set)
  priority_queue_capacity: 10000        # Default capacity for query/reaction priority queues
  broadcast_channel_capacity: 1000      # Capacity for broadcast channels (optional)

# Data sources
sources:
  - id: unique-source-id
    source_type: postgres               # Source type (postgres, http, grpc, platform, mock, application)
    auto_start: true                    # Start automatically

    # Bootstrap provider (optional) - Load initial data independently from streaming
    bootstrap_provider:
      type: scriptfile                  # Provider type: postgres, application, scriptfile, platform, noop
      file_paths:                       # For scriptfile provider
        - path/to/data.jsonl

    # Source-specific configuration fields (flattened, not nested under "properties")
    # Each source type has its own typed configuration fields
    # Example for PostgreSQL source:
    host: localhost
    port: 5432
    database: mydb
    user: postgres
    password: secret
    tables: [table1, table2]
    slot_name: drasi_slot
    publication_name: drasi_pub
    ssl_mode: prefer
    dispatch_buffer_capacity: 1000      # Optional: Buffer size for dispatching
    dispatch_mode: channel              # Optional: Dispatch mode (channel, direct)

# Continuous queries
queries:
  - id: unique-query-id
    query: |                            # Cypher or GQL query
      MATCH (n:Node)
      RETURN n
    queryLanguage: Cypher               # Query language (Cypher or GQL, default: Cypher)
    sources: [source-id]                # Source subscriptions
    auto_start: true                    # Start automatically (default: true)
    enableBootstrap: true               # Enable bootstrap data (default: true)
    bootstrapBufferSize: 10000          # Buffer size during bootstrap (default: 10000)
    priority_queue_capacity: 5000       # Override default priority queue capacity (optional)
    joins:                              # Optional synthetic joins
      - id: RELATIONSHIP_TYPE
        keys:
          - label: Node1
            property: join_key
          - label: Node2
            property: join_key

# Reactions
reactions:
  - id: unique-reaction-id
    reaction_type: http                 # Reaction type (http, grpc, sse, log, platform, profiler, etc.)
    queries: [query-id]                 # Query subscriptions
    auto_start: true                    # Start automatically (default: true)
    priority_queue_capacity: 5000       # Override default priority queue capacity (optional)

    # Reaction-specific configuration fields (flattened, not nested under "properties")
    # Each reaction type has its own typed configuration fields
    # Example for HTTP reaction:
    base_url: https://example.com
    timeout_ms: 5000
    token: optional-bearer-token
    routes:
      query-id:
        path: /webhook
        method: POST
```

### Source Configuration Patterns

DrasiServer supports **strongly-typed configuration** where each source type has its own specific configuration fields that are flattened at the source level (not nested under a `properties` key).

**PostgreSQL Source Example:**
```yaml
sources:
  - id: my-postgres
    source_type: postgres
    auto_start: true
    # PostgreSQL-specific typed fields
    host: localhost
    port: 5432
    database: mydb
    user: postgres
    password: secret
    tables: [orders, customers]
    slot_name: drasi_replication_slot
    publication_name: drasi_publication
    ssl_mode: prefer  # Options: disable, prefer, require
```

**HTTP Source Example:**
```yaml
sources:
  - id: my-http-api
    source_type: http
    auto_start: true
    # HTTP-specific typed fields
    host: 0.0.0.0
    port: 9000
    timeout_ms: 10000
```

**Platform Source Example (Redis Streams):**
```yaml
sources:
  - id: redis-stream
    source_type: platform
    auto_start: true
    # Platform-specific typed fields
    redis_url: redis://localhost:6379
    stream_key: my-stream
    consumer_group: my-consumer-group
    batch_size: 10
    block_ms: 1000
```

### Reaction Configuration Patterns

Similar to sources, reactions use strongly-typed configuration fields:

**HTTP Reaction Example:**
```yaml
reactions:
  - id: webhook-reaction
    reaction_type: http
    queries: [my-query]
    auto_start: true
    # HTTP reaction typed fields
    base_url: https://api.example.com
    timeout_ms: 5000
    token: my-bearer-token
    routes:
      my-query:
        path: /events
        method: POST
```

**Adaptive HTTP Reaction Example (with retry logic):**
```yaml
reactions:
  - id: adaptive-webhook
    reaction_type: http_adaptive  # or adaptive_http
    queries: [my-query]
    auto_start: true
    base_url: https://api.example.com
    timeout_ms: 5000
    max_retries: 3
    retry_delay_ms: 1000
```

**Platform Reaction Example (Redis Streams with CloudEvents):**
```yaml
reactions:
  - id: redis-publisher
    reaction_type: platform
    queries: [my-query]
    auto_start: true
    redis_url: redis://localhost:6379
    stream_key: output-stream
```

### Capacity Configuration

DrasiServer supports hierarchical capacity configuration for query and reaction priority queues:

```yaml
server_core:
  priority_queue_capacity: 10000  # Default for all queries and reactions

queries:
  - id: high-volume-query
    priority_queue_capacity: 50000  # Override for this specific query
    query: "MATCH (n) RETURN n"
    sources: [my-source]

reactions:
  - id: high-volume-reaction
    priority_queue_capacity: 50000  # Override for this specific reaction
    reaction_type: http
    queries: [high-volume-query]
```

**Capacity Settings:**
- `server_core.priority_queue_capacity` - Default capacity for all query/reaction priority queues
- `server_core.broadcast_channel_capacity` - Capacity for broadcast channels
- `queries[].priority_queue_capacity` - Override default for a specific query
- `reactions[].priority_queue_capacity` - Override default for a specific reaction
- `sources[].dispatch_buffer_capacity` - Buffer size for source event dispatching

### Configuration Validation

DrasiServer validates all configuration on startup and when creating components via API:

**Server Settings Validation:**
- Port must be non-zero (1-65535)
- Host must be a valid IP address, hostname, "localhost", "0.0.0.0", or "*"
- Hostnames are validated per RFC 1123 standards

**Component Validation:**
- All component IDs must be unique within their type
- Source types must be valid and supported
- Query Cypher syntax is validated
- Reaction types must be valid and supported
- All referenced sources/queries in subscriptions must exist
- Component configuration is delegated to DrasiLib for detailed validation

### Configuration Persistence

DrasiServer supports automatic persistence of runtime configuration changes made through the REST API:

**Persistence is enabled when:**
- A config file is provided on startup (`--config path/to/config.yaml`)
- The config file has write permissions
- `disable_persistence: false` in server settings (default)

**Persistence is disabled when:**
- No config file is provided (server starts with empty configuration)
- Config file is read-only
- `disable_persistence: true` in server settings

**Read-Only Mode:**
- Enabled ONLY when the config file is not writable (file permissions)
- Blocks all API mutations (create/delete operations)
- Different from `disable_persistence: true` which allows mutations but doesn't save them

**Behavior:**
- When persistence enabled: all API mutations are automatically saved to the config file
- Uses atomic writes (temp file + rename) to prevent corruption
- When persistence disabled: changes work but are lost on restart
- When read-only: all create/delete operations via API are rejected with an error

**Example Configuration:**
```yaml
server:
  host: 0.0.0.0
  port: 8080
  log_level: info
  disable_persistence: false  # Enable persistence (default)
sources: []
queries: []
reactions: []
```

### Configuration Migration Guide

If you're upgrading from an older version of DrasiServer, you may need to update your configuration files:

#### Source Type Renames (Breaking Change)

**PostgreSQL sources:**
```yaml
# OLD (no longer supported)
source_type: postgres_replication

# NEW
source_type: postgres
```

#### Flattened Source Configuration (Recommended)

Source configurations now use strongly-typed fields flattened at the source level instead of nested under `properties`:

**OLD pattern (still supported for backward compatibility):**
```yaml
sources:
  - id: my-source
    source_type: postgres
    auto_start: true
    properties:
      host: localhost
      port: 5432
      database: mydb
```

**NEW pattern (recommended - provides better type safety):**
```yaml
sources:
  - id: my-source
    source_type: postgres
    auto_start: true
    # Flattened typed fields
    host: localhost
    port: 5432
    database: mydb
    user: postgres
    password: secret
    tables: [table1]
    slot_name: drasi_slot
    publication_name: drasi_pub
    ssl_mode: prefer
```

**Note:** The flattened pattern is recommended as it provides compile-time type checking and validation. The nested `properties` pattern may be deprecated in future versions.

#### Reaction Configuration

Similar to sources, reactions should use flattened typed fields:

**OLD pattern:**
```yaml
reactions:
  - id: my-reaction
    reaction_type: http
    queries: [my-query]
    properties:
      endpoint: https://example.com
      method: POST
```

**NEW pattern:**
```yaml
reactions:
  - id: my-reaction
    reaction_type: http
    queries: [my-query]
    base_url: https://example.com
    timeout_ms: 5000
    routes:
      my-query:
        path: /webhook
        method: POST
```

### Environment Variables

```bash
# Set log level
RUST_LOG=drasi_server=debug
```

## Library Usage

Embed DrasiServer in your Rust application:

```rust
use drasi_server::{DrasiServerBuilder, DrasiServer};

#[tokio::main]
async fn main() -> Result<()> {
    // Using builder pattern
    let server = DrasiServerBuilder::new()
        .with_port(8080)
        .with_log_level("info")
        .with_source("my-source", source_config)
        .with_query("my-query", "MATCH (n) RETURN n", vec!["my-source"])
        .with_reaction("my-reaction", reaction_config)
        .build()
        .await?;

    server.run().await?;
    
    // Or load from config file
    let server = DrasiServer::new(
        PathBuf::from("config.yaml"),
        8080
    ).await?;
    
    server.run().await
}
```

## REST API

DrasiServer provides a comprehensive REST API for runtime control:

### Health Check

```bash
# Check server health
GET /health
# Returns: {"status": "ok", "timestamp": "2025-01-15T12:00:00Z"}
```

### Sources API

```bash
# List all sources
GET /sources

# Get source details
GET /sources/{id}

# Create a new source
POST /sources
Content-Type: application/json
{
  "id": "new-source",
  "source_type": "postgres",
  "auto_start": true,
  "host": "localhost",
  "port": 5432,
  "database": "mydb",
  "user": "postgres",
  "password": "secret",
  "tables": ["table1"],
  "slot_name": "drasi_slot",
  "publication_name": "drasi_pub",
  "ssl_mode": "prefer"
}

# Delete a source
DELETE /sources/{id}

# Start a source
POST /sources/{id}/start

# Stop a source
POST /sources/{id}/stop
```

### Queries API

```bash
# List all queries
GET /queries

# Get query details
GET /queries/{id}

# Create a new query
POST /queries
Content-Type: application/json
{
  "id": "new-query",
  "query": "MATCH (n:Node) RETURN n",
  "queryLanguage": "Cypher",
  "sources": ["source-id"],
  "auto_start": true,
  "enableBootstrap": true,
  "bootstrapBufferSize": 10000
}

# Delete a query
DELETE /queries/{id}

# Start a query
POST /queries/{id}/start

# Stop a query
POST /queries/{id}/stop

# Get current query results
GET /queries/{id}/results
```

### Reactions API

```bash
# List all reactions
GET /reactions

# Get reaction details
GET /reactions/{id}

# Create a new reaction
POST /reactions
Content-Type: application/json
{
  "id": "new-reaction",
  "reaction_type": "http",
  "queries": ["query-id"],
  "auto_start": true,
  "base_url": "https://api.example.com",
  "timeout_ms": 5000,
  "routes": {
    "query-id": {
      "path": "/webhook",
      "method": "POST"
    }
  }
}

# Delete a reaction
DELETE /reactions/{id}

# Start a reaction
POST /reactions/{id}/start

# Stop a reaction
POST /reactions/{id}/stop
```

### API Documentation

Interactive API documentation is available at:
- Swagger UI: `http://localhost:8080/docs/`
- OpenAPI spec: `http://localhost:8080/api-docs/openapi.json`

### API Response Format

All API responses use a consistent format:

```json
{
  "success": true,
  "data": {...},
  "error": null
}
```

Error responses:

```json
{
  "success": false,
  "data": null,
  "error": "Error message"
}
```

## Use Cases

### Real-Time Inventory Management
```yaml
queries:
  - id: reorder-alert
    query: |
      MATCH (p:Product)
      WHERE p.quantity <= p.reorder_point
        AND p.reorder_status = 'none'
      RETURN p.sku, p.name, p.quantity, p.supplier
```

### Fraud Detection Pipeline
```yaml
queries:
  - id: suspicious-activity
    query: |
      MATCH (t:Transaction)
      WHERE t.amount > 10000
        AND t.country <> t.account.home_country
        AND t.timestamp > timestamp() - 3600
      RETURN t.id, t.account_id, t.amount, t.country
```

### Service Health Monitoring
```yaml
queries:
  - id: service-degradation
    query: |
      MATCH (s:Service)-[:DEPENDS_ON]->(d:Dependency)
      WHERE s.status = 'healthy'
        AND d.status = 'unhealthy'
      RETURN s.name, collect(d.name) as affected_dependencies
```

## Bootstrap Providers

DrasiServer supports pluggable bootstrap providers that supply initial data to queries independently from source streaming. Any source can use any bootstrap provider, enabling powerful patterns like "bootstrap from database, stream changes from HTTP."

### Available Bootstrap Providers

#### PostgreSQL Provider (`postgres`)
Loads initial data from PostgreSQL using snapshot-based bootstrap with LSN coordination:
```yaml
bootstrap_provider:
  type: postgres
  # Uses source properties for connection details
```

#### Script File Provider (`scriptfile`)
Loads initial data from JSONL (JSON Lines) files, useful for testing and development:
```yaml
bootstrap_provider:
  type: scriptfile
  file_paths:
    - /path/to/initial_data.jsonl
    - /path/to/more_data.jsonl  # Multiple files processed in order
```

#### Platform Provider (`platform`)
Fetches initial data from a Query API service in a remote Drasi environment:
```yaml
bootstrap_provider:
  type: platform
  query_api_url: http://remote-drasi:8080  # Query API endpoint
  timeout_seconds: 300                      # Request timeout (default: 300)
```

#### Application Provider (`application`)
Replays stored insert events for application sources:
```yaml
bootstrap_provider:
  type: application
  # Automatically used for application sources
```

#### No-Op Provider (`noop`)
Returns no bootstrap data (useful for streaming-only sources):
```yaml
bootstrap_provider:
  type: noop
```

### Script File Format

Script files use JSONL format with these record types:
- **Header** (required first): Metadata about the script
- **Node**: Graph nodes with labels and properties
- **Relation**: Relationships between nodes
- **Comment**: Filtered out during processing
- **Label**: Checkpoint markers
- **Finish** (optional): Marks end of data

Example script file:
```jsonl
{"type": "Header", "version": "1.0", "description": "Initial product data"}
{"type": "Node", "id": "1", "labels": ["Product"], "properties": {"name": "Widget", "price": 99.99}}
{"type": "Node", "id": "2", "labels": ["Category"], "properties": {"name": "Hardware"}}
{"type": "Relation", "id": "r1", "startId": "1", "endId": "2", "type": "IN_CATEGORY", "properties": {}}
{"type": "Finish"}
```

## Production Deployment

### Health Checks

```bash
# Health check endpoint
GET /health

# Component status checks
GET /sources/{id}
GET /queries/{id}
GET /reactions/{id}
```

### Security Considerations

- Run in **read-only mode** for production deployments
- Use **TLS/HTTPS** for API endpoints
- Implement **authentication** via reverse proxy
- **Validate** all query inputs to prevent injection
- **Limit** resource consumption per query

## Troubleshooting

### Common Issues

**Build fails with "failed to get `drasi-core` as a dependency":**
This error occurs when nested submodules aren't initialized. DrasiServer uses nested submodules:
- `drasi-lib` is a submodule of `drasi-server`
- `drasi-core` is a submodule of `drasi-lib`

Solution:
```bash
# Initialize all submodules recursively
git submodule update --init --recursive

# Verify both submodules are present
ls -la drasi-lib/         # Should exist
ls -la drasi-lib/drasi-core/  # Should also exist

# If issues persist, force update
git submodule update --init --recursive --force
```

**Submodule shows as modified after build:**
This is normal if the submodule was updated. To reset:
```bash
git submodule update --recursive
```

**Port already in use:**
```bash
# Use a different port
cargo run -- --port 9090
```

**Query not receiving data:**
- Verify source is running: `GET /sources/{id}`
- Check source subscription: `GET /queries/{id}`
- Review logs: `RUST_LOG=debug cargo run`

### Debug Logging

Enable detailed logging for troubleshooting:

```bash
# All components
RUST_LOG=debug cargo run

# Specific components
RUST_LOG=drasi_server=debug,drasi_server_core::queries=trace cargo run
```

## Examples

Learn Drasi through practical examples:

### Getting Started Example
**[`examples/getting-started/`](examples/getting-started/)**

A minimal, beginner-friendly example perfect for first-time users:
- Single HTTP source for data ingestion
- Script file bootstrap provider for initial data
- Simple Cypher query filtering products over $50
- Log reaction for viewing results
- Complete with helper scripts and curl examples

**Start here** if you're new to Drasi Server!

### Platform Integration Examples
**[`examples/drasi-platform/`](examples/drasi-platform/)** and **[`examples/drasi-platform-read/`](examples/drasi-platform-read/)**

Demonstrates integration with Drasi Platform via Redis Streams:
- Platform source consuming from Redis Streams
- Platform bootstrap provider for initial data loading
- Platform reaction publishing results to Redis in CloudEvent format
- Consumer group management for horizontal scaling
- Examples show both bootstrap-enabled and read-only modes

### Trading Demo
**[`examples/trading/`](examples/trading/)**

A comprehensive real-world example demonstrating advanced features:
- PostgreSQL replication source with bootstrap
- HTTP sources for live data
- Complex multi-source queries with joins
- Full production-like configuration

## Important Limitations

- **Query Language**: Drasi Core does not support Cypher and GQL queries with `ORDER BY`, `TOP`, and `LIMIT` clauses
- **Nested Submodules**: The project uses nested Git submodules which must be initialized recursively
- **Bootstrap Providers**: Not all sources may work correctly with all bootstrap providers - refer to the examples for tested combinations

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## License

DrasiServer is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.

## Related Projects

- [DrasiLib](https://github.com/drasi-project/drasi-lib) - Core event processing engine
- [Drasi](https://github.com/drasi-project/drasi) - Main Drasi project
- [Drasi Documentation](https://drasi.io/docs) - Complete documentation

## Support

- **Issues**: [GitHub Issues](https://github.com/drasi-project/drasi-server/issues)
- **Discussions**: [GitHub Discussions](https://github.com/drasi-project/drasi/discussions)
- **Blog**: [Drasi Blog](https://techcommunity.microsoft.com/blog/linuxandopensourceblog)

---

*Built with d by the Drasi Project team*