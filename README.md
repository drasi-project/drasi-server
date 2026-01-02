# Drasi Server

A standalone server for building change-driven solutions with Drasi. DrasiServer provides a REST API, configuration management, and lifecycle control around the powerful [DrasiLib](https://github.com/drasi-project/drasi-lib/blob/main/README.md) event processing engine.

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

Get DrasiServer running in under 5 minutes. Choose the option that works best for you:

### Option 1: Native with Make (Recommended)

```bash
# Clone the repository with submodules
git clone --recurse-submodules https://github.com/drasi-project/drasi-server.git
cd drasi-server

# One-command setup (checks dependencies, builds, creates config)
make setup

# Start the server
make run
```

### Option 2: Docker

```bash
# Clone the repository with submodules
git clone --recurse-submodules https://github.com/drasi-project/drasi-server.git
cd drasi-server

# Copy environment template
cp .env.example .env

# Start the full stack (Drasi Server + PostgreSQL)
docker compose up -d

# View logs
docker compose logs -f drasi-server
```

See [DOCKER.md](DOCKER.md) for detailed Docker deployment instructions.

### Option 3: Manual Setup

```bash
# Ensure Rust is installed (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository with all submodules (including nested ones)
git clone --recurse-submodules https://github.com/drasi-project/drasi-server.git
cd drasi-server

# If you already cloned without submodules, initialize them:
git submodule update --init --recursive

# Build and run
cargo run
```

### Verify It's Working

```bash
# Check health
curl http://localhost:8080/health

# List available API versions
curl http://localhost:8080/api/versions

# View API documentation
open http://localhost:8080/api/v1/docs/

# List configured instances
curl http://localhost:8080/api/v1/instances

# List running queries (default instance)
curl http://localhost:8080/api/v1/queries

# List queries for a specific instance
curl http://localhost:8080/api/v1/instances/{instanceId}/queries
```

### CLI Commands

After building, you can run CLI commands using `cargo run --` or the binary directly:

```bash
# Using cargo run (recommended during development)
cargo run -- --version
cargo run -- doctor --all
cargo run -- validate --config config/server.yaml
cargo run -- init --output config/my-config.yaml

# Or use the binary directly
./target/debug/drasi-server --version
./target/debug/drasi-server doctor --all

# Or install globally (then use 'drasi-server' directly)
cargo install --path .
drasi-server --version
```

See the [Interactive Configuration (init command)](#interactive-configuration-init-command) section for details on the `init` command.

### Example Configuration

```yaml
# config/server.yaml
id: inventory-server
host: 0.0.0.0
port: 8080
log_level: info
disable_persistence: false
persist_index: false  # Use RocksDB for persistent indexing (default: false)

sources:
  - id: inventory-db
    source_type: postgres
    auto_start: true
    host: "${DB_HOST:-localhost}"
    port: "${DB_PORT:-5432}"
    database: "${DB_NAME:-inventory}"
    user: "${DB_USER:-postgres}"
    password: "${DB_PASSWORD}"
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
    base_url: https://alerts.example.com
    timeout_ms: 5000
    routes:
      low-stock-detector:
        path: /webhook
        method: POST
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

## Interactive Configuration (init command)

The `drasi-server init` command provides an interactive wizard for creating configuration files. Instead of manually writing YAML, you can answer a series of questions to build a custom configuration tailored to your needs.

### Usage

```bash
# Create a new configuration file interactively
drasi-server init

# Specify output path
drasi-server init --output config/my-config.yaml

# Overwrite existing file
drasi-server init --output config/server.yaml --force
```

### Command Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--output` | `-o` | `config/server.yaml` | Output path for the configuration file |
| `--force` | | `false` | Overwrite existing configuration file |

### Interactive Flow

The init wizard guides you through four main steps:

#### Step 1: Server Settings

Configure basic server parameters:

```
Server Settings
---------------
? Server host: [0.0.0.0]
? Server port: [8080]
? Log level: [info/debug/warn/error/trace]
? Enable persistent indexing (RocksDB)? [y/N]
```

#### Step 2: Data Sources

Select one or more data sources for your configuration:

```
Data Sources
------------
? Select sources (space to select, enter to confirm):
  ▸ [x] PostgreSQL - CDC from PostgreSQL database
    [ ] HTTP - Receive events via HTTP endpoint
    [ ] gRPC - Stream events via gRPC
    [x] Mock - Generate test data (for development)
    [ ] Platform - Redis Streams integration
```

For each selected source, you'll be prompted for source-specific configuration:

**PostgreSQL Source:**
```
Configuring PostgreSQL Source
------------------------------
? Source ID: [postgres-source]
? Database host: [localhost]
? Database port: [5432]
? Database name: [postgres]
? Database user: [postgres]
? Database password: ****
? Tables to monitor (comma-separated): [my_table]
? Bootstrap provider (for initial data loading):
  ▸ PostgreSQL - Load initial data from PostgreSQL
    Script File - Load from JSONL file
    None - No initial data loading
```

**HTTP Source:**
```
Configuring HTTP Source
-----------------------
? Source ID: [http-source]
? Listen host: [0.0.0.0]
? Listen port: [9000]
? Bootstrap provider: [None/Script File/Platform]
```

**gRPC Source:**
```
Configuring gRPC Source
-----------------------
? Source ID: [grpc-source]
? Listen host: [0.0.0.0]
? Listen port: [50051]
? Bootstrap provider: [None/Script File/Platform]
```

**Mock Source:**
```
Configuring Mock Source
-----------------------
? Source ID: [mock-source]
? Data generation interval (milliseconds): [5000]
```

**Platform Source:**
```
Configuring Platform Source
---------------------------
? Source ID: [platform-source]
? Redis URL: [redis://localhost:6379]
? Stream key in Redis: [external-source:changes]
? Consumer group name: [drasi-core]
? Bootstrap provider: [None/Script File/Platform]
```

#### Step 3: Bootstrap Providers

For sources that support bootstrap (initial data loading), you can select a provider:

| Provider | Description |
|----------|-------------|
| **PostgreSQL** | Load initial data from PostgreSQL database (uses source connection) |
| **Script File** | Load from JSONL file (prompts for file path) |
| **Platform** | Load from Query API service (prompts for URL) |
| **None** | No initial data loading |

#### Step 4: Reactions

Select how you want to receive query results:

```
Reactions
---------
? Select reactions (space to select, enter to confirm):
  ▸ [x] Log - Write query results to console
    [x] SSE - Server-Sent Events endpoint
    [ ] HTTP Webhook - POST results to external URL
    [ ] gRPC - Stream results via gRPC
    [ ] Platform - Drasi Platform integration
```

For each selected reaction, you'll be prompted for reaction-specific configuration:

**Log Reaction:**
```
Configuring Log Reaction
------------------------
? Reaction ID: [log-reaction]
```

**SSE Reaction:**
```
Configuring SSE Reaction
------------------------
? Reaction ID: [sse-reaction]
? SSE server host: [0.0.0.0]
? SSE server port: [8081]
```

**HTTP Webhook Reaction:**
```
Configuring HTTP Webhook Reaction
----------------------------------
? Reaction ID: [http-reaction]
? Webhook base URL: [http://localhost:9000]
```

**gRPC Reaction:**
```
Configuring gRPC Reaction
-------------------------
? Reaction ID: [grpc-reaction]
? gRPC endpoint URL: [grpc://localhost:50052]
```

**Platform Reaction:**
```
Configuring Platform Reaction
-----------------------------
? Reaction ID: [platform-reaction]
? Redis URL: [redis://localhost:6379]
```

### Generated Configuration

The init command generates a complete YAML configuration file with:

- Server settings (host, port, log level, persist_index)
- All selected sources with their configurations
- Bootstrap providers for each source (if selected)
- A sample query that references the first source
- All selected reactions

**Example generated configuration:**

```yaml
# Drasi Server Configuration
# Generated with: drasi-server init
#
# Edit this file to customize your configuration.
# See documentation at: https://drasi.io/docs

host: 0.0.0.0
port: 8080
log_level: info
disable_persistence: false
persist_index: false

sources:
  - kind: postgres
    id: postgres-source
    auto_start: true
    bootstrap_provider:
      type: postgres
    host: localhost
    port: 5432
    database: mydb
    user: postgres
    password: secret
    tables:
      - users
      - orders
    slot_name: drasi_slot
    publication_name: drasi_pub
    ssl_mode: prefer

queries:
  - id: my-query
    query: MATCH (n) RETURN n
    queryLanguage: Cypher
    auto_start: true
    enableBootstrap: true
    bootstrapBufferSize: 10000
    sources:
      - source_id: postgres-source

reactions:
  - kind: log
    id: log-reaction
    auto_start: true
    queries:
      - my-query
  - kind: sse
    id: sse-reaction
    auto_start: true
    queries:
      - my-query
    host: 0.0.0.0
    port: 8081
    sse_path: /events
    heartbeat_interval_ms: 30000

# Tips:
# - Use environment variables: ${VAR_NAME:-default}
# - Update 'my-query' with your actual Cypher query
# - Connect reactions to your queries by updating the 'queries' field
```

### Post-Generation Steps

After generating the configuration:

1. **Edit the query**: Replace the sample `MATCH (n) RETURN n` query with your actual Cypher query
2. **Add environment variables**: Replace hardcoded passwords with `${DB_PASSWORD}` syntax
3. **Configure table keys**: For PostgreSQL sources, add `table_keys` for proper change tracking
4. **Test the configuration**: Run `drasi-server validate --config your-config.yaml`
5. **Start the server**: Run `drasi-server --config your-config.yaml`

### Tips

- Use **environment variables** for sensitive data like passwords: `${DB_PASSWORD}`
- The generated config includes helpful comments and tips
- You can always edit the YAML file manually after generation
- Run `drasi-server validate` to check your configuration before starting

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
id: "${SERVER_ID:-production-server}"
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

DrasiServer can be configured in two complementary ways:

1. **Configuration Files (YAML)**: Define your initial setup in a YAML file. Supports both single-instance (simple) and multi-instance (advanced) formats.
2. **REST API**: Create, modify, and delete sources, queries, and reactions at runtime. Changes are automatically persisted to the config file when persistence is enabled.

These approaches work together - you can start with a config file and dynamically add/remove components via the API.

### Configuration File Structure

DrasiServer uses YAML configuration files supporting both single-instance (default) and multi-instance deployments.

#### Single-Instance Configuration (Default)

For most use cases, configure sources, queries, and reactions at the root level:

```yaml
# Server settings
host: 0.0.0.0                           # Bind address
port: 8080                              # API port
log_level: info                         # Log level (trace, debug, info, warn, error)
disable_persistence: false              # Disable automatic config file persistence
persist_index: false                    # Use RocksDB for persistent indexing (default: false)

# Core settings (optional)
id: my-server-id                              # Unique server ID (auto-generated if not set)
default_priority_queue_capacity: 10000        # Default capacity for query/reaction priority queues
default_dispatch_buffer_capacity: 1000        # Default buffer capacity for dispatching

# Data sources
sources:
  - id: unique-source-id
    source_type: postgres               # Source type (postgres, http, grpc, platform, mock, application)
    auto_start: true                    # Start automatically
    # ... source-specific fields

# Continuous queries
queries:
  - id: unique-query-id
    query: "MATCH (n:Node) RETURN n"
    queryLanguage: Cypher
    sources: [unique-source-id]
    auto_start: true

# Reactions
reactions:
  - id: unique-reaction-id
    reaction_type: log
    queries: [unique-query-id]
    auto_start: true
```

#### Multi-Instance Configuration

For advanced use cases requiring multiple isolated DrasiLib instances (separate source/query/reaction namespaces), use the `instances` array:

```yaml
# Server settings (shared across all instances)
host: 0.0.0.0
port: 8080
log_level: info
disable_persistence: false

# Multiple DrasiLib instances
instances:
  - id: analytics
    persist_index: true                 # Per-instance index persistence
    sources:
      - kind: postgres
        id: analytics-db
        auto_start: true
        host: analytics.db.example.com
        port: 5432
        database: analytics
        # ... other postgres fields
    queries:
      - id: high-value-orders
        query: "MATCH (o:Order) WHERE o.total > 1000 RETURN o"
        queryLanguage: Cypher
        sources:
          - source_id: analytics-db
        auto_start: true
    reactions:
      - kind: log
        id: analytics-log
        queries: [high-value-orders]
        auto_start: true

  - id: monitoring
    persist_index: false
    sources:
      - kind: http
        id: metrics-api
        auto_start: true
        host: 0.0.0.0
        port: 9001
    queries:
      - id: alert-threshold
        query: "MATCH (m:Metric) WHERE m.value > m.threshold RETURN m"
        queryLanguage: Cypher
        sources:
          - source_id: metrics-api
        auto_start: true
    reactions:
      - kind: sse
        id: alert-stream
        queries: [alert-threshold]
        auto_start: true
        host: 0.0.0.0
        port: 8082
```

**Key differences in multi-instance mode:**
- Each instance has its own isolated namespace for sources, queries, and reactions
- Component IDs only need to be unique within their instance
- API routes use `/api/v1/instances/{instanceId}/...` prefix (see [REST API](#rest-api) section)
- The first instance is also accessible via convenience routes (`/api/v1/sources`, `/api/v1/queries`, `/api/v1/reactions`)

**Dynamic configuration:**
Both single-instance and multi-instance configurations can be modified at runtime using the REST API. Create, delete, and manage sources, queries, and reactions dynamically. When `disable_persistence: false` (the default), changes are automatically saved to the config file.

#### Full Single-Instance Configuration Reference

```yaml
# Server settings (all at root level)
host: 0.0.0.0                           # Bind address
port: 8080                              # API port
log_level: info                         # Log level (trace, debug, info, warn, error)
disable_persistence: false              # Disable automatic config file persistence
persist_index: false                    # Use RocksDB for persistent indexing (default: false)

# Core settings (optional)
id: my-server-id                              # Unique server ID (auto-generated if not set)
default_priority_queue_capacity: 10000        # Default capacity for query/reaction priority queues
default_dispatch_buffer_capacity: 1000        # Default buffer capacity for dispatching

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
# Root-level capacity settings (support environment variables)
default_priority_queue_capacity: 10000  # Default for all queries and reactions
# default_priority_queue_capacity: "${PRIORITY_QUEUE_CAPACITY:-10000}"

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
- `default_priority_queue_capacity` - Default capacity for all query/reaction priority queues (root level, supports env vars)
- `default_dispatch_buffer_capacity` - Default buffer capacity for dispatching (root level, supports env vars)
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
id: my-server
host: 0.0.0.0
port: 8080
log_level: info
disable_persistence: false  # Enable persistence (default)
persist_index: false         # Use in-memory indexes (default)
sources: []
queries: []
reactions: []
```

### Persistent Indexing

By default, DrasiServer uses in-memory indexes for query state, which provides fast performance but loses data on restart. For production workloads requiring data persistence across restarts, enable RocksDB-based persistent indexing:

```yaml
persist_index: true  # Enable RocksDB persistent indexing
```

**When `persist_index: false` (default):**
- Uses in-memory indexes
- Fastest performance
- Data is lost on restart
- Best for development and stateless deployments

**When `persist_index: true`:**
- Uses RocksDB for persistent storage
- Data stored at `./data/index`
- Query state survives restarts
- Archive indexing enabled (supports `past()` function in queries)
- Best for production workloads requiring durability

**Note:** The index path (`./data/index`) is currently fixed. Future versions may allow customizing this path.

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

Embed DrasiServer in your Rust application using either the builder pattern or configuration files.

### Single-Instance Builder (Default)

For most use cases, use the simple builder pattern:

```rust
use drasi_server::{DrasiServerBuilder, DrasiServer};

#[tokio::main]
async fn main() -> Result<()> {
    let server = DrasiServerBuilder::new()
        .with_port(8080)
        .with_log_level("info")
        .with_source("my-source", source_instance)
        .with_query("my-query", "MATCH (n) RETURN n", vec!["my-source"])
        .with_reaction("my-reaction", reaction_instance)
        .build()
        .await?;

    server.run().await
}
```

### Multi-Instance Builder

For advanced use cases requiring multiple isolated DrasiLib instances:

```rust
use drasi_server::{DrasiServerBuilder, DrasiServer};
use drasi_lib::DrasiLib;

#[tokio::main]
async fn main() -> Result<()> {
    // Build first instance for analytics
    let analytics = DrasiLib::builder()
        .with_id("analytics")
        .with_source(analytics_source)
        .with_query(analytics_query)
        .with_reaction(analytics_reaction)
        .build()
        .await?;

    // Build second instance for monitoring
    let monitoring = DrasiLib::builder()
        .with_id("monitoring")
        .with_source(monitoring_source)
        .with_query(monitoring_query)
        .with_reaction(monitoring_reaction)
        .build()
        .await?;

    // Create server with multiple instances
    let server = DrasiServerBuilder::new()
        .with_port(8080)
        .with_log_level("info")
        .with_instance(analytics)      // First instance (also available via legacy routes)
        .with_instance(monitoring)     // Second instance
        .build()
        .await?;

    server.run().await
}
```

### Loading from Configuration File

```rust
use drasi_server::DrasiServer;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Load from config file (supports both single and multi-instance configs)
    let server = DrasiServer::new(
        PathBuf::from("config.yaml"),
        8080
    ).await?;

    server.run().await
}
```

## REST API

DrasiServer provides a comprehensive REST API for runtime control. The API uses URL-based versioning with all endpoints prefixed with `/api/v1/`.

### API Versioning

The API uses URL-based versioning for stability and clarity:
- All API endpoints are prefixed with `/api/v1/`
- `/health` remains at root level (operational endpoint)
- `/api/versions` lists all available API versions

### API Route Patterns

DrasiServer supports both **convenience routes** for the default instance and **instance-specific routes** for multi-instance deployments:

| Route Type | Pattern | Description |
|------------|---------|-------------|
| **Convenience Routes** | `/api/v1/sources`, `/api/v1/queries`, `/api/v1/reactions` | Access the first/default instance |
| **Instance Routes** | `/api/v1/instances/{instanceId}/sources` | Access a specific instance by ID |

**Choosing the right routes:**

- **Single-instance deployments**: Use convenience routes (`/api/v1/sources`, `/api/v1/queries`, `/api/v1/reactions`) for clean, straightforward API calls
- **Multi-instance deployments**: Use instance routes (`/api/v1/instances/{instanceId}/...`) to target specific instances
- **Mixed approach**: The first configured instance is always accessible via convenience routes, so you can use those for the primary instance and instance routes for secondary instances

**Dynamic configuration via Web API:**

All component management (sources, queries, reactions) can be performed dynamically at runtime through the REST API:
- **Create components**: `POST /api/v1/sources`, `POST /api/v1/queries`, `POST /api/v1/reactions`
- **Delete components**: `DELETE /api/v1/sources/{id}`, `DELETE /api/v1/queries/{id}`, `DELETE /api/v1/reactions/{id}`
- **Lifecycle control**: `POST /api/v1/{component}/{id}/start`, `POST /api/v1/{component}/{id}/stop`

Changes made via the API are automatically persisted to the config file when `disable_persistence: false` (the default).

### Instances API

```bash
# List all configured instances
GET /api/v1/instances
# Returns: {"success": true, "data": [{"id": "analytics"}, {"id": "monitoring"}]}
```

### Health Check

```bash
# Check server health (unversioned operational endpoint)
GET /health
# Returns: {"status": "ok", "timestamp": "2025-01-15T12:00:00Z"}

# List available API versions
GET /api/versions
# Returns: {"versions": ["v1"], "current": "v1"}
```

### Sources API

```bash
# List all sources (convenience route - default instance)
GET /api/v1/sources

# List all sources (instance-specific)
GET /api/v1/instances/{instanceId}/sources

# Get source details
GET /api/v1/instances/{instanceId}/sources/{id}
GET /api/v1/sources/{id}

# Create a new source
POST /api/v1/instances/{instanceId}/sources
POST /api/v1/sources
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
DELETE /api/v1/instances/{instanceId}/sources/{id}
DELETE /api/v1/sources/{id}

# Start a source
POST /api/v1/instances/{instanceId}/sources/{id}/start
POST /api/v1/sources/{id}/start

# Stop a source
POST /api/v1/instances/{instanceId}/sources/{id}/stop
POST /api/v1/sources/{id}/stop
```

### Queries API

```bash
# List all queries
GET /api/v1/instances/{instanceId}/queries
GET /api/v1/queries

# Get query details
GET /api/v1/instances/{instanceId}/queries/{id}
GET /api/v1/queries/{id}

# Create a new query
POST /api/v1/instances/{instanceId}/queries
POST /api/v1/queries
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
DELETE /api/v1/instances/{instanceId}/queries/{id}
DELETE /api/v1/queries/{id}

# Start a query
POST /api/v1/instances/{instanceId}/queries/{id}/start
POST /api/v1/queries/{id}/start

# Stop a query
POST /api/v1/instances/{instanceId}/queries/{id}/stop
POST /api/v1/queries/{id}/stop

# Get current query results
GET /api/v1/instances/{instanceId}/queries/{id}/results
GET /api/v1/queries/{id}/results
```

### Reactions API

```bash
# List all reactions
GET /api/v1/instances/{instanceId}/reactions
GET /api/v1/reactions

# Get reaction details
GET /api/v1/instances/{instanceId}/reactions/{id}
GET /api/v1/reactions/{id}

# Create a new reaction
POST /api/v1/instances/{instanceId}/reactions
POST /api/v1/reactions
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
DELETE /api/v1/instances/{instanceId}/reactions/{id}
DELETE /api/v1/reactions/{id}

# Start a reaction
POST /api/v1/instances/{instanceId}/reactions/{id}/start
POST /api/v1/reactions/{id}/start

# Stop a reaction
POST /api/v1/instances/{instanceId}/reactions/{id}/stop
POST /api/v1/reactions/{id}/stop
```

### API Documentation

Interactive API documentation is available at:
- Swagger UI: `http://localhost:8080/api/v1/docs/`
- OpenAPI spec: `http://localhost:8080/api/v1/openapi.json`

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
# Health check endpoint (unversioned)
GET /health

# Component status checks
GET /api/v1/sources/{id}
GET /api/v1/queries/{id}
GET /api/v1/reactions/{id}
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
- Verify source is running: `GET /api/v1/sources/{id}`
- Check source subscription: `GET /api/v1/queries/{id}`
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
