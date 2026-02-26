# Dynamic Plugin Loading — Dylib Sharing Plan

## Background

After extensive investigation across 10+ sessions, we proved that **Rust `dylib` crate-type** (not `cdylib`) is the right approach for dynamic plugin loading. Unlike `cdylib` which statically links all dependencies and creates isolated compilation units, `dylib` participates in Rust's dynamic linking graph — plugins and host share a single copy of tokio, tracing, and all core types in memory.

### What Failed (cdylib + C FFI)
- `cdylib` statically links ALL Rust dependencies into each .so — each plugin gets its own tokio with unique symbol hashes
- Rust's symbol mangling includes a disambiguator hash based on the instantiating crate's `StableCrateId`
- Result: ZERO of 608 undefined plugin symbols matched any of 46,084 runtime symbols
- Required JSON serialization for all data crossing the boundary, sync-only plugin code, and host-side FFI wrappers

### What Works (dylib)
The PoC at `/home/danielgerlag/dev/drasi/dylib-poc/` proved all of the following:

1. **Plugin's `tokio::spawn()` runs on the host's tokio runtime** — shared thread-locals
2. **`tokio::select!`, `tokio::time::interval`, channels** all work natively in plugins
3. **`SourceChange` (`Arc<str>`, `BTreeMap`, `ElementValue`)** crosses the boundary with zero serialization
4. **`Arc<QueryResult>` (`serde_json::Value`, `chrono`, `HashMap`)** crosses the boundary with zero serialization
5. **`Box<dyn Source>` and `Box<dyn Reaction>` trait objects** work directly across the boundary
6. **`tokio::sync::mpsc` channels** shared between host and plugin
7. **`tracing::info!()` from plugin** is captured by the host's tracing subscriber
8. **`log::info!()` from plugin** is bridged via `tracing-log` and captured
9. **Tracing spans** with component metadata (`component_type`, `component_id`) work across the boundary
10. **Plugin code is IDENTICAL to current static plugin code** — no rewrites needed

## Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                     drasi-server (host binary)            │
│                                                          │
│  ┌───────────────┐   ┌────────────────────────────────┐  │
│  │ Tracing       │   │ Plugin Registry                │  │
│  │ Subscriber    │   │                                │  │
│  │ (fmt + log    │   │ kind → Box<dyn SourceDesc>     │  │
│  │  bridge)      │   │ kind → Box<dyn ReactionDesc>   │  │
│  └───────────────┘   │ kind → Box<dyn BootstrapDesc>  │  │
│                      └───────────────┬────────────────┘  │
│  Built with -C prefer-dynamic        │                   │
│  Loads runtime .so with RTLD_GLOBAL   │                   │
│  then loads plugin .so files          │                   │
└──────────────────────────────────────┼───────────────────┘
                                       │
              ┌────────────────────────┼───────────────────┐
              │                        │                   │
              ▼                        ▼                   ▼
 ┌──────────────────┐  ┌─────────────────────┐  ┌────────────────┐
 │ libdrasi_plugin   │  │ libdrasi_source     │  │ libdrasi_react │
 │ _runtime.so       │  │ _postgres.so        │  │ _log.so        │
 │                   │  │                     │  │                │
 │ crate-type: dylib │  │ crate-type: dylib   │  │ crate-type:    │
 │                   │  │                     │  │   dylib        │
 │ Contains:         │  │ Links to: runtime   │  │                │
 │  tokio (full)     │  │                     │  │ Links to:      │
 │  tracing          │  │ Exports:            │  │   runtime      │
 │  serde/serde_json │  │  plugin_init() →    │  │                │
 │  chrono           │  │  PluginRegistration │  │ Exports:       │
 │  drasi-lib types  │  │  with Box<dyn Src>  │  │  plugin_init() │
 │  log              │  │                     │  │                │
 │  ordered-float    │  │ Uses tokio::spawn,  │  │ Uses tokio,    │
 │  async-trait      │  │ channels, tracing   │  │ channels,      │
 │                   │  │ DIRECTLY            │  │ tracing        │
 │ Loaded RTLD_GLOBAL│  │                     │  │ DIRECTLY       │
 └──────────────────┘  └─────────────────────┘  └────────────────┘
        ▲                        │                       │
        │      Dynamic link      │                       │
        └────────────────────────┴───────────────────────┘
