# OBS-03 Runtime Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Initialize Server observability before construction, bind every instance path correctly, and retain cleanup ownership across failure, cancellation and shutdown.

**Architecture:** A Server-owned coordinator separates immutable Core collection bindings from resources Server actually owns. Standalone initialization composes logging once; embedded applications explicitly lend known collection handles. A single lifecycle driver owns accepted construction and cleanup independently of individual waiters.

**Tech Stack:** Rust 1.95.0, Tokio, Axum, tracing/log, and existing sibling Core observability/worker contracts; existing OTel HTTP/JSON exporter retained.

**Spec:** [Approved OBS-03 design](../specs/2026-11-12-observability-runtime-ownership-design.md).

## Global Constraints

- Work in the existing `observability-item3` branch; do not create a worktree or switch the sibling Core branch.
- Reviewed revisions: Server `7233182`; Core `otel-drasi-server-work` at `5fc20eec`. Recheck before execution; do not reset local changes.
- Preserve the current standalone overall shutdown wait of **30 seconds**.
- Preserve the existing **5-second API drain cap**, capped by remaining time.
- Collection drain plus provider flush share `min(telemetry.shutdownTimeoutSeconds, remaining overall time)`; default **5 seconds**, valid **1..=300 seconds**.
- Startup rollback gets one 30-second bounded wait from failure. Timeout does not cancel accepted cleanup; CLI remains alive owning unfinished cleanup, then exits nonzero.
- No production Prometheus/OTLP tracing/new-metrics activation, process collector, inspection routes, new global metric recorder, or provider installation in this issue.
- Preserve legacy HTTP/JSON progress metrics, millisecond settings, gauge semantics and root `metrics`/any `telemetry` rejection.
- Preserve ordinary logs, graph/log/SSE contracts, UUID/configuration ownership, business outputs and replay/receipt formats.
- No new Core instrumentation, FFI changes or dependencies on plugin-backed secrets during early initialization.
- Embedded providers/collectors/logging remain caller-owned; Server never shuts them down.
- Support current-thread and multi-thread Tokio; never use `block_in_place`.
- Reuse Core's validated queue/record/series/link limits. No new numerical capacity defaults.
- Preserve existing dirty/untracked files, particularly the observability example and its documentation.
- Plan authored in the GPT-6 Astra session. Implementation subagents must use **GPT-6.1 Sol** (`gpt-6.1-sol`); do not infer a model override for other agent roles.
- Before implementation, obtain plan confirmation. For terminal validation use the execution subagent; do not run it in parallel with another execution subagent.

## Review Focus

1. A cancelled build after one Core is created must keep that Core and its dependencies owned; Task 2/4 tests interrupt construction before it returns.
2. An instance-creation request racing shutdown must either be rejected before construction or be included in cleanup, even before registry publication; Task 5 tests both sides.
3. Managed background work must reach an embedded caller's actual process subscriber, not merely a scoped subscriber around a build future; Task 7 exercises background publication.
4. A timeout followed by completed cleanup must remain a failed shutdown, while repeated waits do not repeat provider shutdown; Task 6 checks the terminal outcome.
5. A failed global installation can leave irreversible global state behind; Task 3 uses isolated subprocesses and never treats retry as authorization to borrow an unknown provider.

---

## File ownership and execution order

This is one connected lifecycle change, not seven independently shippable projects. Execute sequentially; do not have multiple workers edit Server construction/shutdown concurrently.

| File | Responsibility |
|---|---|
| New `src/telemetry/bindings.rs` | Borrowed public handoff, policy/capability checks and shared Core-builder binding |
| New `src/telemetry/runtime.rs` | Standalone logging composition and owned-versus-borrowed collection resources |
| New `src/server/lifecycle.rs` | Construction operation, cleanup handle/state, admission gate and retained driver |
| New `src/telemetry/runtime_tests.rs` | Focused binding, runtime and deadline tests |
| New `src/server/lifecycle_tests.rs` | Cancellation, ownership and admitted-work races |
| New `tests/observability_runtime.rs` | Subprocess-isolated logging/embedding and Server-path integration |
| `src/telemetry.rs`, `src/telemetry/tests.rs` | Keep legacy exporter; integrate owned cleanup/deadline without changing wire behavior |
| `src/builder.rs`, `src/builder_result.rs`, `src/lib.rs` | Additive public APIs and exports |
| `src/server.rs` | Configured preparation, runtime state transfer, running and shutdown |
| `src/instance_registry.rs`, `src/api/shared/handlers/instance_handlers.rs` | Carry binding/lifecycle context into API creation |
| `src/api/v1/routes.rs`, `src/api/shared/error.rs` | Close admission consistently and report structured shutdown errors |
| `src/plugin_orchestrator.rs` | Coordinate admitted plugin work with the same lifecycle |
| `src/main.rs`, `src/plugin/mod.rs`, `src/init/mod.rs` | Early CLI logging and awaited cleanup on all relevant exits |
| `src/computation.rs` | Preserve typed preparation failures and existing retryable cleanup |
| `Cargo.toml`, `Cargo.lock` | Promote only required logging dependencies; test clock support if needed |
| `tests/shutdown_signal_test.rs` | Actual process signals, empty startup and deadline reporting |
| `docs/telemetry-configuration.md`, `docs/observability-baseline.md`, `README.md` | Accurate availability, embedding and cleanup guidance |
| `examples/observability/README.md` | Describe improved logs without claiming exported telemetry |

