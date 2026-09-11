# Drasi Plugin Architecture

This document preserves the plugin system's architectural rationale and design
sketches. For current build, installation, verification, and run commands, use
the [Server setup guide](setup.md).

**Status:** The normal Server build now loads cdylib plugins at runtime; the old
static/dynamic Server feature modes are retired. The FFI layouts and authoring
snippets below are illustrative design notes, not a current, complete SDK
implementation tutorial. Use the matching sibling Core
[`plugin-sdk`](https://github.com/drasi-project/drasi-core/tree/workgraph-generic-recovery/components/plugin-sdk)
and [`host-sdk`](https://github.com/drasi-project/drasi-core/tree/workgraph-generic-recovery/components/host-sdk)
source for exact interfaces.

## Overview

The current host uses two kinds of registration, not two Cargo build modes:

| Registration | How it is used |
|--------------|----------------|
| Small static core | `register_core_plugins` registers no-op/application bootstrappers and the application reaction |
| Runtime plugins | The host SDK loads `.so`/`.dylib`/`.dll` libraries from the configured plugin directory |

Generic plugin crates live in sibling `drasi-core`; their `dynamic-plugin`
feature is a plugin-crate build setting, not a Server feature. WorkGraph-specific
plugins belong to `drasi-workgraph`; see its [host setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/host.md).
The former `builtin-plugins` and `dynamic-plugins` Server flags no longer exist.

## Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                      drasi-server (host binary)                  │
│                                                                  │
│  API routes, config persistence, OpenAPI spec, server lifecycle  │
│  Uses DrasiLib for query processing                              │
│                                                                  │
│  Registration:                                                   │
│  • Small application/no-op core registered statically            │
│  • drasi-host-sdk loads connector cdylib plugins at runtime       │
├──────────────────────────────────────────────────────────────────┤
│                      drasi-host-sdk (library crate)              │
│                                                                  │
│  ┌──────────────────┐  ┌──────────────────────────────────────┐  │
│  │  PluginLoader    │  │  Proxy wrappers (impl DrasiLib traits)│  │
│  │                  │  │                                      │  │
│  │  load .so/.dll   │  │  SourceProxy       (wraps vtable)    │  │
│  │  validate meta   │  │  ReactionProxy     (wraps vtable)    │  │
│  │  call init()     │  │  SourcePluginProxy (wraps factory)   │  │
│  └──────────────────┘  └──────────────────────────────────────┘  │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  Callbacks: host_log_callback, host_lifecycle_callback   │    │
│  │  State store vtable construction (host → plugin)         │    │
│  │  Schema merging: plugin JSON → utoipa OpenAPI schemas     │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
                ↕ stable C ABI (#[repr(C)] vtables)
                ↕ opaque pointers for rich Rust types
┌──────────────────┐ ┌──────────────────┐ ┌──────────────────┐
│ libdrasi_source  │ │ libdrasi_react   │ │ libdrasi_boot    │
│ _mock.so (cdylib)│ │ _log.so (cdylib) │ │ _pg.so (cdylib)  │
│                  │ │                  │ │                  │
│ Own tokio runtime│ │ Own tokio runtime│ │ Own tokio runtime│
│ Own deps (serde, │ │ Own deps (serde, │ │ Own deps (serde, │
│  tracing, etc.)  │ │  tracing, etc.)  │ │  tracing, etc.)  │
│                  │ │                  │ │                  │
│ drasi_plugin_    │ │ drasi_plugin_    │ │ drasi_plugin_    │
│ init() → Reg     │ │ init() → Reg     │ │ init() → Reg     │
└──────────────────┘ └──────────────────┘ └──────────────────┘
```

## Crate Responsibilities

| Crate | Location | Role |
|-------|----------|------|
| `drasi-plugin-sdk` | `drasi-core/components/plugin-sdk` | Plugin-side SDK: FFI types, vtables, `export_plugin!` macro, vtable generation, FfiLogger, FfiStateStoreProxy |
| `drasi-host-sdk` | `drasi-core/components/host-sdk` | Host-side SDK: `PluginLoader`, proxy types (impl Source/Reaction/SourcePlugin), callback wiring, schema merging |
| `drasi-server` | `drasi-server/` | Application: REST API, config persistence, OpenAPI spec, server lifecycle — uses `drasi-host-sdk` for dynamic loading |
| `drasi-lib` | `drasi-core/lib/` | Core processing: query engine, routers, channels — no FFI awareness |
| `drasi-core` | `drasi-core/core/` | Core types: Element, SourceChange, QueryResult — crosses FFI as opaque pointers |

## Plugin Types

The original design below focuses on sources, reactions, and bootstrappers.
The current [Server loader](../src/dynamic_loading.rs) also registers secret-store
and identity-provider plugins; their exact interfaces live in the sibling SDK.

### Source Plugins
Ingest data from external systems (PostgreSQL, HTTP, gRPC, etc.) and emit `SourceChange` events.

**Trait**: `drasi_lib::sources::Source`
**Descriptor**: `drasi_plugin_sdk::descriptor::SourcePluginDescriptor`

### Reaction Plugins
Consume query results and take actions (webhooks, SSE, logging, etc.).

**Trait**: `drasi_lib::reactions::Reaction`
**Descriptor**: `drasi_plugin_sdk::descriptor::ReactionPluginDescriptor`

### Bootstrap Plugins
Provide initial data snapshots to populate queries when sources are connected.

**Trait**: `drasi_lib::bootstrap::BootstrapProvider`
**Descriptor**: `drasi_plugin_sdk::descriptor::BootstrapPluginDescriptor`

## How Dynamic Plugin Loading Works

### Loading Sequence

```
1. Server reads config and applies its startup plugin verification policy
   ↓
2. PluginLoader scans plugin directory for matching .so/.dylib/.dll files
   ↓
3. For each plugin file:
   a. dlopen() the shared library
   b. Resolve drasi_plugin_metadata() → PluginMetadata
   c. Validate SDK version (major.minor must match host)
   d. Validate target triple (must match host)
   e. Resolve drasi_plugin_init() → FfiPluginRegistration
   f. Call init → plugin initializes its tokio runtime, installs FfiLogger
   g. Wire log callback (plugin → host logging)
   h. Wire lifecycle callback (plugin → host events)
   i. Extract descriptor vtables into proxy types
   ↓
4. Register proxy types into PluginRegistry
   ↓
5. Host uses descriptors to create instances on demand:
   descriptor.create_source(id, config_json, auto_start) → SourceProxy
```

The default plugin directory is `plugins/` beside the executable; override it
with `--plugins-dir`. Signature verification defaults to on and can exclude
unsigned local libraries before loading. See [local plugin setup](setup.md#6-add-generic-plugins-for-examples)
for the narrowly scoped development exception and compatibility requirements.

### Plugin Entry Points

The original design centers on two primary entry points, sketched here.
Current SDKs may expose additional entry points:

```rust
// Returns version/compatibility metadata (safe to call with any ABI)
#[no_mangle]
pub extern "C" fn drasi_plugin_metadata() -> *const PluginMetadata

// Initializes the plugin and returns descriptor factories
#[no_mangle]
pub extern "C" fn drasi_plugin_init() -> *mut FfiPluginRegistration
```

### Version Validation (Design Summary)

These checks describe the original compatibility design. The current host
reports SDK, Core, library, and target versions through
[`PluginOperations::host_version_info`](../src/plugin_operations.rs) and delegates
loading checks to Core's host SDK. Version labels alone do not prove that two
locally modified builds have compatible layouts; rebuild matching binaries.

| Field | Check | Severity | Rationale |
|-------|-------|----------|-----------|
| `sdk_version` | Major.minor match | **REJECT** | `#[repr(C)]` layout changes are breaking |
| `target_triple` | Exact match | **REJECT** | Cannot load x86_64 `.so` on aarch64 |
| `plugin_version` | Log only | **INFO** | Plugin's own version — no compatibility constraint |

## FFI Boundary Design

### Opaque Pointer Pattern

Rich Rust types cross the FFI boundary as opaque `*mut c_void` pointers inside
`#[repr(C)]` envelope structs. Both the plugin and host statically link `drasi-core`,
so the memory layout of types like `SourceChange`, `Element`, and `QueryResult` is
identical (validated via version metadata).

```rust
// Plugin creates a SourceChange (rich Rust type)
let change = SourceChange::new(/* ... */);

// SDK wraps it in an FFI envelope
#[repr(C)]
struct FfiSourceEvent {
    opaque: *mut c_void,       // Box<SourceChange>
    source_id: FfiStr,         // borrows from opaque
    op: FfiChangeOp,           // Insert/Update/Delete
    drop_fn: extern "C" fn(*mut c_void),
}

// Host reads the opaque pointer as &SourceChange (zero-copy)
let source_change = unsafe { &*(ffi_event.opaque as *const SourceChange) };
```

### Vtable Pattern

Component interactions use `#[repr(C)]` vtable structs (tables of function pointers):

```rust
#[repr(C)]
pub struct SourceVtable {
    pub state: *mut c_void,           // Plugin's concrete state
    pub id_fn: extern "C" fn(*const c_void) -> FfiStr,
    pub start_fn: extern "C" fn(*mut c_void) -> FfiResult,
    pub stop_fn: extern "C" fn(*mut c_void) -> FfiResult,
    pub status_fn: extern "C" fn(*const c_void) -> FfiComponentStatus,
    pub subscribe_fn: extern "C" fn(...) -> *mut FfiSubscriptionResponse,
    pub drop_fn: extern "C" fn(*mut c_void),
    // ... more methods
}
```

The host wraps each vtable in a proxy type that implements the real DrasiLib trait:

```rust
// Host-side proxy (in drasi-host-sdk)
impl Source for SourceProxy {
    async fn start(&self) -> Result<()> {
        let result = (self.vtable.start_fn)(self.vtable.state);
        result.into_result()
    }
    // ...
}
```

### Reverse Vtables (Host → Plugin)

Some services flow from host to plugin:

- **StateStoreProvider**: Host owns the state store, plugins access it via `StateStoreVtable`
- **BootstrapProvider**: Bootstrap plugin A → host → source plugin B (mediated via `BootstrapProviderVtable`)

The host builds a vtable from its own trait implementation and passes it to the plugin,
which wraps it in a local proxy (`FfiStateStoreProxy`, `FfiBootstrapProviderProxy`).

## Runtime Model

### Multiple Tokio Runtimes

Each cdylib plugin runs its own tokio runtime, initialized during `drasi_plugin_init()`.
The host also has its own runtime. This means:

- No tokio version coupling between host and plugins
- No shared thread pools
- Plugins can use any tokio features independently
- ~20µs FFI overhead per async call (negligible)

### Async → Sync Bridge

DrasiLib traits have async methods, but FFI vtable functions must be `extern "C"` (sync).
The SDK bridges this with `std::thread::spawn` + `block_on`:

```
Host calls vtable.start_fn(state)
  → Plugin spawns OS thread
  → OS thread calls runtime.block_on(source.start())
  → Async work completes on plugin's tokio runtime
  → OS thread returns FfiResult
  → Host receives result
```

This avoids nesting tokio runtimes (which would panic) by running `block_on` on a fresh
OS thread.

## Logging and Tracing

### Plugin → Host Log Bridge

Plugins use standard `log::info!()` / `log::error!()` macros. The `export_plugin!` macro
installs an `FfiLogger` that forwards all log records to the host via a callback:

```
Plugin: log::info!("connected")
  → FfiLogger.log(record)
  → Serialize to FfiLogEntry { level, plugin_id, message }
  → Call host_log_callback(entry_ptr)
  → Host: log::log!(level, "[plugin:{}] {}", id, message)
```

The `tracing` crate's `log` feature causes tracing events to fall back to `log` records
when no tracing subscriber is set in the plugin's cdylib, so both logging frameworks work.

## Plugin Development Sketches (Historical)

These partial examples retain the original trait/factory/export design. They
omit fields and signatures needed by current SDK versions and are not
copy-and-build setup instructions.

### Creating a New Source Plugin

1. **Create crate** with `crate-type = ["lib", "cdylib"]`:

```toml
# Cargo.toml
[package]
name = "drasi-source-mydb"

[lib]
crate-type = ["lib", "cdylib"]

[features]
dynamic-plugin = []

[dependencies]
drasi-lib = { workspace = true }
drasi-plugin-sdk = { workspace = true }
# ... your deps
```

2. **Implement the Source trait** (same code for static and dynamic):

```rust
use drasi_lib::sources::Source;

pub struct MyDbSource { /* ... */ }

#[async_trait]
impl Source for MyDbSource {
    fn id(&self) -> &str { &self.id }
    fn type_name(&self) -> &str { "mydb" }
    async fn start(&self) -> anyhow::Result<()> { /* connect */ }
    async fn stop(&self) -> anyhow::Result<()> { /* disconnect */ }
    async fn status(&self) -> ComponentStatus { /* ... */ }
    async fn subscribe(&self, settings: SourceSubscriptionSettings)
        -> anyhow::Result<SubscriptionResponse> { /* ... */ }
    // ...
}
```

3. **Implement the descriptor** (factory + config schema):

```rust
use drasi_plugin_sdk::descriptor::SourcePluginDescriptor;

pub struct MyDbSourceDescriptor;

#[async_trait]
impl SourcePluginDescriptor for MyDbSourceDescriptor {
    fn kind(&self) -> &str { "mydb" }
    fn config_version(&self) -> &str { "1.0.0" }
    fn config_schema_name(&self) -> &str { "MyDbSourceConfig" }
    fn config_schema_json(&self) -> String {
        // Return utoipa OpenAPI schema as JSON
    }
    async fn create_source(&self, id: &str, config: &Value, auto_start: bool)
        -> anyhow::Result<Box<dyn Source>> {
        let config: MyDbConfig = serde_json::from_value(config.clone())?;
        Ok(Box::new(MyDbSource::new(id, config, auto_start)))
    }
}
```

4. **Register with `export_plugin!`**:

```rust
#[cfg(feature = "dynamic-plugin")]
drasi_plugin_sdk::export_plugin!(
    plugin_id = "mydb-source",
    core_version = env!("CARGO_PKG_VERSION"),
    lib_version = env!("CARGO_PKG_VERSION"),
    plugin_version = env!("CARGO_PKG_VERSION"),
    source_descriptors = [MyDbSourceDescriptor],
    reaction_descriptors = [],
    bootstrap_descriptors = [],
);
```

5. **Build in the owning plugin workspace**, using its current SDK and
   `dynamic-plugin` support. See [generic plugin build commands](setup.md#6-add-generic-plugins-for-examples)
   rather than the removed Server static/dynamic build switches.

### Creating a Reaction Plugin

Same pattern as source, but implement `Reaction` trait and `ReactionPluginDescriptor`:

```rust
use drasi_lib::reactions::Reaction;
use drasi_plugin_sdk::descriptor::ReactionPluginDescriptor;

pub struct MyReaction { /* ... */ }

#[async_trait]
impl Reaction for MyReaction {
    fn id(&self) -> &str { &self.id }
    fn type_name(&self) -> &str { "myreaction" }
    fn query_ids(&self) -> Vec<String> { self.query_ids.clone() }
    async fn start(&self) -> anyhow::Result<()> { /* ... */ }
    async fn stop(&self) -> anyhow::Result<()> { /* ... */ }
    // ...
}

pub struct MyReactionDescriptor;

#[async_trait]
impl ReactionPluginDescriptor for MyReactionDescriptor {
    fn kind(&self) -> &str { "myreaction" }
    async fn create_reaction(&self, id: &str, query_ids: Vec<String>,
        config: &Value, auto_start: bool) -> anyhow::Result<Box<dyn Reaction>> {
        // ...
    }
}
```

## OpenAPI Schema Flow

Plugin DTOs serve double duty: serde deserialization AND OpenAPI schema generation.
The schema crosses the FFI boundary as a JSON string.

```
Plugin                              Host
──────                              ────
#[derive(utoipa::ToSchema)]
struct MyConfig { ... }
        ↓
config_schema_json() → JSON string  →  Parse JSON
                                        ↓
                                    Merge into global OpenAPI spec
                                        ↓
                                    Build OneOf union of all plugin configs
                                        ↓
                                    /api/v1/openapi.json
```

## Build System

The [setup guide](setup.md#3-build-the-server-and-optionally-the-ui) is the
canonical build path. Build the host with Cargo (and the UI first if wanted);
use `make build-local-plugins` or `make build-local-plugins-debug` in Server to
delegate generic plugin builds to sibling Core. Server's `cargo xtask` manages
vendored native libraries, not plugin discovery/builds.

**Historical build rationale:** Plugins were built individually to avoid
feature-unification issues where adaptive plugins inherited cdylib entry points
from base dependencies. Keep that rationale when working on the Core plugin
builder; the obsolete `make build-dynamic*` commands are not retained here as
runnable instructions.

### Testing

```sh
# Server library tests
cargo test --lib

# Smoke tests build and start a server; not a read-only documentation check
make test-smoke
```

For SDK/FFI integration testing, use Core's
`components/host-sdk/tests/integration_test.rs` with its matching prebuilt
plugins. See [tests/README.md](../tests/README.md) for the Server suites.

## FAQ

### Why cdylib instead of dylib?

The original `dylib` approach required:
- Identical Rust compiler version (symbol hashes must match)
- Shared runtime loaded with `RTLD_GLOBAL`
- Single-invocation build (to prevent symbol hash mismatches)
- `libstd-*.so` copying alongside plugins

The `cdylib` approach reduces Rust symbol and shared-runtime coupling through a
C ABI boundary. It does not make arbitrary host/plugin builds compatible:
opaque Rust values still need matching layouts. Follow the current setup
guide's same-checkout/toolchain policy for local development.

### Can plugins use different tokio versions?

Yes. Each cdylib plugin statically links its own tokio. The host and plugins can
use different tokio versions as long as the DrasiLib trait types (which cross as
opaque pointers) remain layout-compatible (validated via version metadata).

### What happens if a plugin panics?

All FFI entry points are wrapped in `std::panic::catch_unwind` by the `export_plugin!`
macro. A panic in a plugin is caught and converted to an `FfiResult::Err` — the host
receives an error message instead of undefined behavior.

### Can I debug plugins?

Yes. Since cdylib plugins are standard shared libraries, you can:
- Use `RUST_LOG=debug` for log output
- Attach gdb/lldb to the host process (plugin code is in the .so)
- Use `nm -D plugin.so` to verify exported symbols
- Use `ldd plugin.so` to check dependencies

### How do I test a plugin in isolation?

Plugins implement the same DrasiLib traits for both static and dynamic builds.
Write unit tests against the trait implementation directly (no FFI needed):

```rust
#[tokio::test]
async fn test_my_source() {
    let source = MyDbSource::new("test", config);
    source.start().await.unwrap();
    assert_eq!(source.status().await, ComponentStatus::Running);
}
```

For integration testing of the FFI layer, see the host-sdk integration tests at
`drasi-core/components/host-sdk/tests/integration_test.rs`.