```

**Key insight**: All `.so` files dynamically link to `libdrasi_plugin_runtime.so`, sharing a single copy of tokio, tracing, and all core types in memory. No FFI boundary, no serialization, no wrappers needed.

## How It Works

### Shared Runtime (RTLD_GLOBAL)
The shared runtime dylib contains all heavy dependencies (tokio, tracing, serde, drasi-lib types). It is loaded with `RTLD_GLOBAL` so its symbols are available to all subsequently loaded plugin dylibs.

### Plugin Entry Point
Each plugin dylib exports a single `extern "C"` function:

```rust
#[no_mangle]
pub extern "C" fn plugin_init() -> *mut PluginRegistration {
    let registration = PluginRegistration {
        sources: vec![Box::new(MySourceDescriptor::new())],
        reactions: vec![],
        bootstrappers: vec![],
    };
    Box::into_raw(Box::new(registration))
}
```

The `PluginRegistration` struct contains `Box<dyn SourceDescriptor>`, `Box<dyn ReactionDescriptor>`, etc. — exactly the same trait objects the static build uses today. The host calls `Box::from_raw()` to take ownership and registers them in the `PluginRegistry`.

### Data Flow (Zero-Copy)
```
Source plugin                    Host                     Reaction plugin
     │                            │                            │
     │ tokio::spawn (on host RT)  │                            │
     │──────────────────────────► │                            │
     │                            │                            │
     │ SourceChange via mpsc      │                            │
     │ (Arc<str>, BTreeMap, etc.) │                            │
     │──────────────────────────► │                            │
     │                            │ Arc<QueryResult> via mpsc  │
     │                            │──────────────────────────► │
     │                            │                            │
     │ tracing::info!()           │  Captured by host's        │
     │──────────────────────────► │  tracing subscriber        │
     │                            │                            │
     │ log::debug!()              │  Bridged via tracing-log   │
     │──────────────────────────► │  then captured             │
