# Drasi Plugin Architecture

This document describes the plugin system architecture for Drasi Server, covering both
the static (builtin) and dynamic (cdylib) plugin loading approaches.

## Overview

Drasi Server always registers a small set of core descriptors and loads other
plugins dynamically. It has no `builtin-plugins` / `dynamic-plugins` engine or
server build selector. Two independent dynamic ABI families share one host:

| Family | ABI | Entry points |
|--------|-----|--------------|
| Source / Reaction / Bootstrap (and existing provider descriptors) | `0.17`, also accepts `0.16` fast-mode plugins | `drasi_plugin_metadata`, `drasi_plugin_init` |
| Native ComputationGraph factories | `1.0` | `drasi_computation_plugin_metadata`, `drasi_computation_plugin_entry` |

Each producer enables its own `dynamic-plugin` feature. Legacy `export_plugin!`
and native `export_computation_plugin!` are separate contracts; native plugins do
not implement Source/Reaction just to cross the boundary. Both run on the same
ComputationGraph runtime, with no fallback engine or runtime selector.

## Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                      drasi-server (host binary)                  │
│                                                                  │
│  API routes, config persistence, OpenAPI spec, server lifecycle  │
│  Uses DrasiLib for query processing                              │
│                                                                  │
│  Plugin families:                                                │
│  • Legacy Source/Reaction/Bootstrap ABI 0.16 / 0.17                │
│  • Independent native ComputationGraph ABI 1.0                    │
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
| `drasi-computation-plugin-abi` | `drasi-core/components/computation-plugin-abi` | Independent native ABI 1.0 C layouts and version headers |
| `drasi-computation-plugin-sdk` | `drasi-core/components/computation-plugin-sdk` | Native factory metadata, wire serialization, producer-owned handles and export macro |
| `drasi-host-sdk` | `drasi-core/components/host-sdk` | Host-side SDK: `PluginLoader`, proxy types (impl Source/Reaction/SourcePlugin), callback wiring, schema merging |
| `drasi-server` | `drasi-server/` | Application: REST API, config persistence, OpenAPI spec, server lifecycle — uses `drasi-host-sdk` for dynamic loading |
| `drasi-lib` | `drasi-core/lib/` | Core processing: query engine, routers, channels — no FFI awareness |
| `drasi-core` | `drasi-core/core/` | Core types: Element, SourceChange, QueryResult — crosses FFI as opaque pointers |

## Plugin Types

Ordinary components retain their existing descriptor contracts:

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

### Native ComputationGraph Plugins

Native factories declare typed ports, schemas, roles, configuration versions and
explicit capabilities. Host `NativeFactory` implements Core's `ComponentFactory`;
transactional participants also register with the instance's transaction factory
registry. Each instance's optional `computation` configuration supplies
factory-only component declarations and explicit resource construction recipes
for its ComputationGraph.

The `sourceProgress` resource recipe uses `{"kind":"sourceProgress","component":"query"}`
and the `checkpoint` resource role. Bind the same resource ID to the named
consumer and its replayable source under their `source_progress` dependencies.
Only the actual consumer publishes recovered progress; configuration cannot
restore checkpoints or assert readiness/durability. Each instance constructs
its own owner, and the graph verifies actual shared ownership rather than
matching names. Native sources must independently negotiate recovery-v1.
This recipe does not turn a volatile source or an old ABI 1.0 plugin into a
replayable source, and it does not provide a database bootstrap implementation.

Native bootstrap-v1 factories use a graph-owned `Bootstrap` resource with a
`nativeBootstrap` recipe. The fields are `component` (the owning query),
`implementation`, `configurationVersion`, `configuration` (unresolved
`ConfigurationValue` entries) and optional `sourceProgress`. The latter is
required exactly when negotiated by the factory and must name the same actual
checkpoint owner as the query's `source_progress` dependency. The bootstrap
resource declares that `Checkpoint` dependency and every referenced `SecretStore`
in `resource_dependencies`. The query binds it in its `bootstrap` slot.
Providers cannot be shared across queries or instances. Cleanup joins work and
revokes retired progress readers before releasing prerequisite resources.

Native consumer-v1 factories bind a `consumer` dependency to a graph-owned
`IndexBackend` resource with this recipe:

```yaml
kind: nativeConsumer
failureScope: processRestart
maxStreams: 16
receiptsPerStream: 64
```

Declare exactly one `IndexBackend` dependency for this resource, normally a
`rocksdbIndexes` recipe. It uses that actual provider, not a second open inferred
from a path, and isolates progress by instance, graph and consumer. Optional
`retry` uses the shared delivery retry policy. The plugin's negotiated external
or transactional mode is validated against persisted progress; no mode is
inferred from configuration. A producer's shared storage group is not a
consumer transaction. External effects still require destination idempotence;
transactional completion covers one operation, not an entire source transaction.

Both recipes work through managed reconstruction and imperative component
configuration. Rust callers needing all native services should use
`build_components_with_registry` (or `register_components`); the older
factory-only `build_components` cannot discover bootstrap factories.
Configuration snapshots retain secret references, never their resolved values.
Current qualification uses separately built test-only libraries, real RocksDB
and persisted desired definitions; production plugin migration is deferred.

QoS resources can opt into bounded admission or output replay using `recovery`:

```yaml
recovery:
  kind: admission
  component: source
  failureScope: processRestart
  maxProducers: 16
  receiptsPerProducer: 64
```

Alternatively use `kind: replay`, `failureScope: processRestart` and
`receiptCapacity: 64` for a persistent producer's outgoing journal. These are
mutually exclusive. Both require `definition.durable: true`, backpressure
retention, a storage `path`, and actual provider survival evidence. Admission
bounds are at most 1024 producers, 1024 receipts per producer and 16384 receipts
in total; replay allows at most 1024 receipts. Instance/graph identity is supplied
by the owner, not by these settings. The component must actually support the
corresponding service; configuration alone does not upgrade its contract.
Omitting recovery keeps a new channel untracked, but cannot silently disable a
previously tracked journal. Reopen must match persisted settings; arbitrary
changes require a separate migration. Enabling tracking on an existing channel
is supported only before its first accepted event.

Shared producer/output transactions use two graph-owned resources:
`{"kind":"sharedStorage","path":"./data/processing","component":"query"}` as
`IndexBackend`, and a `sharedQos` recipe containing `definition` and replay
`recovery` as `StateStore`. The journal declares exactly one `IndexBackend`
in the topology's `resource_dependencies`; the graph supplies that actual
group, rather than independently opening the same path. The producer's indexes
and its outgoing pipe's `shared_storage` both name the group resource.

Dependency-bearing imperative batches defer resource acquisition to the same
graph lifecycle used by managed definitions. Construction follows dependencies;
cleanup releases dependents before providers, retaining parents on failed or
cancelled cleanup. Empty dependency maps leave ordinary construction unchanged.
This supports shared ownership and reconstruction, not arbitrary storage
migration or permission to forget pending messages. Source recovery and external
effect completion still need their own supported contracts.

Managed changes protect persistent recovery domains before accepting a new
definition. A refused path, membership, producer, downgrade or removal change
returns HTTP 409 `CONFIGURATION_TRANSITION_REQUIRED` without a new receipt or
revision. Shared-storage domains support verified drain after successful instance
stop: the graph freezes restart and holds actual transaction gates through the
producer/journal/timer checks and authoritative configuration commit. Confirmed
rejection resumes old owners; acceptance fences and reconstructs them. Unknown
commit confirmation keeps them frozen until reconciliation resolves it.
Standalone persistent QoS journals also hold their actual transaction owner
through acceptance, after proving every nonretired subscriber reached the accepted
head. Proof takes journal state before its transaction gate; queued writers cannot
change the inspected state. Terminal shutdown revokes the lease without reopening
the old owner, including when commit confirmation is unavailable.
Initialized built-in continuous queries and native linear transaction sequences
can also retire a standalone provider: the output contract weakly identifies the actual provider and transaction,
then holds that transaction while checking output and scheduled work. A matching
path or an unused provider is not evidence. Quiesce the graph before stopping it
for lossless retirement; ordinary stop can interrupt an active handoff and will
then require recovery rather than authorizing a change.
Unrelated and lifecycle-only changes retain their ordinary path. Unsupported
providers or missing initialization remain refused even if apparently stopped.
Native consumers use positive completed-ledger evidence from their actual
successfully closed delivery owner, without changing stop's awaited cleanup.
Their lifecycle stays held until configuration resolution. Partial, uncertain or
uninitialized completion cannot prove lossless drain; a stopped consumer is not assumed empty.
Scoped consumer bindings identify their actual prerequisite provider.
Native bootstrap resources hold their own positively stopped call gate, including
against retained handles. Terminal cleanup revokes a held lease permanently.
Source-progress resources must match the actual processing owner's publication;
known query-catalog, configuration and factory-registry resources provide no
independent storage proof. Unknown concrete services remain refused.
Explicit loss-authorized **complete domain removal** is supported as described
below. In-place reset/reuse and automatic data migration are not provided.

