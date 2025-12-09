# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

This is the Drasi Server repository - a standalone server wrapper around DrasiLib that provides REST API, configuration management, and server lifecycle features for Microsoft's Drasi data processing system. The actual core functionality is provided by the external drasi-lib library located at `../drasi-lib/`.

## Development Commands

### Build and Run
- Build: `cargo build`
- Build release: `cargo build --release`
- Run server: `cargo run` or `cargo run -- --config config/server.yaml`
- Run with custom port: `cargo run -- --port 8080`
- Check compilation: `cargo check`

### Testing
- Run all tests: `cargo test`
- Run unit tests only: `cargo test --lib`
- Run specific test: `cargo test test_name`
- Run integration tests: `./tests/run_working_tests.sh`
- Run with logging: `RUST_LOG=debug cargo test -- --nocapture`

### Code Quality
- Format code: `cargo fmt`
- Run linter: `cargo clippy`
- Fix linter warnings: `cargo clippy --fix`

## Architecture

### DrasiServer Components (This Repository)

This repository contains only the server wrapper functionality:

1. **Server** (`src/server.rs`) - Main server implementation that wraps DrasiLib
2. **API** (`src/api/`) - REST API implementation with OpenAPI documentation
3. **Builder** (`src/builder.rs`) - Builder pattern for server construction
4. **Main** (`src/main.rs`) - CLI entry point for standalone server

### Core Components (External Dependency)

The actual data processing functionality is provided by drasi-lib:

1. **Sources** - Data ingestion from various systems (PostgreSQL, HTTP, gRPC, etc.)
2. **Queries** - Continuous Cypher queries over data with joins and bootstrap
3. **Reactions** - Automated responses to changes (webhooks, SSE, logging, etc.)
4. **Channels** - Inter-component communication
5. **Routers** - Message routing between components

### Data Flow Architecture

```
Sources → Bootstrap Router → Queries → Data Router → Reactions
         ↓                           ↓
    Label Extraction          Query Results
         ↓                           ↓
    Filtered Data              Change Events
```

### Channel Communication

All components communicate through async channels:
- Bootstrap requests flow through `BootstrapRouter`
- Data changes flow through `DataRouter` 
- Subscriptions managed by `SubscriptionRouter`
- Each component has send/receive channel pairs

## Configuration

### Configuration File Support

DrasiServer supports YAML configuration files for defining server settings and queries:

```bash
cargo run -- --config config/server.yaml
```

**Example configuration file:**
```yaml
server:
  host: "0.0.0.0"
  port: 8080
  log_level: "info"
  disable_persistence: false  # Enable persistence (default)

queries:
  - id: "high-temp"
    query: "MATCH (s:Sensor) WHERE s.temperature > 75 RETURN s"
    query_language: "cypher"
    source_subscriptions:
      - source_id: "sensors"
    auto_start: true
```

**Important**: Sources and reactions are plugins that must be provided programmatically. Only queries can be defined via configuration files.

### Configuration Persistence

DrasiServer separates two independent concepts:

1. **Persistence** - Whether API changes are saved to the config file
2. **Read-Only Mode** - Whether API changes are allowed at all

**Persistence is enabled when:**
- Config file is provided on startup (`--config path/to/config.yaml`)
- Config file is writable
- `disable_persistence: false` in server settings (default)

**Persistence is disabled when:**
- No config file provided (server starts with empty configuration)
- Config file is read-only
- `disable_persistence: true` in server settings

**Read-Only mode is enabled ONLY when:**
- Config file is not writable (file permissions prevent writing)

**Important distinction:**
- `disable_persistence: true` → API mutations are allowed but NOT saved to config file
- Read-only config file → API mutations are blocked entirely
- This allows dynamic query creation without persistence (useful for programmatic usage)

**Behavior:**
- When persistence enabled: all API mutations (create/delete queries) are automatically saved to the config file using atomic writes (temp file + rename) to prevent corruption
- When persistence disabled: API mutations work but changes are lost on restart
- When read-only: all create/delete operations via API are rejected

### Builder-Based Configuration

DrasiServer also supports a builder pattern for programmatic configuration. Sources and reactions are provided as plugin instances:

```rust
use drasi_server::DrasiServerBuilder;
use drasi_lib::Query;

let server = DrasiServerBuilder::new()
    .with_id("my-server")
    .with_host_port("0.0.0.0", 8080)
    .with_source(my_source_instance)  // Plugin instance
    .add_query(
        Query::cypher("my-query")
            .query("MATCH (n) RETURN n")
            .from_source("my-source")
            .build()
    )
    .with_reaction(my_reaction_instance)  // Plugin instance
    .build()
    .await?;

server.run().await?;
```