Use the existing `src/server/` and `src/telemetry/` module layout; do not move unrelated legacy implementation.
Internal helper bodies and fixture mechanics are implementer choices. The public signatures and ownership rules below are the cross-task contract.

For commands below, use:

```bash
CARGO_TARGET_DIR="$PWD/target/observability-server" cargo ...
```

Server and Core must not share one build-output directory across their lockfiles.
Test snippets specify assertions, not a requirement to copy undefined fixture helpers literally. Each named fixture used by a test is implemented in that test's owning file.

## Task 1: Validated collection bindings

**Files:** Create `src/telemetry/bindings.rs`, `src/telemetry/runtime_tests.rs`; modify `src/telemetry.rs`, `src/config/telemetry.rs`, `src/lib.rs`.

**Interfaces produced:**

```rust
// Public, re-exported from telemetry; Core types use drasi_lib::observability.
#[derive(Clone)]
pub struct TelemetryBindings { /* private fields, redacted Debug */ }
impl TelemetryBindings {
    pub fn new(settings: TelemetrySettings, logging: ComponentLogRuntime) -> Self;
    pub fn with_metric_context(self, context: MetricRegistrationContext) -> Self;
    pub fn with_context_adapter(self, adapter: Arc<dyn TraceContextAdapter>) -> Self;
    pub fn with_native_span_collector(self, collector: NativeSpanCollector) -> Self;
    pub fn with_shutdown_timeout_seconds(self, seconds: u64) -> Self;
    pub fn validate(&self) -> anyhow::Result<()>;
    pub fn apply_to(&self, builder: DrasiLibBuilder) -> anyhow::Result<DrasiLibBuilder>;
    pub async fn validate_prebuilt(&self, core: &DrasiLib) -> anyhow::Result<()>;
}
```

These are **borrowed** handles, including when a test creates them. `apply_to` never installs globals, constructs components, or assigns an overriding instance ID. It uses Core's existing `with_telemetry` and `with_component_logging`.

- [ ] **1. Write failing binding tests** in `runtime_tests.rs`:
  - `bindings_preserve_actual_instance_scope`: two builders named `a`/`b` receive one metric context; their actual scopes remain `a`/`b`, and component observations carry their owning IDs.
  - `enabled_collection_requires_explicit_handles`: enabled metrics without a context, distributed tracing without an adapter, and closed logging all fail before the component factory counter increments (`calls == 0`).
  - `off_does_not_close_borrowed_logging`: Off creates no metric registrations and leaves logging open after Core shutdown (`!logging.is_closed()`).
  - `prebuilt_validation_rejects_exposed_mismatch`: wrong profile/signal/sampling settings or missing required exposed capabilities fail; matching preconfigured Core succeeds.
  - `embedded_shutdown_budget_uses_shared_validation`: accept `1`, `5`, `300`; reject `0`, `301`.
  Representative invalid-handoff assertions (use an atomic constructor counter):
  ```rust
  assert!(bindings.validate().is_err());
  assert_eq!(constructor_calls.load(Ordering::SeqCst), 0);
  ```
- [ ] **2. Run red:** `cargo test --locked --lib telemetry::runtime_tests -- --test-threads=1`. Missing symbols are the initial expected failure; after scaffolding, verify behavioral failures.
- [ ] **3. Implement the interfaces.** Reuse `TelemetrySettings::resolve()` and OBS-02 timeout validation. Compare exposed prebuilt policy/scope/capabilities; compare logging registry identity where possible. Do not invent identity equality for metric contexts/native collectors or claim opaque subscriber inspection. Document unverifiable equivalence as an explicit caller obligation.
- [ ] **4. Keep production gating separate.** A binding is not an OTLP destination or a selected backend. Existing `EffectiveTelemetry::ensure_runtime_available()` remains authoritative for configured destinations; do not weaken YAML validation to make injected tests pass.
- [ ] **5. Run green:** repeat Step 2 and `cargo test --locked --lib config:: -- --test-threads=1`. Update directly related embedding rustdoc. Checkpoint this task alone.