`GET /api/v1/plugins/computation` exposes native manifests without instance secrets.
Its `bootstrapFactories` and `consumerFactories` arrays report independently
negotiated bootstrap metadata and consumer handling modes; the base ABI/wire
metadata in `plugins` remains unchanged.
Native runtime IDs are family/version-qualified: `computation:<id>@<version>`.
The same plugin may supply several factories; its own package version is not the
ABI version or a factory's configuration version.

### Durable desired configuration

Configuration persistence is optional and separate from message/state recovery.
Omitting `configurationStore` preserves the ordinary fast configuration and
existing YAML-backed APIs. Selecting it does not add a journal, acknowledgements
or exactly-once processing to an otherwise volatile pipeline.

```yaml
id: analytics
configurationStore:
  kind: redb
  path: ./data/configuration.redb
  keyFile: ./secrets/configuration.key
```

For multiple instances, put these settings on each `instances` entry. IDs must
be explicit and stable; different IDs may share one physical database and key.
The key file must already contain exactly 32 raw bytes, not a hexadecimal or
base64 string. Provision it securely, restrict its file permissions, and back it
up separately from the database. Missing, wrong or malformed keys fail startup;
there is no empty-state or memory fallback. Definitions and receipts are
encrypted; instance, request and snapshot names are not.

YAML's `computation` definition initializes an instance **only once**. Thereafter
the stored desired definition is authoritative, even if the YAML seed is changed,
omitted or semantically obsolete. YAML must still parse and supply valid Server
settings, a stable instance ID and the storage/key locations. An explicitly
accepted empty definition is initialized state too. Ordinary `sources`,
`queries` and `reactions` cannot be automatically adopted into this mode: use
reconstructible `computation` declarations.

The revisioned API is rooted at
`/api/v1/instances/{instanceId}/computation`:

| Method and suffix | Meaning |
|---|---|
| `GET /desired` | Privileged committed revision and desired definition; may contain literal secrets. |
| `PUT /desired` | JSON or YAML body with `expectedRevision`, a stable `requestId`, and `desired` (`version: 1`, `topology`, and optional `retirement`). Returns HTTP 202 and a receipt after durable acceptance, before construction or readiness. |
| `GET /receipts/{requestId}` | Resolve a lost response using the original request ID. A 404 is not proof of rejection while a request is still in flight. |
| `GET /management` | Secret-safe status: accepted, initializing, ready, failed or cleanupPending; no raw constructor/store errors or configuration. |
| `POST /reconcile` | Retry realization of the accepted definition; inspect the returned status rather than assuming success means ready. |

Use the revision and `data.desired` returned by `GET /desired` to prepare a new
request. Repeating the same request ID and definition returns the original receipt
without creating another owner; changing its content or using a stale revision
returns 409. Request IDs must be nonempty, at most 1024 bytes, and outside the
reserved `drasi-server/` prefix. On 503 or a lost connection, look up the receipt
and status before retrying. Unconfirmed state is not reported as authoritative.
Successful management responses use `Cache-Control: no-store`; protect the
privileged desired endpoint with the deployment's access controls.

To explicitly abandon pending obligations, first stop the instance and prepare a
topology that removes every component and resource in the affected recovery
domains. Include this field in `desired` (example IDs only):

```yaml
retirement:
  from_revision: 7
  allow_data_loss: true
  resources: [delivery, indexes, journal]
  components: [input, consumer]
```