### Component Types

**Internal Sources:**
- `postgres` - Direct PostgreSQL connection
- `postgres_replication` - PostgreSQL WAL replication
- `http` - HTTP endpoint polling
- `grpc` - gRPC streaming
- `mock` - Testing source
- `application` - Programmatic API

**Internal Reactions:**
- `http` - HTTP webhook
- `grpc` - gRPC stream
- `sse` - Server-Sent Events
- `log` - Console logging
- `application` - Programmatic API

## Testing Approach

### Test Organization
- Unit tests: In module files or `src/*/tests.rs`
- Integration tests: `tests/*.rs`
- API tests: `tests/api/`
- Protocol tests: `tests/grpc/`, `tests/http/`, `tests/postgres/`
- End-to-end tests: Files ending with `_e2e_test.rs`

### Running Tests
- Always run `cargo test` before committing
- Use `./tests/run_working_tests.sh` for comprehensive testing
- Check specific functionality with targeted tests

## API Endpoints

The server exposes a REST API on port 8080 by default:

- `GET /health` - Health check
- `GET /openapi.json` - OpenAPI specification
- `GET /swagger-ui/` - Interactive API documentation

Component management:
- `GET/POST /api/sources` - Source CRUD operations
- `GET/POST /api/queries` - Query CRUD operations  
- `GET/POST /api/reactions` - Reaction CRUD operations
- `POST /api/{component}/{id}/start` - Start component
- `POST /api/{component}/{id}/stop` - Stop component
- `GET /api/queries/{id}/results` - Get query results

## Important Patterns

### Error Handling
- Use `anyhow::Result` for functions that can fail
- Custom `DrasiError` type for domain-specific errors
- Proper error propagation with `?` operator

### Async/Await
- All I/O operations are async using Tokio
- Components run in separate Tokio tasks
- Channel communication is async

### State Management
- Components track their status (Stopped/Starting/Running/Stopping/Failed)
- Configuration persisted to YAML files (when persistence enabled)
- In-memory state for active components

### Bootstrap Mechanism
- Queries can request initial data from sources
- Sources filter bootstrap data by labels from Cypher queries
- Bootstrap completes before normal data flow begins

### Logging Conventions

**Use log macros for operational logging:**
- `error!()` - For errors that require attention
- `warn!()` - For warnings and non-fatal issues
- `info!()` - For important operational information
- `debug!()` - For detailed debugging information

**When to use `println!`:**
- CLI help output and usage messages
- Setup scripts (like `basic_setup.rs`)
- Direct user interaction in binaries
- Server startup banners in `main.rs` and `server.rs` (user-facing CLI output)

**Never use `println!` for:**
- Operational logging in library code
- Error messages
- Debugging output
- Progress updates

**Example:**
```rust
// Good: Use log macros for operational logging
info!("Server starting on port {}", port);
warn!("Config file not found, using defaults");
error!("Failed to connect to database: {}", err);
debug!("Processing message: {:?}", msg);

// Good: Use println! for CLI user output
println!("Starting Drasi Server");
println!("  API Port: {}", port);

// Bad: Don't use println! for operational logging
// println!("Error: Connection failed"); // Use error!() instead
// println!("Debug: Processing message"); // Use debug!() instead
```

## Library Usage

The server can be used as a library in other Rust projects:

```rust
use drasi_server::DrasiServerBuilder;
use drasi_lib::Query;

let server = DrasiServerBuilder::new()
    .with_id("my-server")
    .with_host_port("0.0.0.0", 8080)
    .with_source(my_source)
    .add_query(
        Query::cypher("my-query")
            .query("MATCH (n) RETURN n")
            .from_source("my-source")
            .build()
    )
    .build()
    .await?;

server.run().await?;
```

## Dependencies

### Core Dependencies
- Rust edition 2021 minimum
- `drasi-lib` - External library at `../drasi-lib/`
- Tokio for async runtime
- Axum for HTTP server
- Serde for serialization
- Utoipa for OpenAPI documentation

### Important Notes
- The core functionality is provided by the external `drasi-lib` library
- Sources and reactions are plugins that must be provided as instances (no YAML configuration)
- Queries can be created via the builder pattern
- All data processing logic resides in drasi-lib
- This repository focuses on API and server lifecycle management