## Task 2: Owned construction and cancellation-safe cleanup

**Files:** Create `src/server/lifecycle.rs`, `src/server/lifecycle_tests.rs`; modify `src/server.rs`, `src/computation.rs`, `Cargo.toml` only for Tokio test-clock support if required.

**Consumes:** Task 1 `TelemetryBindings`.

**Interfaces produced:** Public exports from `server`:

```rust
#[derive(Clone)]
pub struct ServerCleanupHandle { /* shared owner state */ }
pub struct ServerPreparation<T> { /* result waiter + same owner */ }
pub struct ServerCleanupError { /* primary/cleanup causes + handle */ }
pub struct CleanupStatus {
    pub phase: CleanupPhase,
    pub deadline_missed: bool,
    pub pending_workers: usize,
    pub retry_required: bool,
}
pub enum CleanupPhase {
    Preparing, Prepared, Running, Quiescing, StoppingProducers,
    DrainingCollection, ClosingProviders, Complete,
}
impl<T> ServerPreparation<T> {
    pub fn cleanup_handle(&self) -> ServerCleanupHandle;
    pub async fn finish(self) -> anyhow::Result<T>;
}
impl ServerCleanupHandle {
    pub fn request_shutdown(&self) -> anyhow::Result<()>;
    pub fn status(&self) -> CleanupStatus;
    pub async fn wait_until(&self, deadline: tokio::time::Instant) -> anyhow::Result<()>;
    pub async fn retry_until(&self, deadline: tokio::time::Instant) -> anyhow::Result<()>;
}
impl ServerCleanupError {
    pub fn cleanup_handle(&self) -> ServerCleanupHandle;
}
```

Internal contract: `ShutdownBudget::new(deadline: Instant, telemetry: Duration)`,
`api_deadline(&self) -> Instant`, `begin_collection(&mut self) -> Instant`.
Compute `min(overall, now + 5s)` for API and `min(overall, now + telemetry)`
once when collection begins; subsequent phases reuse that collection deadline.

Internal admission contract: `ServerCleanupHandle::admit(&self) -> anyhow::Result<AdmissionGuard>`.
Closing admission and accepting a guard are serialized. The guard counts admitted
work; owner-managed pending construction survives a dropped HTTP/build waiter.
This is a lifecycle list, not a shadow component/observability graph.

- [ ] **1. Write failing lifecycle tests**:
  - `cancelled_preparation_retains_created_core`: pause construction after Core A exists, drop `finish()`, assert rollback requested, A retained and logging open; release the fixture and assert cleanup joins.
  - `cancelled_wait_does_not_cancel_cleanup`: cancel a wait while a producer is stopped behind a barrier; retain handle and retry the wait; stop count remains `1`.
  - `collection_uses_remaining_budget`: with a paused clock, advance to 29 seconds of the 30-second deadline; a 5-second collection allowance produces exactly 1 remaining second. Later phases get no fresh budget.
  - `failure_causes_survive_retry`: original preparation error and failed cleanup remain downcastable; explicit retry joins/retries only the unfinished phase.
  - `shutdown_closes_admission_atomically`: requests accepted before closure are tracked; requests after closure cannot invoke a constructor.
  In the paused-clock budget test, pin the numeric contract:
  ```rust
  assert_eq!(collection_deadline, overall_deadline);
  assert_eq!(collection_deadline - Instant::now(), Duration::from_secs(1));
  ```
- [ ] **2. Run red:** `cargo test --locked --lib server::lifecycle_tests -- --test-threads=1`.
- [ ] **3. Implement owner state and driver.** Establish the owner before spawning via Core's owned-worker helpers. A preparation object can exist before polling; construction starts only when its operation is accepted. Dropping its waiter requests rollback, not driver cancellation. Retain every created Core and plugin/collection dependency before another suspension.
- [ ] **4. Implement incomplete/terminal outcomes.** Cache typed outcomes, not formatted error strings. Separate “wait deadline missed,” “worker still pending,” and “cleanup completed with errors.” `retry_until` is explicit; never spin automatic retries or report a later clean join as erasing an earlier timeout.
- [ ] **5. Verify dropped-handle ownership too.** A running driver retains actual resources and child joins until completion even if all external waiters disappear; terminal success releases cycles. Verify with weak references/drop probes. A log-only `Drop` warning is not sufficient ownership.
- [ ] **6. Run green:** repeat Step 2 on both Tokio flavors (paused-clock numerical tests can be current-thread). Update lifecycle rustdoc, then checkpoint.