`from_revision` must equal the current configuration revision and the request's
`expectedRevision`. Membership must exactly cover the changed domains, including
connected producers/consumers and resource dependencies; omitted live unmanaged
users, extra members, or retained/replacement members are refused. Malformed
authorization is HTTP 400; stale or incomplete permission and unqualified
transitions are HTTP 409 without acceptance. A durable configuration store is
required. Permission is part of the existing persisted definition and idempotency
receipt, not a separate best-effort action. Retry the identical request after an
uncertain response; carrying a previously accepted authorization forward cannot
authorize a new retirement.

Authorization bypasses pending journal/output/scheduled-work checks, not actual
ownership, successful stop/cleanup, initialized producer evidence or healthy
transaction gates. A consumer may retire partial or unknown progress only after
its actual delivery storage has successfully closed. A prior processing failure
may remain recorded on a stopped component; stop/removal failures still refuse.
Bootstrap resources still need their own positively stopped, held lifecycle.
Confirmed rejection resumes old owners; unknown confirmation keeps them frozen.
Accepted retirement removes the desired domain and fences its old owners.
**No stored input, output, checkpoint, ledger or business state is deleted,
reset, or marked handled.** External effects already performed are not undone.
Restart restores the accepted removed topology. Explicitly restoring the original
definition later may replay its intact pending work; retirement is not a storage
tombstone or an in-place reset.

Persistent managed instances reject imperative create/update/delete, clone-target
and solution-deployment mutations; use the desired-state API instead. Runtime
start/stop and the existing read-only gate retain their meanings. Unavailable
factories and failed processing resources remain accepted, inspectable
declarations rather than disappearing or becoming a replacement empty graph.
Factories available to management are captured when the instance is built:
install a missing plugin and restart Server to make its factories available.
Resource reconstruction reads the current provider registry on each attempt.

`persistConfig` still controls YAML updates, including recording newly provisioned
instances. It does not disable durable desired-state acceptance. YAML saves retain
managed instance/store settings but do not export accepted definitions or their
literal secrets into plaintext. The original seed file is not automatically
scrubbed; remove literal credentials from it after initialization if necessary.

This control-plane support does not qualify arbitrary durability downgrades,
in-place reuse or storage migration with pending processing work. Such changes
require verified lossless drain; explicit loss permission only covers complete
domain removal. Configuration durability alone is not evidence of safe processing
transitions.

## How Dynamic Plugin Loading Works

### Loading Sequence

```
1. Server verifies signatures and local integrity, producing an exact filename allowlist
   ↓
2. PluginLoader scans plugin directory for matching .so/.dylib/.dll files
   ↓
3. For each plugin file:
   a. Only an allowed candidate may reach dlopen()
   b. Explicit family dispatch: native symbols use ABI 1.0; invalid/partial native
      declarations fail without invoking a legacy initializer
   c. For legacy plugins, use the ABI 0.16 / 0.17 flow:
      Resolve drasi_plugin_metadata() → PluginMetadata
      Reject missing or null metadata before initialization
      Validate SDK version (major.minor must match host) and target triple
      Resolve drasi_plugin_init() → FfiPluginRegistration
      Call init → plugin initializes its tokio runtime, installs FfiLogger
      Wire log and lifecycle callbacks
      Extract descriptor vtables into proxy types
   ↓
4. Register proxy types into PluginRegistry
   ↓
5. Host uses descriptors to create instances on demand:
   descriptor.create_source(id, config_json, auto_start) → SourceProxy
```

### Plugin Entry Points

Legacy cdylib plugins export these symbols (additional callback symbols may exist):

```rust
// Returns version/compatibility metadata after the binary has been trusted
#[no_mangle]
pub extern "C" fn drasi_plugin_metadata() -> *const PluginMetadata

// Initializes the plugin and returns descriptor factories
#[no_mangle]
pub extern "C" fn drasi_plugin_init() -> *mut FfiPluginRegistration
```

### Version Validation

| Field | Check | Severity | Rationale |
|-------|-------|----------|-----------|
| `sdk_version` | Major.minor match | **REJECT** | `#[repr(C)]` layout changes are breaking |
| `target_triple` | Exact match | **REJECT** | Cannot load x86_64 `.so` on aarch64 |
| `plugin_version` | Log only | **INFO** | Plugin's own version — no compatibility constraint |

The Server API (`GET /api/v1/plugins`) reports three different versions:

| JSON field | Meaning |
|------------|---------|
| `pluginVersion` | The plugin's package version, read from the loaded library's metadata |
| `kinds[].configVersion` | The configuration format for a plugin kind, from `config_version()` |
| `sdkVersion` | The plugin/host interface compatibility version, not the Cargo package version of `drasi-plugin-sdk` |

For native plugins, `sdkVersion` records the independent native ABI (`1.0.0`).
The native manifest also exposes its wire version. Current native plugins use
wire version 2 (binary computation envelopes and bulk MessagePack buffers).
Rebuild native plugins from the matching SDK; the unreleased wire-version-1
prototype is rejected. Legacy ABI 0.17 adds versioned storage durability evidence
and releases transferred state-store ownership. Existing ABI 0.16 plugins remain
supported for fast-mode behavior, retaining their original state-store pointer
lifetime and without new recovery services. ABI 0.15, unknown future contracts,
malformed/missing metadata and wrong targets are rejected before initialization.
Provider key-list errors remain explicit, and bootstrap subscription settings and
context properties are forwarded. Existing data-envelope formats are unchanged.
Loading compatibility and storage durability do not establish end-to-end recovery.
Header/ABI/wire compatibility
is checked independently of the legacy SDK; absent legacy-only version fields
remain empty, never synthesized.
Local auto-install resolutions retain declared `abi_family` and `abi_version`
from embedded metadata, irrespective of the binary's filename. The current
`plugins.lock` format does not carry these declarations, so lockfile-only
resolutions leave them unknown; family validation still occurs when loading.

Runtime load, install, and filesystem watching share verification and family
dispatch. Verification-enabled loading fails closed when a candidate is not
verifiable. Runtime installation refuses to overwrite an existing plugin binary;
replacing loaded code requires a server restart. Both ABI families keep their
libraries pinned; there is no hot unloading.

Startup and runtime loading both retain the package version. Source and reaction
metadata also use it for `pluginVersion`. If the package version is unavailable,
the plugin listing uses an empty string and component metadata omits
`pluginVersion`; neither substitutes the configuration-format version.

For matching local Server and plugin builds on the ComputationGraph branch, see
[ComputationGraph development](../README.md#computationgraph-development).

## FFI Boundary Design

The opaque Rust object examples below describe the **legacy ABI only**. Native
ComputationGraph ABI 1.0 uses serialized wire data and opaque producer-owned
handles. It does not exchange Rust trait objects, futures, allocators, or native
`SourceChange` memory layouts between independently built libraries.

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

## Plugin Development Guide

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

5. **Build**:

```sh
# Static (linked into server binary):
cargo build  # with drasi-server's builtin-plugins feature

# Dynamic (standalone .so):
cargo build --lib -p drasi-source-mydb --features drasi-source-mydb/dynamic-plugin
```

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

### Static Build (default)

```sh
cargo build                    # debug
cargo build --release          # release
```

All plugins are statically linked. No `.so` files needed.

### Dynamic Build

```sh
make build-dynamic             # build server + all plugins (debug)
make build-dynamic-release     # build server + all plugins (release)
make build-dynamic-server      # server only
make build-dynamic-plugins     # plugins only
```

Plugins are built individually (not in batch) to avoid feature unification issues
where adaptive plugins inherit cdylib entry points from their base dependencies.

### Testing

```sh
# Static tests
cargo test --lib                                                    # 196 tests
cargo test --test error_resilience_test                             # 11 tests

# Dynamic tests (requires: make build-dynamic-plugins)
cargo test --no-default-features --features dynamic-plugins --lib   # 185 tests

# Host-SDK integration tests (requires pre-built cdylib plugins)
cd ../drasi-core && cargo test -p drasi-host-sdk --test integration_test  # 22 tests

# Smoke tests (builds and runs server with all plugins)
make test-smoke
```

## FAQ

### Why cdylib instead of dylib?

The original `dylib` approach required:
- Identical Rust compiler version (symbol hashes must match)
- Shared runtime loaded with `RTLD_GLOBAL`
- Single-invocation build (to prevent symbol hash mismatches)
- `libstd-*.so` copying alongside plugins

The `cdylib` approach eliminates all of these constraints. Each plugin is fully
self-contained with a stable C ABI boundary.

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