```

All Rust types cross the dylib boundary natively. No serialization. No FFI wrappers.

### Tracing & Logging
The host initializes the tracing subscriber once. Because the plugin shares the same `tracing` crate binary:
- `tracing::info!()` in plugins → captured by host's subscriber
- `log::info!()` in plugins → bridged via `tracing-log` → captured
- `tracing::info_span!()` in plugins → span context flows through normally
- Component metadata (id, type) in spans → visible in structured logs

## Constraints

### Same Rust Compiler Version
Host and all plugins MUST be built with the same Rust toolchain version. This ensures:
- Compatible symbol hashes
- Identical type layouts and vtable layouts
- Same ABI for `Box<dyn Trait>`, `Arc<T>`, `Vec<T>`, etc.

**Recommended**: Build all plugins in the same Cargo workspace and same `cargo build` invocation.

### Pinned Dependency Versions in Runtime Crate

The shared runtime crate pins exact versions of all re-exported dependencies:

```toml
# drasi-plugin-runtime/Cargo.toml
[dependencies]
tokio = { version = "=1.49.0", features = ["full"] }
tracing = "=0.1.44"
tracing-subscriber = { version = "=0.3.19", features = ["env-filter", "fmt"] }
tracing-log = "=0.2.0"
log = "=0.4.29"
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.140"
chrono = { version = "=0.4.44", features = ["serde"] }
ordered-float = "=4.6.0"
async-trait = "=0.1.88"
anyhow = "=1.0.98"
```

**Why**: Plugins should only access these crates via the runtime's re-exports (`shared_runtime::tokio`, etc.), never as direct dependencies. Pinning ensures that if a plugin author accidentally adds `tokio = "1.50"` to their own `Cargo.toml`, Cargo will **error at build time** with a version conflict — a clear early failure instead of a silent runtime crash from duplicate symbols.

**Upgrade process**: When upgrading a dependency (e.g., tokio), update the pin in the runtime crate and rebuild everything. The `Cargo.lock` and build hash ensure all components stay in sync.

### `-C prefer-dynamic` Flag
All crates (host + plugins) must be compiled with `-C prefer-dynamic` so they dynamically link to `libstd.so` instead of statically linking it. Without this, the linker fails with "std only shows up once" errors.

Set in `.cargo/config.toml`:
```toml
[build]
rustflags = ["-C", "prefer-dynamic"]
```

### Build Compatibility Hash

Since host and plugins must be built with the same compiler and dependency versions, the shared runtime embeds a **build hash** that plugins check at load time. This prevents cryptic segfaults from mismatched builds.

**How it works:**

1. The shared runtime crate uses a `build.rs` script to compute a hash at compile time from:
   - Rust compiler version (`rustc --version`)
   - `drasi-plugin-runtime` crate version
   - Target triple
   - Profile (debug/release)

2. The hash is exposed as a public constant:
   ```rust
   // In drasi-plugin-runtime (auto-generated by build.rs)
   pub const BUILD_HASH: &str = "a3f9b2c1...";
   ```

3. Each plugin's `plugin_init()` is replaced by a two-step handshake:
   ```rust
   #[no_mangle]
   pub extern "C" fn plugin_build_hash() -> *const u8 {
       // Returns the BUILD_HASH the plugin was compiled against
       shared_runtime::BUILD_HASH.as_ptr()
   }

   #[no_mangle]
   pub extern "C" fn plugin_init() -> *mut PluginRegistration {
       // ... register components
   }
   ```

4. The host's `dynamic_loading.rs` calls `plugin_build_hash()` first and compares it to its own `BUILD_HASH`. If they don't match, it logs an error with both hashes and skips the plugin — no segfault, no UB.

**What it catches:**
- Plugin built with different Rust compiler version
- Plugin built against different `drasi-plugin-runtime` version
- Plugin built for different target (e.g., x86 vs ARM)
- Plugin built with different profile (debug vs release)

### Cross-Platform Support

The `dylib` approach works on all three platforms, with slightly different loading mechanics:

| | Linux | macOS | Windows |
|---|---|---|---|
| **Plugin extension** | `.so` | `.dylib` | `.dll` |
| **Runtime loading** | `dlopen` with `RTLD_GLOBAL` | `dlopen` with `RTLD_GLOBAL` | Not needed — implicit linking |
| **Plugin loading** | `dlopen` (symbols resolved via global table) | `dlopen` (symbols resolved via global table) | `LoadLibrary` (symbols resolved via import library `.dll.lib`) |
| **`-C prefer-dynamic`** | Required (dynamic `libstd.so`) | Required (dynamic `libstd.dylib`) | Needs verification on MSVC targets |

**How it works on each platform:**

**Linux / macOS:**
1. Host loads `libdrasi_plugin_runtime.{so,dylib}` with `RTLD_GLOBAL` — makes all symbols globally visible
2. Host loads each plugin `.{so,dylib}` — unresolved symbols are found via the global symbol table
3. Both share the same tokio, tracing, etc. in memory

**Windows:**
1. When Rust builds the runtime as `dylib`, it produces `drasi_plugin_runtime.dll` **and** `drasi_plugin_runtime.dll.lib` (import library)
2. When Rust builds a plugin `dylib` that depends on the runtime, it links against the `.dll.lib` — standard Windows implicit linking
3. At load time, `LoadLibrary("plugin.dll")` → Windows automatically loads `drasi_plugin_runtime.dll` as a dependency
4. No `RTLD_GLOBAL` equivalent needed — symbol resolution is explicit via import libraries
5. Requirement: `drasi_plugin_runtime.dll` must be in the DLL search path (same directory as the server binary)

**Platform-aware loading in `dynamic_loading.rs`:**
```rust
// Linux/macOS: Load runtime with RTLD_GLOBAL first
#[cfg(unix)]
{
    let flags = RTLD_NOW | RTLD_GLOBAL;
    load_library_with_flags(&runtime_path, flags)?;
}

// Windows: No pre-loading needed — plugins pull in runtime.dll automatically
// Just need runtime.dll in the same directory as the binary
#[cfg(windows)]
{
    // Verify runtime.dll exists alongside the binary
    ensure_runtime_dll_exists(&runtime_path)?;
}

