# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

This is the Drasi Server repository - a standalone server wrapper around DrasiLib that provides REST API, configuration management, and server lifecycle features for Microsoft's Drasi data processing system. The actual core functionality is provided by the external drasi-lib library located at `../drasi-lib/`.

## Development Commands

### Build and Run
- Build: `cargo build`
- Build release: `cargo build --release`
- Cross-compile: `make build-cross TARGET=x86_64-pc-windows-gnu`
- Run server: `cargo run` or `cargo run -- --config config/server.yaml`
- Run with custom port: `cargo run -- --port 8080`
- Run with plugin verification: `cargo run -- --verify-plugins --config config/server.yaml`
- Run with UI disabled: `cargo run -- --disable-ui`
- Run with UI enabled (override config): `cargo run -- --enable-ui`
- Validate config (structure only): `cargo run -- validate --config config/server.yaml`
- Validate config (with plugins): `cargo run -- validate --config config/server.yaml --plugins-dir ./plugins`
- Check compilation: `cargo check`

### Plugin Loading
Plugins (sources, reactions, bootstrap providers) are loaded at runtime as cdylib shared libraries (`.so`/`.dylib`/`.dll`) from a `plugins/` directory next to the binary. Each plugin is self-contained with its own tokio runtime, communicating via a stable C ABI. Plugin building is managed by drasi-core, not this repository.

**Important: `[patch.crates-io]` does NOT affect plugins.** Cargo patches only affect compile-time dependency resolution for the server binary. Plugins are separate shared libraries loaded at runtime — they must be built separately. When developing with local drasi-core changes, always use `make build-local-plugins` to rebuild plugins from local source. Registry-downloaded plugins (`autoInstallPlugins: true`) will NOT be ABI-compatible with local drasi-core changes.

- Build all plugins from local drasi-core (release): `make build-local-plugins`
- Build all plugins from local drasi-core (debug): `make build-local-plugins-debug`
- Build test-only plugins (mock, log, scriptfile): `make build-test-plugins`

**Local directory plugin sources:** The `pluginRegistry` config field (and `--registry` CLI flag) accepts local filesystem paths in addition to OCI registry URLs. When a path is detected (e.g., `/path/to/plugins`, `./plugins`, `../drasi-core/target/debug/plugins`, `file:///opt/plugins`), the system scans the directory for plugin binaries instead of contacting an OCI registry. This is useful for development workflows where plugins are built locally. Detection is cross-platform: Unix absolute paths, relative paths (`./`, `../`), home-relative (`~/`), `file://` URIs, Windows drive letters, and UNC paths are all recognized as local directories.

### Testing
- Run all tests: `cargo test`
- Run unit tests only: `cargo test --lib`
- Run specific test: `cargo test test_name`
- Run integration tests: `./tests/run_working_tests.sh`
- Run plugin smoke tests: `make test-smoke`
- Run with logging: `RUST_LOG=debug cargo test -- --nocapture`
- Run host-sdk integration tests: `cd ../drasi-core && cargo test -p drasi-host-sdk --test integration_test`

### Code Quality
- Format code: `cargo fmt`
- Run linter: `cargo clippy`
- Fix linter warnings: `cargo clippy --fix`

## Architecture

### DrasiServer Components (This Repository)

This repository contains only the server wrapper functionality:

1. **Server** (`src/server.rs`) - Main server implementation that wraps DrasiLib
2. **API** (`src/api/`) - REST API implementation with OpenAPI documentation
   - `v1/` - API version 1 handlers, routes, and OpenAPI spec
   - `v1/plugin_handlers.rs` - Plugin management API endpoints
   - `shared/` - Common handlers, error types, and response types shared across versions
   - `version.rs` - API version constants and utilities
   - `models/` - Data Transfer Objects (DTOs)
   - `mappings/` - DTO to domain model conversions