## Task 2a: Authoritative Core shutdown outcome (approved prerequisite)

**Why:** Task 2 review found terminal cached Core management errors cannot be
distinguished from unfinished cleanup through the existing public Result.
User explicitly approved changing the existing sibling Core branch narrowly.
Complete this prerequisite, then repair/re-review Task 2 before Task 3.

**Files:** Sibling Core `lib/src/lib_core.rs`, `lib/src/lib.rs`,
`lib/src/management/runtime.rs`, `lib/src/management/close_tests.rs`,
`lib/src/computation/instance.rs` and directly dependent cleanup helpers/tests
only where actual completion evidence requires them. Place public outcome types
with existing lifecycle types; no new dependencies or FFI/persistence changes.

**Produces:** additive public `ShutdownCompletion::{Incomplete, Complete}` and
`ShutdownOutcome { completion: ShutdownCompletion, result: drasi_lib::Result<()> }`,
plus `DrasiLib::shutdown_with_outcome(&self) -> impl Future<Output = ShutdownOutcome>`.
Keep existing `shutdown(&self) -> Result<()>`; preserve its semantics and typed
causes. Completion is authoritative for the accepted instance cleanup, not
inferred from status, a graph-only snapshot, a JoinError or formatted messages.

- [ ] **1. Author focused RED tests** before implementation. Cover successful
  store-close acknowledgement followed by a joined management-driver panic
  (Complete + typed Err); absent/failed close acknowledgement or unfinished
  graph/resource/worker cleanup (Incomplete + Err); normal success; and repeated
  calls without a fresh teardown of completed phases. Use actual Core owners,
  preferably extending existing private close fixtures and lifecycle tests.
- [ ] **2. Request controller RED execution** using the smallest Core test
  selectors in one cargo invocation and Core's own target directory/lockfile.
  Supply the exact command and expected failure; do not use Server's target.
- [ ] **3. Implement outcome propagation** from actual lifecycle boundaries.
  Track acknowledgement and joins independently from historical errors;
  include graph, resources, pending adapters and management/store ownership.
  Unknown release remains Incomplete. Do not mask failures or expose mutable
  ownership internals. Document all public types/methods.
- [ ] **4. Request controller GREEN/checks** for the amended Core targets,
  then a focused independent review of the completion contract.
- [ ] **5. Return the exact API/test evidence to Task 2's implementer.**
  Add its Server regression using a real Core failure or a minimal Core
  test-support fixture when the failure cannot be induced via public stores;
  no production failure-injection switch. Release completed-with-error owners,
  retain sticky errors, and preserve genuinely incomplete-resource tests.

## Task 3: Standalone logging composition and CLI-command ownership

**Files:** Create `src/telemetry/runtime.rs`; modify `src/telemetry.rs`, `src/plugin/mod.rs`, `src/plugin/install.rs`, `src/plugin/remove.rs`, `src/plugin/upgrade.rs`, `src/server/lifecycle.rs`, `src/server/lifecycle_tests.rs`, `Cargo.toml`, `Cargo.lock`; create `tests/observability_runtime.rs`.

**Consumes:** bindings and cleanup coordinator from Tasks 1/2.

**Interfaces produced:**

```rust
// Public because main is a separate binary crate; no raw provider fields.
pub struct StandaloneTelemetry { /* owned capture + cleanup handle */ }
impl StandaloneTelemetry {
    pub fn prepare(log_level: String) -> ServerPreparation<Self>;
    pub fn bindings(&self) -> TelemetryBindings;
    pub fn cleanup_handle(&self) -> ServerCleanupHandle;
}
```

The runtime is an optional low-level composition/handoff API for early
diagnostics, not a mandatory application-owned provider framework.
Transferring it moves ownership; cloning `bindings()` does not. Its internal
binding policy is supplied from the already resolved standalone configuration
before Core construction; the logs-only public initializer defaults to Off.
Only inactive configured production capabilities are accepted in OBS-03.
The lifecycle coordinator may start with no collection bindings/resources,
then accept the actual initialized logging owner before any suspension. This
supports fallible initialization without dummy capture workers or fabricated
successful preparation. Keep the bootstrap and owned-logging slot concrete;
do not add a generic cleanup-participant framework.