// All platforms: Load plugins normally via libloading
for plugin_path in scan_plugin_directory(&plugin_dir)? {
    let lib = Library::new(&plugin_path)?;
    // ...
}
```

**Open item**: `-C prefer-dynamic` on Windows MSVC targets needs testing. The Rust toolchain ships dynamic `std` for some Windows targets but this must be verified. If it doesn't work, Windows may need a different build configuration or may fall back to static builds only.

## Crate Changes

### Crate: `drasi-plugin-runtime` (in drasi-core)

Currently `crate-type = ["lib", "dylib"]`. This is correct.

**Changes needed:**
- Add all shared dependencies: tokio (full), tracing, tracing-subscriber, tracing-log, log, serde, serde_json, chrono, ordered-float, async-trait, anyhow
- Re-export everything plugins need: `pub use tokio; pub use tracing;` etc.
- Export drasi-lib core types (SourceChange, Element, QueryResult, etc.)
- Export plugin traits (Source, Reaction, BootstrapProvider descriptors)
- Export `PluginRegistration` struct
- Provide `init_tracing()` helper for the host

**What it does NOT contain:**
- No C FFI functions
- No JSON serialization for data crossing boundaries
- No runtime bridge abstractions

### Crate: `drasi-plugin-sdk` (in drasi-core)

**Drastically simplified** compared to the old FFI plan. The SDK now just:
1. Re-exports everything from `drasi-plugin-runtime`
2. Provides the `PluginRegistration` struct and entry point conventions
3. Provides config schema helpers (utoipa integration)

Plugin author experience is UNCHANGED from today's static code:

```rust
use drasi_plugin_sdk::prelude::*;

struct MySource { /* fields */ }

#[async_trait]
impl Source for MySource {
    async fn start(&self, change_tx: mpsc::Sender<SourceChange>) -> Result<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                change_tx.send(SourceChange::Insert { element }).await?;
            }
        });
        Ok(())
    }
    // ... exactly like today
}

#[no_mangle]
pub extern "C" fn plugin_init() -> *mut PluginRegistration {
    Box::into_raw(Box::new(PluginRegistration {
        sources: vec![Box::new(MySourceDescriptor)],
        ..Default::default()
    }))
}
```

### Plugin Crates (sources/*, reactions/*, bootstrappers/*)

**Minimal changes per plugin:**
1. Change `crate-type` from `["lib", "cdylib"]` to `["lib", "dylib"]`
2. Replace direct tokio/tracing/serde deps with `drasi-plugin-runtime` dependency
3. Add `#[no_mangle] pub extern "C" fn plugin_init()` entry point
4. **Remove the `dynamic-plugin` feature flag** — no longer needed (plugins always build both rlib + dylib)
5. Keep existing descriptors (`SourcePluginDescriptor`, etc.) — they work as-is
6. **No code changes to plugin logic** — tokio::spawn, select!, channels all work

The plugin code is identical regardless of whether it's consumed statically (as a Cargo dependency) or dynamically (loaded from .so at runtime). The `plugin_init()` function is always present — in static builds it's simply unused.

**Cargo.toml cleanup per plugin** — remove direct dependencies that are now provided by the runtime:
- Remove: `tokio`, `tracing`, `tracing-subscriber`, `tracing-log`, `log`, `serde`, `serde_json`, `chrono`, `ordered-float`, `async-trait`, `anyhow`
- Remove: the `dynamic-plugin` feature and any conditional compilation gated on it
- Add: `drasi-plugin-runtime = { path = "..." }`
- Keep: plugin-specific deps that aren't in the runtime (e.g., `tokio-postgres`, `tiberius`, `reqwest`, `tonic`, `axum`)
- Update `use` statements: `use drasi_plugin_runtime::tokio;` instead of `use tokio;`, etc.