3. **Builder** (`src/builder.rs`) - Builder pattern for server construction
4. **Main** (`src/main.rs`) - CLI entry point for standalone server
5. **Dynamic Loading** (`src/dynamic_loading.rs`) - Runtime plugin loading from shared libraries
6. **Plugin Operations** (`src/plugin_operations.rs`) - Shared plugin management service used by CLI, init, startup, and API
7. **Plugin Orchestrator** (`src/plugin_orchestrator.rs`) - Server-level plugin lifecycle coordination with drain-then-retire
8. **Plugin Registry** (`src/plugin_registry.rs`) - Re-exports from host-sdk: mutable registry with `Arc<RwLock<PluginRegistry>>`

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
apiVersion: drasi.io/v1

# Server identification
id: "my-server"  # Unique server ID (defaults to UUID if not specified)

# Server settings
host: "0.0.0.0"
port: 8080
logLevel: "info"
persistConfig: true  # Enable persistence (default)
persistIndex: false  # Use RocksDB for persistent indexing (default: false, uses in-memory)
verifyPlugins: true  # Enable cosign signature verification for downloaded plugins (default: false)
enableUi: true       # Enable the web UI at /ui (default: true)

# Hot-reload plugin settings (default: all disabled)
# hotReloadPlugins: false          # Enable filesystem watching for plugin changes
# hotReloadDebounceMs: 2000        # Debounce window in milliseconds
# hotReloadMode: "upgrade"         # "upgrade" (drain-then-retire) or "side-by-side"

# Optional trusted identities for plugin signature verification
# trustedIdentities:
#   - issuer: "https://accounts.google.com"
#     subjectPattern: "builder@my-org.iam.gserviceaccount.com"

# Optional state store for plugin state persistence
# stateStore:
#   kind: redb
#   path: ./data/state.redb

# Optional capacity defaults (cascades to queries/reactions)
# Supports environment variables like other fields
# defaultPriorityQueueCapacity: 10000
# defaultPriorityQueueCapacity: "${PRIORITY_QUEUE_CAPACITY:-10000}"
# defaultDispatchBufferCapacity: 1000
# defaultDispatchBufferCapacity: "${DISPATCH_BUFFER_CAPACITY:-1000}"

# Sources (parsed into plugin instances)
sources:
  - kind: mock
    id: "sensors"
    autoStart: true

# Queries
queries:
  - id: "high-temp"
    query: "MATCH (s:Sensor) WHERE s.temperature > 75 RETURN s"
    queryLanguage: Cypher
    sources:
      - sourceId: "sensors"
    autoStart: true

# Reactions
reactions:
  - kind: log
    id: "log-temps"
    queries:
      - "high-temp"
    autoStart: true
```

For multiple DrasiLib instances, use the `instances` array (legacy single-instance fields continue to work and map to the first instance):

```yaml
apiVersion: drasi.io/v1
host: "0.0.0.0"
port: 8080
logLevel: "info"
persistConfig: true

instances:
  - id: "analytics"
    persistIndex: true
    stateStore:
      kind: redb
      path: ./data/analytics-state.redb
    sources:
      - kind: mock
        id: "sensors"
        autoStart: true
    queries:
      - id: "high-temp"
        query: "MATCH (s:Sensor) WHERE s.temperature > 75 RETURN s"
        queryLanguage: Cypher
        sources:
          - sourceId: "sensors"
  - id: "monitoring"
    sources: []
    queries: []
    reactions: []