- [ ] **1. Write subprocess tests** in `observability_runtime.rs` using the test binary as a child (`--exact` + an environment-selected scenario): initialization before a failing constructor emits its diagnostic, plugin-command diagnostics, invalid `RUST_LOG`, unrelated logger conflict, and unrelated subscriber conflict. Each process installs globals at most once.
- [ ] **2. Assert filter independence:** Off still emits an INFO log; an embedded Debug trace with log policy Off reaches the injected collector while no console log is captured. No default collector/listener/network request occurs. Test plugin discovery logging via its selected registry without loading a real cdylib just for this assertion.
  ```rust
  assert!(captured_console.contains("early constructor diagnostic"));
  assert_eq!(unexpected_export_requests.load(Ordering::SeqCst), 0);
  ```
- [ ] **3. Run red:** `cargo test --locked --test observability_runtime -- --test-threads=1`.
- [ ] **4. Implement composition using Core capture + tracing layers + LogTracer.** Promote `tracing`, `tracing-subscriber` (env-filter/fmt/registry) and `tracing-log` to direct production dependencies using versions already in the lockfile. Do not promote test-only OTel/Prometheus adapters. Keep filters on individual layers. `RUST_LOG` wins; fallback remains `<logLevel>,oci_client=error`, or `warn,oci_client=error` for plugin commands. Avoid late environment mutation.
- [ ] **5. Wire plugin-command entry/exit.** Initialize capture before plugin work; all return/error paths request and await owned cleanup, without `process::exit` inside the owned scope. Failed global installation retains/cleans any created capture; irreversible global changes are reported, never borrowed on retry. Do not change unrelated command output or networking.
- [ ] **6. Run green:** repeat Step 3 and Task 2's relevant cleanup tests. Add runtime rustdoc, then checkpoint. Server run/init/validate integration uses this tested composition in Task 5.

## Task 4: Additive builder and prebuilt-Core APIs

**Files:** Modify `src/builder.rs`, `src/builder_result.rs`, `src/server.rs`, `src/lib.rs`; extend `runtime_tests.rs` and `lifecycle_tests.rs`.

**Consumes:** Tasks 1/2 interfaces and Task 3 standalone composition.

**Interfaces produced:**

```rust
impl DrasiServerBuilder {
    pub fn with_telemetry(self, config: TelemetryConfig) -> Self;
    pub fn with_telemetry_profile(self, profile: TelemetryProfile) -> Self;
    pub fn with_metrics_backend(self, backend: MetricsBackend) -> Self;
    pub fn with_telemetry_bindings(self, bindings: TelemetryBindings) -> Self;
    pub fn prepare(self) -> ServerPreparation<DrasiServer>;
    pub fn prepare_with_owned_handles(self) -> ServerPreparation<DrasiServerWithOwnedHandles>;
    pub async fn build_with_owned_handles(self) -> Result<DrasiServerWithOwnedHandles, DrasiError>;
}
pub struct DrasiServerWithOwnedHandles {
    pub server: Arc<DrasiLib>,
    pub servers: HashMap<String, Arc<DrasiLib>>,
    // private lifecycle owner
}
impl DrasiServerWithOwnedHandles {
    pub fn cleanup_handle(&self) -> ServerCleanupHandle;
    pub async fn shutdown(&self) -> anyhow::Result<()>;
}
impl DrasiServer {
    pub fn cleanup_handle(&self) -> ServerCleanupHandle;
    pub async fn from_cores_with_telemetry(
        cores: Vec<(DrasiLib, Option<String>)>, enable_api: bool, enable_ui: bool,
        host: String, port: u16, config_file_path: Option<String>,
        bindings: TelemetryBindings,
    ) -> anyhow::Result<Self>;
    // Same suffix/signature pattern for one Core: from_core_with_telemetry.
}
```

The owned result owns **Server instance cleanup**, not borrowed providers.
Legacy `build`, `build_with_handles`, `build_core`, `from_core`, `from_cores`
and `DrasiServerWithHandles` retain signatures/shape.

- [ ] **1. Write failing builder tests:** full configuration replaces earlier configuration; conveniences change only profile/backend; repeated backend selection leaves one final selection; final port/UI overrides undergo YAML-equivalent validation. Invalid options invoke no factories.
- [ ] **2. Write ownership tests:** additional builders inherit the Server policy; second-Core construction failure cleans up the first; legacy `build_core`/`build_with_handles` reject a newly Server-owned collection configuration rather than dropping its owner. Borrowed configurations succeed. Old result struct literals still compile.
  Add `legacy_progress_cannot_be_added_to_new_telemetry`: calling the existing
  `DrasiServer::with_metrics` after a new-policy/bindings build returns an error
  without starting either exporter. Keep that existing method's signature.
  ```rust
  assert!(server.with_metrics(legacy_config).is_err());
  assert_eq!(constructor_calls.load(Ordering::SeqCst), 0); // invalid builder case
  ```
