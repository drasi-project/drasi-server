# Drasi Server

A production-ready standalone server for building change-driven solutions with Microsoft Drasi. DrasiServer provides a REST API, configuration management, and lifecycle control around the powerful [DrasiServerCore](https://github.com/drasi-project/drasi-server-core/blob/main/README.md) event processing engine.

## Overview

DrasiServer enables you to build intelligent, event-driven applications that continuously detect and react to critical changes in your data systems - without polling, without delays, and without complexity. It wraps the DrasiServerCore library with enterprise-ready features including:

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

# Clone the repository with submodules
git clone --recursive https://github.com/drasi-project/drasi-server.git
cd drasi-server
```

### 2. Create a Configuration

```yaml
# config/server.yaml
server:
  host: 127.0.0.1
  port: 8080
  log_level: info

sources:
  - id: inventory-db
    source_type: internal.postgres
    auto_start: true
    properties:
      host: localhost
      port: 5432
      database: inventory
      user: postgres

queries:
  - id: low-stock-detector
    query: |
      MATCH (p:Product)
      WHERE p.quantity < p.reorder_threshold
      RETURN p.id, p.name, p.quantity, p.reorder_threshold
    sources: [inventory-db]
    auto_start: true

reactions:
  - id: alert-webhook
    reaction_type: internal.http
    auto_start: true
    properties:
      endpoint: https://alerts.example.com/webhook
      method: POST
    queries: [low-stock-detector]
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
- **PostgreSQL** - Monitor table changes via WAL replication
- **HTTP Endpoints** - Poll REST APIs for updates
- **gRPC Streams** - Subscribe to real-time data feeds
- **Application Sources** - Programmatically inject events

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
- **HTTP Webhooks** - Call external APIs
- **Server-Sent Events (SSE)** - Stream to browsers
- **gRPC Streams** - Push to services
- **Application Reactions** - Custom code handlers

## Building from Source

### Prerequisites
- Rust 1.70 or higher
- Git with submodule support

### Build Steps

```bash
# Clone with submodules
git clone --recursive https://github.com/drasi-project/drasi-server.git
cd drasi-server

# If you already cloned without --recursive
git submodule update --init --recursive

# Build debug version
cargo build

# Build optimized release version
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

## Configuration

### Configuration File Structure

DrasiServer uses YAML configuration files with the following structure:

```yaml
# Server settings
server:
  host: 0.0.0.0              # Bind address
  port: 8080                 # API port
  log_level: info            # Log level (trace, debug, info, warn, error)
  max_connections: 100       # Max concurrent connections
  shutdown_timeout_seconds: 30  # Graceful shutdown timeout
  disable_persistence: false    # Disable config persistence

# Data sources
sources:
  - id: unique-source-id
    source_type: internal.postgres  # Source type
    auto_start: true               # Start automatically
    properties:                    # Source-specific properties
      host: localhost
      database: mydb

# Continuous queries  
queries:
  - id: unique-query-id
    query: |                       # Cypher query
      MATCH (n:Node)
      RETURN n
    sources: [source-id]           # Source subscriptions
    auto_start: true
    joins:                         # Optional joins
      - id: RELATIONSHIP_TYPE
        keys:
          - label: Node1
            property: join_key
          - label: Node2
            property: join_key

# Reactions
reactions:
  - id: unique-reaction-id
    reaction_type: internal.http   # Reaction type
    queries: [query-id]            # Query subscriptions
    auto_start: true
    properties:                    # Reaction-specific properties
      endpoint: https://example.com
```

### Environment Variables

```bash
# Override config file location
DRASI_CONFIG=/path/to/config.yaml

# Set log level
RUST_LOG=drasi_server=debug

# Override port
DRASI_PORT=9000
```

## Library Usage

Embed DrasiServer in your Rust application:

```rust
use drasi_server::{DrasiServerBuilder, DrasiServer};

#[tokio::main]
async fn main() -> Result<()> {
    // Using builder pattern
    let server = DrasiServerBuilder::new()
        .with_api_port(8080)
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

### Component Management

```bash
# List all sources
GET /sources

# Create a new source
POST /sources
Content-Type: application/json
{
  "id": "new-source",
  "source_type": "internal.postgres",
  "properties": {...}
}

# Start/stop components
POST /sources/{id}/start
POST /sources/{id}/stop

# Get query results
GET /queries/{id}/results
```

### API Documentation

Interactive API documentation is available at:
- Swagger UI: `http://localhost:8080/docs`
- OpenAPI spec: `http://localhost:8080/api-docs/openapi.json`

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

## Production Deployment

### Health Checks

```bash
# Liveness probe
GET /health

# Readiness probe (checks component status)
GET /health/ready
```

### Monitoring

DrasiServer exposes metrics and status information:

```bash
# Component status
GET /sources/{id}/status
GET /queries/{id}/status
GET /reactions/{id}/status

# Runtime metrics
GET /metrics
```

### Security Considerations

- Run in **read-only mode** for production deployments
- Use **TLS/HTTPS** for API endpoints
- Implement **authentication** via reverse proxy
- **Validate** all query inputs to prevent injection
- **Limit** resource consumption per query

## Troubleshooting

### Common Issues

**Submodule errors during build:**
```bash
git submodule update --init --recursive
```

**Port already in use:**
```bash
# Use a different port
cargo run -- --port 9090
```

**Query not receiving data:**
- Verify source is running: `GET /sources/{id}/status`
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

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## License

DrasiServer is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.

## Related Projects

- [DrasiServerCore](https://github.com/drasi-project/drasi-server-core) - Core event processing engine
- [Drasi](https://github.com/drasi-project/drasi) - Main Drasi project
- [Drasi Documentation](https://drasi.io/docs) - Complete documentation

## Support

- **Issues**: [GitHub Issues](https://github.com/drasi-project/drasi-server/issues)
- **Discussions**: [GitHub Discussions](https://github.com/drasi-project/drasi/discussions)
- **Blog**: [Drasi Blog](https://techcommunity.microsoft.com/blog/linuxandopensourceblog)

---

*Built with d by the Drasi Project team*