**Example before → after:**
```toml
# BEFORE
[lib]
crate-type = ["lib", "cdylib"]

[features]
dynamic-plugin = []

[dependencies]
tokio = { version = "1.44", features = ["full"] }
tracing = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
async-trait = "0.1"
reqwest = "0.12"  # plugin-specific

# AFTER
[lib]
crate-type = ["lib", "dylib"]

[dependencies]
drasi-plugin-runtime = { path = "../plugin-runtime" }
reqwest = "0.12"  # plugin-specific — kept
```

### drasi-server Changes

**Removed: `plugin-builder/` crate**
- The `plugin-builder` crate was a dummy crate that existed only to pull plugin crates into the workspace for unified dependency resolution during `cdylib` builds
- With the `dylib` approach, plugins are workspace members built with `crate-type = ["lib", "dylib"]` — no feature flags needed to switch between static and dynamic
- The `plugin-builder` crate and its `dynamic-plugin` feature flag coordination are no longer needed

**Modified: `src/dynamic_loading.rs`**
1. Load `libdrasi_plugin_runtime.so` with `RTLD_GLOBAL` first
2. Check `plugin_build_hash()` before `plugin_init()`
3. Scan plugin directory for `libdrasi_*.so` files
4. Load each with `libloading::Library::new()`
5. Call `plugin_init()` → get `*mut PluginRegistration` → `Box::from_raw()`
6. Register descriptors in `PluginRegistry` via existing `register_all()`
7. Keep `Library` handles alive for the process lifetime

**Modified: `src/builtin_plugins.rs`**
- Keep for `builtin-plugins` feature (static linking fallback)
- When `dynamic-plugins` feature is active, skip static registration

**Minimal changes to: `src/plugin_registry.rs`, `src/factories.rs`**
- These already work with trait objects (`Box<dyn SourceDescriptor>`, etc.)
- The dynamically-loaded descriptors implement the same traits
- May need small adjustments for `Send + Sync` bounds

**New: `.cargo/config.toml`**
- Add `-C prefer-dynamic` to rustflags

**Modified: `Cargo.toml`**
- Remove `plugin-builder` from workspace members
- Feature flags control only the **server's** behavior — plugins have no feature flags:

```toml
[features]
default = ["builtin-plugins"]

# Static linking — selected plugins compiled into the binary
builtin-plugins = [
    "source-mock", "source-http", "source-grpc", "source-postgres", "source-mssql",
    "bootstrap-postgres", "bootstrap-scriptfile", "bootstrap-mssql",
    "reaction-log", "reaction-http", "reaction-http-adaptive",
    "reaction-grpc", "reaction-grpc-adaptive", "reaction-sse",
    "reaction-profiler", "reaction-storedproc-postgres",
    "reaction-storedproc-mysql", "reaction-storedproc-mssql",
]

# Dynamic linking — plugins loaded from .so files at runtime
dynamic-plugins = ["dep:libloading"]

# Individual plugin features (for selective static builds)
source-mock = ["dep:drasi-source-mock"]
source-http = ["dep:drasi-source-http"]
source-postgres = ["dep:drasi-source-postgres"]
source-grpc = ["dep:drasi-source-grpc"]
source-mssql = ["dep:drasi-source-mssql"]
# ... etc for all plugins

[dependencies]
drasi-source-mock = { version = "0.1.7", optional = true }
drasi-source-http = { version = "0.1.7", optional = true }
# ... etc
libloading = { version = "0.8", optional = true }
```

**Build commands:**
- `cargo build` → static build with all plugins (default)
- `cargo build --no-default-features --features dynamic-plugins` → dynamic build, plugins are separate .so files
- `cargo build --no-default-features --features "source-postgres,reaction-log"` → static build with only specific plugins

### Plugin Discovery

Cargo places all dylib outputs into `target/{profile}/` alongside the server binary. The server defaults to scanning its own binary's directory — no post-build steps, no copying, no `xtask`.

**Naming convention filter** — the server loads files matching these patterns (skipping everything else):
- `libdrasi_source_*` / `drasi_source_*` (sources)
- `libdrasi_reaction_*` / `drasi_reaction_*` (reactions)
- `libdrasi_bootstrap_*` / `drasi_bootstrap_*` (bootstrappers)
- Skips `libdrasi_plugin_runtime.*` (shared runtime, not a plugin)
- Skips `libstd.*`, `libserde.*`, etc. (other dylibs that happen to be present)