- [ ] **3. Run red:** `cargo test --locked --lib builder -- --test-threads=1` plus Task 2's cancellation selector.
- [ ] **4. Implement shared validation and preparation.** Track configured policy versus explicit borrowed policy as mutually exclusive inputs; supplying both is an explicit build error, independent of call order. Each complete setter replaces its own previous value. Do not silently let injected handles bypass a configured backend. Preserve causes when mapping internal `anyhow` failures into existing `DrasiError` returns.
- [ ] **5. Preserve legacy default embedding.** For programmatic builds without new telemetry selection, initialize/borrow Core's **known** default logging service early, before factories. It is not owned by one Server. Explicit configured standalone logging uses Task 3's owned composition; borrowed bindings never install globals.
- [ ] **6. Implement checked wrappers.** Validate supplied prebuilt Cores without replacing handles; retain supplied Cores through failed validation cleanup. Legacy wrappers keep their existing preconfiguration obligations and never acquire collection ownership. Derive IDs from actual Core configuration, not the existing `"default"` error fallback.
- [ ] **7. Run green:** repeat Step 3; verify new public examples compile. Checkpoint only with real standalone/borrowed paths passing; do not leave a success-shaped placeholder.

## Task 5: Configured/API instances and CLI startup share bindings

**Files:** Modify `src/server.rs`, `src/instance_registry.rs`, `src/api/shared/handlers/instance_handlers.rs`, `src/api/v1/routes.rs`, `src/api/shared/error.rs`, `src/plugin_orchestrator.rs`, `src/main.rs`, `src/init/mod.rs`; test in `runtime_tests.rs`, `lifecycle_tests.rs`, `tests/observability_runtime.rs`.

**Consumes:** Binding application, preparation, admission and composition from Tasks 1-4.

**Interfaces produced:**

```rust
impl DrasiServer {
    pub fn prepare_new(
        config_path: PathBuf, port: u16, plugins_dir: PathBuf,
        skip_verification: bool, enable_ui: bool,
    ) -> ServerPreparation<Self>;
    pub fn prepare_new_with_telemetry(
        config_path: PathBuf, port: u16, plugins_dir: PathBuf,
        skip_verification: bool, enable_ui: bool, bindings: TelemetryBindings,
    ) -> ServerPreparation<Self>;
    pub fn prepare_new_with_runtime(
        config_path: PathBuf, port: u16, plugins_dir: PathBuf,
        skip_verification: bool, enable_ui: bool, runtime: StandaloneTelemetry,
    ) -> ServerPreparation<Self>;
}
// crate-private, immutable context; legacy registry constructors stay unchanged.
impl InstanceRegistry {
    pub(crate) fn with_runtime_context(
        self, bindings: TelemetryBindings, cleanup: ServerCleanupHandle,
    ) -> Self;
    pub(crate) fn bind_builder(&self, builder: DrasiLibBuilder)
        -> anyhow::Result<DrasiLibBuilder>;
}
```

`new` awaits `prepare_new().finish()`. The explicit configured-file embedding
variant rejects a simultaneous YAML telemetry section, just like Task 4; do
not persist runtime bindings or invent a serialized embedded backend.
The owned-runtime variant consumes Task 3's initialized logging instead of
reinstalling it, and applies the file's resolved inactive collection policy.
Legacy registries without a runtime context keep their established Core-default
behavior for direct router users; actual Server startup always attaches context.

- [ ] **1. Write failing path tests:** an initially empty Server creates an instance through the real API handler; its policy/logging matches startup bindings. Two instances produce correctly separated identity and logs. A configured managed seed/restoration receives bindings before factory execution. Subprocess tests assert empty startup and missing-config creation diagnostics are visible before any Core exists.
- [ ] **2. Add race tests:** hold an API constructor before publication, begin shutdown, then release it; the new Core must be retired and never escape owner tracking. A request entering after closure gets a structured 503 and runs no factory. Request cancellation before publication still rolls back owned work.
  ```rust
  assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(body["code"], error_codes::SERVER_SHUTTING_DOWN);
  assert_eq!(constructor_calls.load(Ordering::SeqCst), 0);
  ```