```

The REST API is exposed under `/api/v1/instances/{instanceId}/...` for multi-instance access; the first configured instance is also accessible via convenience routes at `/api/v1/sources`, `/api/v1/queries`, and `/api/v1/reactions`.

**Important**: Sources and reactions are plugins that must be provided programmatically or via the configuration file's tagged enum format. Queries can also be defined via configuration files.

### Configuration Persistence

Persistence uses a snapshot-based approach: when saving, `ConfigPersistence::save()` calls `snapshot_configuration()` on each DrasiLib instance via the ComponentGraph. The ComponentGraph is the single source of truth — there are no shadow caches or separate registration steps. Mutations flow through the ComponentGraph, and the persisted YAML is reconstructed from the current graph state at save time.

DrasiServer separates two independent concepts:

1. **Persistence** - Whether API changes are saved to the config file
2. **Read-Only Mode** - Whether API changes are allowed at all

**Persistence is enabled when:**
- Config file is provided on startup (`--config path/to/config.yaml`)
- Config file is writable
- `persistConfig: true` in server settings (default)

**Persistence is disabled when:**
- No config file provided (server starts with empty configuration)
- Config file is read-only
- `persistConfig: false` in server settings

**Read-Only mode is enabled ONLY when:**
- Config file is not writable (file permissions prevent writing)

**Important distinction:**
- `persistConfig: false` → API mutations are allowed but NOT saved to config file
- Read-only config file → API mutations are blocked entirely
- This allows dynamic query creation without persistence (useful for programmatic usage)

**Behavior:**
- When persistence enabled: `save()` snapshots component state from the ComponentGraph and writes to YAML using atomic writes (temp file + rename) to prevent corruption
- When persistence disabled: API mutations work but changes are lost on restart
- When read-only: all create/delete operations via API are rejected

### Builder-Based Configuration

DrasiServer also supports a builder pattern for programmatic configuration. Sources, reactions, and state store providers are provided as plugin instances:

```rust
use drasi_server::DrasiServerBuilder;
use drasi_lib::Query;
use drasi_state_store_redb::RedbStateStoreProvider;
use std::sync::Arc;

// Create a state store provider (optional)
let state_store = RedbStateStoreProvider::new("./data/state.redb")?;

let server = DrasiServerBuilder::new()
    .with_id("my-server")
    .with_host_port("0.0.0.0", 8080)
    .with_state_store_provider(Arc::new(state_store))  // Optional: for plugin state persistence
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
- Host-SDK integration tests: `../drasi-core/components/host-sdk/tests/integration_test.rs`
  - Tests load real cdylib plugins (mock source, log reaction) and exercise the full
    dynamic loading pipeline including metadata validation, callbacks, factory invocation,
    and lifecycle management through FFI vtables.
  - Prerequisites: build plugins in drasi-core with `make build-cdylib-plugins`

### Running Tests
- Always run `cargo test` before committing
- Use `./tests/run_working_tests.sh` for comprehensive testing
- Check specific functionality with targeted tests

## API Endpoints

The server exposes a versioned REST API on port 8080 by default. All API endpoints use URL-based versioning with the `/api/v1/` prefix.

### API Versioning

- `GET /health` - Health check (unversioned operational endpoint)
- `GET /api/versions` - List available API versions
- `GET /api/v1/openapi.json` - OpenAPI specification for v1
- `GET /api/v1/docs/` - Interactive Swagger UI documentation

### Instance Management

- `GET /api/v1/instances` - List all DrasiLib instances
- `GET /api/v1/instances/{instanceId}/snapshot` - Get configuration snapshot of an instance
- `POST /api/v1/instances/{instanceId}/clone` - Clone components from another instance

### Component Management (Instance-Specific)

Sources:
- `GET /api/v1/instances/{instanceId}/sources` - List sources
- `POST /api/v1/instances/{instanceId}/sources` - Create source (returns 409 if exists)
- `PUT /api/v1/instances/{instanceId}/sources/{id}` - Upsert source (create or update)
- `GET /api/v1/instances/{instanceId}/sources/{id}` - Get source status
- `DELETE /api/v1/instances/{instanceId}/sources/{id}` - Delete source
- `POST /api/v1/instances/{instanceId}/sources/{id}/start` - Start source
- `POST /api/v1/instances/{instanceId}/sources/{id}/stop` - Stop source

Queries:
- `GET /api/v1/instances/{instanceId}/queries` - List queries
- `POST /api/v1/instances/{instanceId}/queries` - Create query (returns 409 if exists)
- `GET /api/v1/instances/{instanceId}/queries/{id}` - Get query config
- `DELETE /api/v1/instances/{instanceId}/queries/{id}` - Delete query
- `POST /api/v1/instances/{instanceId}/queries/{id}/start` - Start query
- `POST /api/v1/instances/{instanceId}/queries/{id}/stop` - Stop query
- `GET /api/v1/instances/{instanceId}/queries/{id}/results` - Get query results