**Dev workflow:**
```bash
cargo build
cargo run   # server finds plugins in target/debug/ automatically
```

**Production deployment:**
```
/opt/drasi/
  drasi-server                          ← binary
  libdrasi_plugin_runtime.so            ← shared runtime
  libdrasi_source_postgres.so           ← plugins
  libdrasi_reaction_log.so
  ...
```

Just drop everything in the same directory. Done.

**Custom plugin directory**: supported via `--plugin-dir` CLI arg or `pluginDir` in server config, but the default (binary's directory) works out of the box.

**Build output by platform:**

| Platform | Runtime | Plugin Example |
|---|---|---|
| Linux | `libdrasi_plugin_runtime.so` | `libdrasi_source_mock.so` |
| macOS | `libdrasi_plugin_runtime.dylib` | `libdrasi_source_mock.dylib` |
| Windows | `drasi_plugin_runtime.dll` | `drasi_source_mock.dll` |

## What We DON'T Need (compared to old FFI plan)

These are all **eliminated** by the dylib approach:

- ❌ C FFI contract / `extern "C"` functions for every operation
- ❌ JSON serialization for SourceChange, QueryResult, configs crossing boundary
- ❌ Host-side FFI wrapper types (DynamicSource, DynamicReaction, DynamicBootstrapProvider)
- ❌ Runtime bridge functions (drasi_runtime_spawn, drasi_runtime_sleep_ms, etc.)
- ❌ Callback bridges for event delivery
- ❌ SDK macro generators for extern "C" boilerplate
- ❌ Thread-local error strings
- ❌ Plugin-side serde reimplementation
- ❌ Sync-only plugin API
- ❌ Any plugin code rewrites

## Open Questions

1. **Static build support**: Keep `builtin-plugins` feature for single-binary deployment? This adds a dual code path but is useful for simple deployments. Recommend: yes, keep both.

2. **Hot reloading**: The dylib approach could support unloading and reloading plugins. Worth implementing now or defer? Recommend: defer, but design the loading path to not preclude it.

## Phases

### Phase 1: Shared Runtime Dylib ✅
- [x] Redesign `drasi-plugin-runtime` as the shared dylib with all dependencies
- [x] Re-export tokio, tracing, serde, chrono, drasi-lib types
- [x] Export `PluginRegistration` struct and plugin traits
- [x] Provide `init_tracing()` helper
- [x] Add build hash compatibility constant via `build.rs`
- [x] Test: host loads runtime .so with RTLD_GLOBAL, tracing works

### Phase 2: Plugin SDK Update ✅
- [x] Simplify `drasi-plugin-sdk` to re-export from runtime
- [x] Define `plugin_init()` and `plugin_build_hash()` entry point conventions
- [x] Add prelude module for convenient imports
- [x] Document plugin authoring guide

### Phase 3: Convert One Plugin End-to-End ✅
- [x] Converted `source-mock` and `reaction-log`
- [x] Changed crate-type to `["lib", "dylib"]`
- [x] Added `drasi-plugin-runtime` dependency
- [x] Removed `#[cfg(feature = "dynamic-plugin")]` gates from entry points
- [x] Verified tracing/logging from plugin appears in host (PoC)

### Phase 4: Update Host Loading Infrastructure ✅
- [x] Updated `dynamic_loading.rs` for dylib loading (RTLD_GLOBAL + plugin scan)
- [x] Added `dynamic-plugins` feature flag to Cargo.toml
- [x] Updated `builtin_plugins.rs` to be conditional on `builtin-plugins` feature
- [x] `PluginRegistry` and `factories.rs` work with dynamic descriptors
- [x] Factory tests gated with `#[cfg(feature = "builtin-plugins")]`
- [x] Removed `plugin-builder` from workspace members

### Phase 5: Convert Remaining Plugins ✅
- [x] Converted all 24 plugin crates (sources, reactions, bootstrappers) to `dylib`
- [x] All plugins have `drasi-plugin-runtime` dependency
- [x] All `dynamic-plugin` feature flags removed
- [x] All `#[cfg(feature = "dynamic-plugin")]` gates removed
- [x] All plugins export ungated `drasi_plugin_init()` entry point
- [x] Full workspace compiles clean, all tests pass

### Phase 6: Testing
Tests are organized across both repos by responsibility.

#### 6a. Build Hash Tests (`drasi-core/components/plugin-runtime/`)
- [ ] Unit test: `BUILD_HASH` is non-empty and deterministic
- [ ] Unit test: hash includes expected components (rustc version, crate version, target)

#### 6b. Plugin Lifecycle Tests (`drasi-core/components/tests/`)
New `tests/` directory under `components/` with integration tests for each plugin type:
- [ ] **Source lifecycle**: create → start → receive SourceChange via channel → stop → verify Stopped
- [ ] **Reaction lifecycle**: create → start → send Arc<QueryResult> → verify processing → stop
- [ ] **Bootstrap lifecycle**: create → request bootstrap → receive events → verify
- [ ] Each test runs twice via test fixtures: once with the plugin **statically linked**, once **dynamically loaded** from .so
- [ ] Verify tracing spans from plugins are captured by a test subscriber
- [ ] Test shutdown ordering (source stops → reaction drains → reaction stops)

```
drasi-core/components/
├── tests/
│   ├── common/          # Shared test helpers, fixtures, test tracing subscriber
│   ├── source_lifecycle_test.rs
│   ├── reaction_lifecycle_test.rs
│   ├── bootstrap_lifecycle_test.rs
│   └── dynamic_vs_static_test.rs   # Same tests, static vs dynamic loading
├── sources/
├── reactions/
├── bootstrappers/
├── plugin-runtime/
└── plugin-sdk/
```

#### 6c. Plugin Loading Tests (`drasi-server/tests/`)
- [ ] **Static registration test**: With `builtin-plugins` feature, verify all expected plugin kinds appear in `PluginRegistry`
- [ ] **Dynamic loading test**: Build mock source + log reaction as dylibs, load via `load_plugins_from_directory()`, verify registration
- [ ] **Hash mismatch rejection**: Load a plugin .so with wrong `plugin_build_hash()`, verify it's rejected with error (not segfault)
- [ ] **Missing runtime rejection**: Attempt to load plugin without loading runtime .so first, verify clear error

#### 6d. Full Pipeline Test (`drasi-server/tests/`)
- [ ] Source → Query → Reaction end-to-end using mock source + log reaction
- [ ] Verify SourceChange flows through, QueryResult reaches reaction
- [ ] Run with both static and dynamic plugin loading (parameterized test)
- [ ] Verify tracing spans from plugins appear in test tracing subscriber

#### 6e. Cargo.toml Dependency Lint (`CI`)
- [ ] CI script that scans all plugin `Cargo.toml` files
- [ ] **Verify**: all plugins have `crate-type = ["lib", "dylib"]`
- [ ] **Verify**: all plugins depend on `drasi-plugin-runtime`
- [ ] **Verify**: no plugins have `[features] dynamic-plugin = []`
- [ ] Run in CI on every PR

#### 6f. Cross-Platform Verification
- [ ] **Linux**: Full test suite (primary platform, already proven in PoC)
- [ ] **macOS**: Build and run the PoC, verify RTLD_GLOBAL + dylib loading works
- [ ] **Windows**: Verify `-C prefer-dynamic` works on MSVC target
- [ ] **Windows**: Build and run the PoC, verify DLL implicit linking works (LoadLibrary on plugin auto-loads runtime.dll)
- [ ] **Windows**: Verify `drasi_plugin_runtime.dll` is found when placed next to the server binary
- [ ] If Windows `-C prefer-dynamic` doesn't work, fall back to static-only builds on Windows

### Phase 7: Build System & CI
- [ ] Verify `cargo build` produces all dylibs alongside the server binary (no extra steps needed)
- [ ] Docker build integration (multi-stage: build all crates, copy binary + runtime + plugin .so files into final image)
- [ ] CI: run full test suite for both static and dynamic builds on Linux, macOS, and Windows
- [ ] Documentation: how to build and deploy with dynamic plugins