- [ ] **3. Run red:** `cargo test --locked --test observability_runtime -- --test-threads=1` and `cargo test --locked --lib server::lifecycle_tests -- --test-threads=1`.
- [ ] **4. Wire binding before resources/components.** Bind configured builders before `ConfigurationStores::configure_host`/restoration and API builders before the same operations. Discovery callback logging uses the selected capture registry, not a passive global accessor. Shared library metadata gets no fabricated instance identity.
- [ ] **5. Wire CLI without duplicate ownership.** Resolve final config first, initialize once, and pass the owned runtime into preparation. Missing-file startup initializes default logging before file creation. Init/validate paths loading plugins get logs before that work, without activating telemetry. Ensure early errors await owned cleanup. A validation-only command can still accept an unavailable policy without constructing an exporter.
- [ ] **6. Close admission coherently.** Use the same gate for API and plugin management work; preserve health access where safe. Add `SERVER_SHUTTING_DOWN` to the standard error-code mapping as 503. Hold accepted-work ownership through constructor/start/publication; HTTP cancellation or an aborted connection cannot release unfinished resources. Shutdown waits for already accepted plugin work and joins hot-reload producers.
- [ ] **7. Verify destination ownership for clone/solution/native paths.** Exercise existing destination-Core operations with a small local fixture; expect the destination ID, not source ID. Those handlers need no alternate telemetry pipeline. Existing Core log subscriptions already use its bound registry; retain response shapes.
- [ ] **8. Run green:** repeat Step 3; run `cargo test --locked --lib instance_registry -- --test-threads=1`. Report obsolete integration fixture failures separately instead of broad migration. Checkpoint.

## Task 6: One shutdown driver, including legacy metrics

**Files:** Modify `src/server.rs`, `src/server/lifecycle.rs`, `src/telemetry/runtime.rs`, `src/telemetry.rs`, `src/telemetry/tests.rs`, `src/main.rs`, `tests/shutdown_signal_test.rs`; extend lifecycle tests.

**Consumes:** the preparation owner, runtime context and admission gate.

**Interfaces produced:** Keep `DrasiServer::run(self) -> anyhow::Result<()>`;
add `DrasiServer::shutdown(&self) -> anyhow::Result<()>` to request/wait via its
handle. Legacy internal `MetricsRuntime` gains
`request_shutdown(&mut self, deadline: Instant) -> anyhow::Result<()>` and
`wait_shutdown_until(&mut self, deadline: Instant) -> anyhow::Result<()>`.
Request and join are separate; waiting again never repeats SDK shutdown.

- [ ] **1. Write failing ordering tests:** record actual stop/drain/close calls with barriers; assert producer stop precedes collector/provider close, logging closes last, and borrowed objects have shutdown count `0`. A failed producer stop keeps dependencies alive.
- [ ] **2. Extend legacy exporter tests:** cancel a shutdown wait, assert the worker slot is still occupied, release the stalled exporter, join successfully, and assert one actual provider shutdown. Preserve existing HTTP/JSON body assertions, gauge values and current-thread progress during a stalled export.
- [ ] **3. Add deadline tests:** at 30 seconds the embedded waiter receives a typed incomplete result and reachable handle; releasing the producer completes cleanup but terminal outcome still records timeout. At the 5-second API cap, report drain failure without losing accepted request owners. A 300-second configured telemetry budget cannot extend a 30-second overall wait.
  ```rust
  assert!(handle.status().deadline_missed);
  assert!(handle.status().pending_workers > 0); // before releasing stalled work
  assert_eq!(borrowed_shutdown_calls.load(Ordering::SeqCst), 0);
  ```
- [ ] **4. Run red:** `cargo test --locked --lib telemetry:: -- --test-threads=1`, then lifecycle tests; run the signal target after it compiles.
- [ ] **5. Replace the split shutdown flow.** Keep instances in lifecycle-owned state rather than taking them into an untracked local future. Transfer the running registry, API task, watcher, plugin dependencies and legacy metrics to that same owner. First signal/explicit shutdown closes admission, establishes one deadline, and starts one accepted driver; later signals/waiters do not duplicate it.
- [ ] **6. Integrate legacy SDK cleanup off-executor.** Replace `timeout * 4 + 5s`/additional join-time allowances as caller budgets with the shared remaining deadline. Blocking SDK work stays in an owned worker. A timed-out SDK call or exporter-stop confirmation is not proof of actual worker termination; retain joins/dependencies and report incomplete cleanup.
- [ ] **7. Implement CLI retained-cleanup behavior.** Catch typed incomplete errors from construction and run. Report immediately; retain runtime and handles while awaiting accepted work. Do not introduce a fresh export/flush allowance after expiry. Safe close/join continues; a phase needing explicit retry remains reported/owned, not busy-looped. Eventual completion after timeout exits nonzero. Completed failure with no outstanding owner exits nonzero without waiting forever.
- [ ] **8. Run green:** `cargo test --locked --lib telemetry:: -- --test-threads=1`; `cargo test --locked --lib server::lifecycle_tests -- --test-threads=1`; `cargo test --locked --test shutdown_signal_test -- --test-threads=1`. Include actual SIGINT/SIGTERM/repeated-signal checks and one delayed-cleanup subprocess proving it remains alive past its deadline, then exits nonzero after release. Use virtual time for exhaustive budget cases, not repeated 30-second wall-clock tests. Checkpoint.

