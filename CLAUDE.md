# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

This is the Drasi Server repository - a standalone server wrapper around DrasiServerCore that provides REST API, configuration management, and server lifecycle features for Microsoft's Drasi data processing system. The actual core functionality is provided by the external drasi-server-core library located at `../drasi-server-core/`.

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

1. **Server** (`src/server.rs`) - Main server implementation that wraps DrasiServerCore
2. **API** (`src/api/`) - REST API implementation with OpenAPI documentation
3. **Builder** (`src/builder.rs`) - Builder pattern for server construction
4. **Main** (`src/main.rs`) - CLI entry point for standalone server

### Core Components (External Dependency)

The actual data processing functionality is provided by drasi-server-core:

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

### Server Configuration

The server uses YAML configuration files (default: `config/server.yaml`):
- Sources defined under `sources:`
- Queries defined under `queries:` 
- Reactions defined under `reactions:`
- Runtime settings under `runtime:`

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
- Configuration persisted to YAML files
- In-memory state for active components

### Bootstrap Mechanism
- Queries can request initial data from sources
- Sources filter bootstrap data by labels from Cypher queries
- Bootstrap completes before normal data flow begins

## Library Usage

The server can be used as a library in other Rust projects:

```rust
use drasi_server::{DrasiServerBuilder, ApplicationSourceHandle};

let builder = DrasiServerBuilder::new();
let server = builder.with_sources(...).build();
let handles = server.start().await?;
```

## Dependencies

### Core Dependencies
- Rust edition 2021 minimum
- `drasi-server-core` - External library at `../drasi-server-core/`
- Tokio for async runtime
- Axum for HTTP server
- Serde for serialization
- Utoipa for OpenAPI documentation

### Important Notes
- The core functionality is provided by the external `drasi-server-core` library
- Config types from drasi-server-core don't implement ToSchema trait, limiting OpenAPI documentation
- All data processing logic resides in drasi-server-core
- This repository focuses on API, configuration, and server lifecycle management