Reactions:
- `GET /api/v1/instances/{instanceId}/reactions` - List reactions
- `POST /api/v1/instances/{instanceId}/reactions` - Create reaction (returns 409 if exists)
- `PUT /api/v1/instances/{instanceId}/reactions/{id}` - Upsert reaction (create or update)
- `GET /api/v1/instances/{instanceId}/reactions/{id}` - Get reaction status
- `DELETE /api/v1/instances/{instanceId}/reactions/{id}` - Delete reaction
- `POST /api/v1/instances/{instanceId}/reactions/{id}/start` - Start reaction
- `POST /api/v1/instances/{instanceId}/reactions/{id}/stop` - Stop reaction

### Convenience Routes (First Instance)

For convenience, the first configured instance is accessible via shortened routes:
- `GET/POST /api/v1/sources` - Sources of the first instance
- `GET/POST /api/v1/queries` - Queries of the first instance
- `GET/POST /api/v1/reactions` - Reactions of the first instance

### Plugin Management (Server-Wide)

Plugin endpoints are server-wide (not per-instance) since plugins are shared across all instances:
- `GET /api/v1/plugins` - List all loaded plugins with status
- `GET /api/v1/plugins/{pluginId}` - Get plugin details
- `POST /api/v1/plugins/load` - Load a plugin from disk
- `POST /api/v1/plugins/install` - Install from remote OCI registry
- `POST /api/v1/plugins/{pluginId}/upgrade` - Upgrade (drain-then-replace)
- `POST /api/v1/plugins/{pluginId}/promote` - Promote side-by-side version to incumbent
- `POST /api/v1/plugins/{pluginId}/retire` - Retire a plugin (with optional `?force=true`)
- `GET /api/v1/plugins/{pluginId}/dependents` - List dependent components
- `GET /api/v1/plugins/kinds` - List all available kinds (sources, reactions, bootstrappers)
- `GET /api/v1/plugins/kinds/{category}/{kind}/schema` - Get config schema for a kind
- `GET /api/v1/plugins/events` - SSE stream for plugin lifecycle events

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
- Plugins track their lifecycle (Loaded/Active/Draining/Retired/Failed)
- Plugin registry uses `Arc<RwLock<PluginRegistry>>` for concurrent read/write access
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
- Plugins (sources, reactions, bootstrappers) use `cdylib` crate-type and export `drasi_plugin_init()` / `drasi_plugin_metadata()` entry points
- Plugins are loaded as self-contained cdylib `.so`/`.dylib`/`.dll` files at runtime via `drasi-host-sdk`
- Each cdylib plugin has its own tokio runtime and communicates with the host via `#[repr(C)]` vtable structs (stable C ABI)
- Plugin metadata validation checks SDK version (major.minor match) and target triple at load time
- All data processing logic resides in drasi-lib
- This repository focuses on API and server lifecycle management
- Plugin signature verification is available via `--verify-plugins` CLI flag or `verifyPlugins: true` in config. Uses Sigstore/cosign keyless verification against the Rekor transparency log.
- **Plugin lifecycle management**: Plugins can be loaded, upgraded (drain-then-retire), and retired at runtime via the `/api/v1/plugins/` API
- **Plugin registry is mutable**: Uses `Arc<RwLock<PluginRegistry>>` — shared types (PluginRegistry, PluginLockfile, PluginLifecycleManager, PluginWatcher) live in `drasi-host-sdk`, re-exported by this repo
- **Component metadata**: Sources and reactions carry `pluginId` and `pluginGeneration` in their ComponentGraph metadata
- **Shared plugin operations**: `PluginOperations` in `src/plugin_operations.rs` provides the single source of truth for plugin management used by CLI, init, startup, and API