## Task 7: Cross-path proof, docs and focused qualification

**Files:** Extend `tests/observability_runtime.rs`, `tests/observability_compatibility.rs` only if a shared fixture is necessary; update the four documentation paths listed above.

**Consumes:** all prior interfaces; introduces no new production abstraction.

- [ ] **1. Write the remaining embedding proof before fixes.** One subprocess installs known caller logging/tracing plus an in-memory recorder/context. Build two Servers using the public borrowed-binding APIs; trigger real deferred/managed background work and observe both identities. Shut down A, trigger B again, and assert B still emits, the capture/collector remain open, provider shutdown count is `0`, and Server installed no globals. Then caller shuts collection down once.
  ```rust
  assert!(observed_instance_ids.contains("a"));
  assert!(observed_instance_ids.contains("b"));
  assert!(b_records_after_shutdown_a > b_records_before_shutdown_a);
  assert!(!logging.is_closed());
  assert_eq!(provider_shutdown_calls.load(Ordering::SeqCst), 0);
  ```
- [ ] **2. Run red:** `cargo test --locked --test observability_runtime -- --test-threads=1`. If already green, record that the public integration test verifies behavior introduced by prior tasks; do not manufacture a failure.
- [ ] **3. Close only demonstrated wiring gaps.** Repeat on both Tokio flavors; verify config-created/API-created/primary/additional/prebuilt/managed/native/clone/solution paths against the coverage table. Real cdylib/exporter parity stays later qualification unless an OBS-03 change actually requires matched plugin rebuilding.
- [ ] **4. Run focused final tests together:**

  ```bash
  cargo test --locked --lib --bin drasi-server \
    --test init_output_test --test observability_compatibility \
    --test observability_runtime --test shutdown_signal_test -- --test-threads=1
  ```

  Expected: all runnable selected tests pass; preserve and report the existing
  ignored plugin-dependent proofs rather than describing them as newly passed.
  Run directly affected additional integration targets only when their existing
  fixtures compile. Record exact successes, failures and skipped gates.

- [ ] **5. Validate types/style:** `cargo check --locked --lib --bin drasi-server`;
  affected-file `rustfmt --check` using the repository edition/config;
  `cargo clippy --locked --lib --bin drasi-server --test observability_runtime --test shutdown_signal_test -- -D warnings`.
  Existing baseline lint categories were `result_large_err` and
  `too_many_arguments`; inspect exact failures, never broadly suppress new ones.
  If baseline-only, a second run allowing only those categories separates new
  findings. Run `git diff --check`.
- [ ] **6. Update documentation** with ownership-bearing and borrowed examples,
  configuration availability, constructor limitations, global conflicts and
  the distinction between bounded waiting and a hard process-exit guarantee.
  Update the observability example's explanation, not its scope: it still has
  no production exporters/data pipeline. Do not add process metrics here.
- [ ] **7. Review and hand off.** Use requesting-code-review and
  verification-before-completion at their proper execution gates. Summarize
  exact qualification; no claim of release readiness from health/log tests.
  Obtain integration/commit instructions before finishing the branch.

## Checkpoints and commits

Tasks are sequential review units: each consumes the preceding tasks' completed
interfaces and has its own passing test gate. Do not create temporary
success-shaped stubs to make an intermediate commit green.
If commits are requested during execution, commit only dependency-complete
changes after validation, never `git add .`; first satisfy the repository's
pre-commit `cargo test` requirement and report baseline blockers. Include:

```text
Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
```

## Plan self-review and execution handoff

Coverage: initialization/filtering/conflicts -> Tasks 1/3/5; all instance paths ->
Tasks 4/5/7; ownership/builder precedence -> Tasks 1/4; cancellation/deadlines/
startup rollback -> Tasks 2/5/6; legacy progress compatibility -> Task 6;
documentation/qualification -> Task 7. The five Review Focus cases each have a
specific owning test. No production exporter/process/inspection implementation
is included.

Recommended execution: **subagent-driven, sequential**, using GPT-6.1 Sol for
implementation workers. This has higher review cost, but cancellation and
cross-instance ownership mistakes warrant task-local review before integration.
Do not parallelize overlapping lifecycle/construction changes. Plan confirmation
is required before implementation starts.
