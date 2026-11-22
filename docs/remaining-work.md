# Drasi Server remaining work

This is the active list of known gaps, divergences and open decisions for Drasi
Server on the current drasi-core (native computation ABI 0.1.0, ordinary
plugin SDK ABI 0.19.0). Core-owned work is tracked in the
[ComputationGraph remaining work](../../drasi-core/lib/docs/computation-graph-remaining-work.md)
document and is linked from here only where it affects Server.

**How to read it:** an **issue** is an established gap in implemented
behavior; a **divergence** is a deliberate departure from an agreed plan; a
**decision** needs an owner's choice before work starts; **qualification**
means the behavior exists but lacks test evidence. Priority describes the
impact on Server deployments.

| Work area | Entries |
|---|---|
| Persisted instances | [Update data mode](#sw-01), [native graph write-back](#sw-02), [write-back status](#sw-03), [write coalescing](#sw-04), [idempotent replay](#sw-05), [identity provider validation](#sw-06), [store retention after instance delete](#sw-07), [untested paths](#sw-08) |
| Configuration and plugins | [Stream binding mismatch](#sw-12), [README graph example](#sw-13), [ABI admission rule](#sw-14), [plugin rebuilds and examples](#sw-15), [Pause gating](#sw-16), [plugin documentation check and wiring guide](#sw-29), [secret references](#sw-30), [query recovery after a source outage](#sw-38) |
| Native workflows and clients | [Server, UI and client workflows](#sw-20), [required examples](#sw-25), [native plugin logs](#sw-32), [custom plugin deployment](#sw-33), [native PostgreSQL publications in examples](#sw-41) |
| Observability | [OTLP log export](#sw-34), [reaction results metric](#sw-36), [stale observability gates](#sw-37) |
| Build and qualification | [Container suites](#sw-18), [workflow and recovery checks](#sw-21), [honest test runners](#sw-22), [example and tutorial acceptance](#sw-23), [CI selection](#sw-26), [repeated helper tests](#sw-27), [Trading dialog test](#sw-28), [observability test fixtures](#sw-35), [Trading build rewrites a tracked file](#sw-39), [Trading live test](#sw-40) |

## Persisted instances

An instance with a `configurationStore` block is persisted. On first start it
seeds the store from the YAML `sources`, `queries` and `reactions`. After that
the store is authoritative, REST changes are desired-state writes, and YAML is
written back after each durable change. The implementation is in
`src/managed_configuration.rs`, `src/ordinary_resolver.rs`,
`src/api/shared/managed_components.rs` and `src/persistence.rs`. Tests are in
`tests/persisted_instance_test.rs`.

<a id="sw-01"></a>

### Ordinary updates preserve data instead of resetting it (SW-01)

**Kind:** Divergence. **Priority:** Medium.

The persistence plan specifies `{mode: Recreate, data: Reset}` for source and
reaction updates. Core refuses any data plan other than `Preserve` for ordinary
updates, so `ServerOrdinaryResolver::plan_update` returns
`{Recreate, Preserve}` (`src/ordinary_resolver.rs:340-352`).

**Effect:** a PUT that points a source at different upstream data keeps the
old checkpoints and query state. Under a Strict recovery policy, the next
start can fail with a recovery error rather than rebootstrap.

**Work:** once Core accepts `Reset` or `Rebootstrap` for ordinary updates (see
Core [CW-92](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-92)),
return the plan's mode, or choose per change kind. Add a test that a
configuration change which invalidates data is rebootstrapped.

<a id="sw-02"></a>

### The native graph of a persisted instance is not written back to YAML (SW-02)

**Kind:** Divergence. **Priority:** Medium.

YAML write-back renders only ordinary sources, queries and reactions
(`render_desired`, `src/persistence.rs:80`). Native topology may hold values
that were accepted only into the encrypted store; an earlier attempt to
render it wrote a literal secret to plaintext YAML in an existing test. As a
result:

- the first write-back drops the instance's `computation:` block from the
  YAML file (`capture_instance_settings` clears it, `src/persistence.rs:147`).
  The graph is still in the store and runs on restart, but the YAML no longer
  shows it, and it cannot be reseeded if the store is lost;
- REST `DELETE` or `PUT` on a native component returns 409
  `MANAGED_CONFIGURATION_REQUIRED`. Native components change only through
  `computation/desired`. `tests/native_computation_test/managed.rs` was
  updated to expect this.

**Decision:** either keep the original `computation:` seed block verbatim in
the YAML, or render the topology with secret references left unresolved.
The second option needs Core to keep the unresolved references in desired
state.

<a id="sw-03"></a>

### Write-back failures are not visible on a status endpoint (SW-03)

**Kind:** Issue. **Priority:** Medium.

When YAML write-back fails, the accepted change stays applied, the error is
logged, and the error is returned in that REST response. The most recent
failure is kept in `ConfigPersistence::last_write_error`
(`src/persistence.rs:525`), but no API exposes it. The plan's resolution (U-2)
was a status field plus a log.

**Work:** expose the last write-back error on the instance or health response,
and clear it on the next successful write. Add a test.

<a id="sw-04"></a>

### Write-back is serialized but does not coalesce (SW-04)

**Kind:** Issue. **Priority:** Low.

Each durable change triggers a complete YAML render and atomic write under
`save_lock`. Concurrent writes queue behind each other rather than being merged
into one write. This is correct but wasteful under bursts of REST changes.

**Work:** coalesce pending writes so that a queued write renders the latest
desired state once.

<a id="sw-05"></a>

### Idempotent replay checks only the key (SW-05)

**Kind:** Issue. **Priority:** Medium.

Before building a request, every REST write (create, replace, delete and
solution deploy) looks up a receipt for the supplied `Idempotency-Key` and
returns it if one exists (`replayed`, `src/api/shared/managed_components.rs:606`).
The request body and target are not compared. A reused key with a different
body, or on a different endpoint, returns the original outcome instead of 409.
Core's own request-conflict check is never reached in that case.

**Work:** store a request digest (method, path and body) with the receipt, or
compare the replayed request with the recorded desired state, and return 409
`CONFIGURATION_CONFLICT` on a mismatch.

<a id="sw-06"></a>

### REST writes do not validate identity provider names (SW-06)

**Kind:** Issue. **Priority:** Low.

A REST create or PUT that names an `identityProvider` not declared for the
instance is accepted and stored. The error appears later, as a component
construction failure in status. Seeding validates references; the REST path
validates bootstrap references and the plugin kind, but not identity provider
names.

**Work:** validate `identityProvider` against the instance settings before
committing, and return 400.

<a id="sw-07"></a>

### Deleting an instance leaves its store record (SW-07)

**Kind:** Decision. **Priority:** Low.

Following the plan, deleting an instance through the API leaves its
configuration-store record in place. Re-creating an instance with the same ID
and store restores the old desired state, not the request's seed. There is no
API to purge a record.

**Decision:** keep this behavior and document it, or add an explicit purge
option to instance delete.

<a id="sw-08"></a>

### Untested persisted-instance paths (SW-08)

**Kind:** Qualification. **Priority:** Medium.

`tests/persisted_instance_test.rs` covers:

- seeding and restart;
- key-file generation and failing closed without the key;
- REST create, PUT and delete, including a kind change and delete with
  dependents;
- `Idempotency-Key` and `If-Match`;
- write-back failure and solution deploy atomicity;
- restoring provider references.

The following plan tests are not implemented:

| Path | Missing evidence | Blocker |
|---|---|---|
| Cancellation and unconfirmed commit (503 with request ID) | Behavior when the store commit outcome is unknown | Needs a fault-injecting configuration store in Server tests |
| Older-ABI plugin | Behavior of a plugin built for an earlier native ABI patch release | No earlier 0.1.x release exists yet; see [SW-14](#sw-14) |
| Environment-variable secret envelopes | `{kind: EnvironmentVariable}` values are stored unresolved and never written to YAML resolved | Only stored-secret references are tested |
| Identity provider attachment | The provider is attached to the rebuilt component after restart | The test checks only the stored reference |

## Configuration and plugins

<a id="sw-12"></a>

### A mismatched source stream is accepted silently (SW-12)

**Kind:** Issue. **Priority:** Medium.

In `computation.graph`, a source `stream` that differs from its binding (the
default is `<componentId>/out`) is accepted at startup. On the first event the
link becomes `Unavailable`, and nothing is logged.

**Work:** reject the mismatch when loading the configuration.

<a id="sw-13"></a>

### The README graph example does not parse (SW-13)

**Kind:** Issue. **Priority:** Low.

Graph component `id`s must be UUIDs, with `name` holding the readable name, and
connections reference the UUIDs. The README example uses names as IDs and fails
to parse.

**Work:** correct the example and add a test that parses it.

<a id="sw-14"></a>

### Plugin ABI admission is stricter than Core's loader (SW-14)

**Kind:** Decision. **Priority:** Medium.

`src/plugin_install.rs:218-227` installs a native plugin only when its ABI
string exactly equals the host's `ABI_VERSION`. Core's loader
(`accepts_abi_version`,
`drasi-core/components/computation-plugin-abi/src/lib.rs:62-80`) requires the
same major version and, before 1.0, the same minor version, but accepts any
patch. Both are 0.1.0 today. At the first 0.1.x patch release, Server would
refuse plugins that Core can load.

**Decision:** keep exact matching, or call Core's rule so that install and load
agree.

<a id="sw-15"></a>

### Plugin rebuilds, registry artifacts and examples (SW-15)

**Kind:** Issue. **Priority:** High. **Owner:** the plugin work stream.

- Local plugins build for the current ABI with
  `make build-local-plugins-debug` (verified in the last full run). Libraries
  built for an older ABI are rejected at load.
- **Open:** registry downloads (`autoInstallPlugins`) have not been checked
  for published current-ABI builds. Until they are, auto-install may fetch
  libraries the host refuses.
- `examples/move-a-wall` and `examples/gpu-cluster-lab/shared/crates/native`
  and their lockfiles have been ported to the current lifecycle API, but were
  not rebuilt or run in the last full run. See [SW-23](#sw-23).
- The native HTTP and gRPC sources and the SSE sink work. Configure them under
  `computation.graph`, and use `--skip-verification` when the plugins
  directory has no lockfile.

<a id="sw-16"></a>

### Some sources keep working while paused (SW-16)

**Kind:** Issue (Core and plugins). **Priority:** Medium.

Server inherits Core
[CW-96](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-96).
The Bridge, MySQL and SQLite sources, and the profiler, keep their own workers
running while paused; SQL Server finishes the query already in flight. The
PostgreSQL, HTTP and gRPC sources and the SSE sink gate on Pause.

<a id="sw-29"></a>

### Plugin documentation has no version check or native wiring guide (SW-29)

**Kind:** Documentation. **Priority:** Medium. **Effort:** Small to medium.

The plugin documentation now states the current versions: native ABI 0.1.0 and
ordinary ABI 0.19.0 in `docs/plugin-architecture.md`, `README.md`, `CLAUDE.md`,
`docs/main-runtime-integration.md` and `tests/README.md`. The nonexistent
`dynamic-plugins` feature is no longer mentioned, and the build instructions use
the Makefile's real targets. Nothing keeps these statements current: Core's
documentation check
([Source X-09](../../drasi-core/lib/docs/computation-graph-remaining-work.md#source-x-09))
does not read Server's documents.

**Work:** test Server's compatibility table against the ABI constants, and
document the name/UUID receipt and explicit wiring flow for native components
(binding by a future name remains Core
[CW-93](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-93)).

<a id="sw-30"></a>

### Readable configuration cannot express typed secret references (SW-30)

**Kind:** Issue. **Priority:** Medium.

Readable `computation.graph.config` accepts literal values, but plugin fields
marked secret require an unresolved secret reference. Pasting a documented
settings object inline, for example a MySQL source's `settings` with its
`password`, is rejected with "requires a compatible value or unresolved secret
reference". Unverified: the exact validation lines in
`src/computation/authoring.rs` were not re-read.

**Work:** add a typed reference syntax to readable authoring (or a supported
generator for the expanded form), with an example that uses a secret store and
one that uses activation credentials.

<a id="sw-38"></a>

### Queries cannot recover from a source outage without restarting Server (SW-38)

**Kind:** Issue. **Priority:** Medium. **Effort:** Small to medium.

Server builds every query with `recovery_policy: None`
(`src/api/mappings/queries/mod.rs:94`), so its query configuration has no
`recoveryPolicy` and every query uses Strict recovery. When an ordinary
PostgreSQL source fails during a database outage (it reconnects only once; see
Core [CW-112](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-112)),
the queries that read it directly fail with "lost its explicit lifecycle
dependency". After the database returns, `POST /sources/{id}/start` succeeds,
but `POST /queries/{id}/start` answers 500 with "bootstrap provider requires an
explicit reset under Strict recovery: source subscription requires
reset/rebootstrap", and resubmitting the same query configuration is accepted
but leaves the query failed. Only restarting Server recovers the queries.
This was observed in learning-drasi-server's Getting Started tutorial by
stopping its database container for 30 seconds.

**Work:** expose the query recovery policy in Server's query configuration and
API, or add an explicit reset or rebootstrap operation for a failed query, and
test the outage-and-recovery sequence without restarting Server.

## Native workflows and clients

<a id="sw-20"></a>

### Finish native Server and client workflows (SW-20)

**Kind:** Integration/improvement.
**Priority:** High for required workflows. **Effort:** Very large.

**Work:** Complete creation, editing, discovery, lifecycle and result viewing for
the remaining native components in Server, UI and clients. Make waiting,
accepted, running and failed states clear. Resolve the recorded full VS Code
type-check failures in the component and YAML tooling. The concrete gaps are:

- Route first-store Server seeds through Core's common creation preparation and
  acceptance path. `managed_configuration::configure` still assigns omitted
  UUIDs in the Host before storing the initial `DesiredInstance`. It enforces
  external-ID opt-in and preserves stored UUIDs, but duplicates allocation/policy
  instead of using the normal Core creation receipt and validation path. Keep
  the persistent-builder guard and genuine stored-record restoration; do not
  introduce pre-acceptance component construction or a temporary runtime.
- Expose the existing Rust named-configuration-snapshot operations through HTTP,
  including revision-aware create/list/delete/restore and clear receipt semantics.
- Expose supported replacement, in-place settings changes, input/resource
  reconnection and safe quiescing with preview/revision checks. Keep persisted
  desired-state acceptance distinct from realized readiness and rollback.
- Add native-component UI lifecycle/editing controls; its inspector currently
  exposes inspection only, despite existing REST start/stop/removal operations.
- Provide first-class persisted wiring from an ordinary Server-configured query
  into an arbitrary native input. Core execution/data contracts already permit
  this composition, but Server validates native definitions separately and
  rejects external component/pipe/boundary bindings. Preserve snapshot/live
  handover and replacement ownership rather than introduce another graph owner.
- Extend solution templates to native topology and resource recipes, with
  owner-assigned UUIDs, bind-once references and no accidental cross-deployment
  sharing.

Existing API/UI operations do not depend on every connector conversion. Only
workflows requiring a missing family depend on [CW-44](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-44).

**Finish when:** Required workflows run entirely through supported native
components and current instance-scoped APIs, without old factory registrations
or misleading success states. The stale-preview, acceptance, realization,
receipt and instance-isolation cases are owned by [SW-21](#sw-21).

<a id="sw-25"></a>

### Finish the required examples (SW-25)

**Kind:** Issue. **Priority:** High for required examples. **Effort:** Large.

Two example sets do not yet run on the current native stack:

- **GPU Cluster Lab on stock Server.** `examples/gpu-cluster-lab/server/` is a
  reserved deployment; only the embedded variant is implemented. Build the
  stock-Server variant, reusing the shared domain, plugin and UI code.
- **PostgreSQL examples.** These still use the ordinary PostgreSQL source and
  should use the native one (see the source's `DESIGN.md`):
  `postgres-cdc-complete.yaml`, Getting Started steps 3–6, and Trading's
  `trading-sources-only.yaml` and `order-exception-handling.yaml`.

**Done when:** each example builds normally and runs against the stock Server
with documented setup and shutdown, and passes [SW-23](#sw-23)'s acceptance.

<a id="sw-32"></a>

### Native plugin logs at the component log API are untested (SW-32)

**Kind:** Qualification. **Priority:** Medium. **Related:** Core
[CW-24](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-24).

Core now forwards native plugin logs: the SDK installs a host logger in every
loaded plugin, and records made inside a component's telemetry scope become
log messages for that component
(`drasi-core/lib/src/observability/plugin_telemetry.rs:225-250`). Server wires
the component log runtime through `with_component_logging`. The per-component
endpoints are `/sources/:id/logs`, `/queries/:id/logs`, `/reactions/:id/logs`
and their streams (`src/api/v1/routes.rs:162-200`).

**What remains:**

- A Server test that a native source's log line (for example "connection
  refused") appears at `GET /sources/<id>/logs` and its stream.
- A troubleshooting sequence in the docs: discovery, compatibility,
  validation, waiting resources, initialization, activation, processing.
- Records a plugin makes outside a component scope (for example from a
  background task it spawned without the scope) reach the host logger without
  a component tag, so they do not appear at any component endpoint.
  **Unverified:** whether the shipped plugins' background tasks keep the scope.

<a id="sw-33"></a>

### No deployment recipe for custom native plugins (SW-33)

**Kind:** Improvement. **Priority:** Medium.

A vendor has no supported way to run their own native plugin in the Server
image. The image has no dedicated plugin directory (`Dockerfile:94-116`),
verification admits nothing without a lockfile, the query plugin must also be
present, and the Compose example mounts only `/app/config`
(`docker-compose-server-only.yml`). A vendor who mounts `libacme_db.so` gets a
server with no plugins.

**Work:** document a derived-image recipe and a mounted-plugin recipe covering
the plugin directory, the required query plugin, non-root permissions,
configuration and state volumes, lockfile and signing steps for local and
signed builds, and upgrade by restart. Provide a Server profile that matches
Core's standalone plugin starter
([DX-1](../../drasi-core/lib/docs/computation-graph-remaining-work.md#dx-1)).

<a id="sw-41"></a>

### Example databases create publications the native PostgreSQL source refuses (SW-41)

**Kind:** Issue. **Priority:** Medium. **Effort:** Small.

The native PostgreSQL source (`drasi.postgres/shared-changes`) refuses a
publication that publishes TRUNCATE, on every start
(`drasi-core/components/postgres-common/src/replication.rs:380-389`, called
from the source, its worker and both bootstrap paths). PostgreSQL publishes
TRUNCATE unless the publication sets a publish list. These native setups create
their publications without one:

- the Getting Started example: `examples/getting-started/scripts/setup-database.sh:56`
  and `examples/getting-started/database/init.sql:64`, used with
  `examples/getting-started/server-config.yaml`;
- `examples/postgres/qualify.py:279`, for every case except
  `getting-started-server`, which uses the setup script above.

`examples/postgres/qualify-trading.py` failed the same way until its
publications added `WITH (publish = 'insert, update, delete')`, which Trading's
`database/init.sql:117-120` already uses. **Unverified:** this is from reading
the code; neither the Getting Started example nor `qualify.py` has been run
since the check was added. `qualify.py` may also share the component-ID
assumptions `qualify-trading.py` needed fixed: inventory `id`s are UUIDs with a
separate `name`, and query nodes report the role `Query`.

**Done when:** each native example's publications set
`publish = 'insert, update, delete'`, the Getting Started example starts on a
new database, and `python3 examples/postgres/qualify.py` passes every case
against the local build.

## Observability

<a id="sw-34"></a>

### Server does not export logs over OTLP (SW-34)

**Kind:** Issue. **Priority:** Medium. **Effort:** Small to medium.

Server exports metrics and traces over OTLP, but not logs. Its
`opentelemetry` dependencies enable only the `metrics` and `trace` features
(`Cargo.toml:122-125`), and `docs/telemetry-configuration.md:5-6` states that
`logLevel` and `RUST_LOG` control ordinary logging independently of
telemetry. Core leaves export to the host by
design, so this is Server's work.

**Example:** an operator collecting traces in an OpenTelemetry Collector sees
a failed delivery span but has to read the Server's stdout for the matching
log line.

**Done when:** logs can be exported over OTLP with the same endpoint and
resource attributes as traces and metrics, carry trace and component
identifiers, and are documented and tested; or the docs state this as a
deliberate exclusion.

<a id="sw-36"></a>

### The reaction results metric stays at zero (SW-36)

**Kind:** Investigation. **Priority:** Medium. **Effort:** Small.

In the observability walkthrough the log reaction prints results, yet
`drasi.reaction.results_processed` stays at 0
(`docs/observability-baseline.md:418,533,557`). learning-drasi-server's OTel
Observability tutorial reproduces it: the metric stays at 0 for the dashboard,
log and HTTP reactions while each processes results. Core registers the metric
(`drasi-core/lib/src/observability/detail_metrics.rs:264`). **Unverified:** the
cause has not been investigated; it may be a recording gap in the native
reaction path or a label mismatch in the query used.

**Done when:** the metric counts the results each reaction processes, and a
test checks it.

<a id="sw-37"></a>

### Stale completion gates in the observability baseline (SW-37)

**Kind:** Issue. **Priority:** Low. **Effort:** Small.

The "Remaining completion gates" in `docs/observability-baseline.md:683-720`
still list clippy, formatting and fixture `cargo check` failures, all of which
now pass. The note at `:560-566` that the Core #1027 fix is not on the branch
is also out of date. The document also records two minor open behaviours:
two concurrent first `wait_for_completion` callers can get a spurious "not
requested" error (`:379-381`), and shutdown logs a "Cannot collect query
progress" warning (`src/telemetry/progress.rs:19,28`).

**Done when:** the gates reflect the current state, and the two minor
behaviours are either fixed or recorded as accepted.

## Build and qualification

<a id="sw-18"></a>

### Container and application suites not run on the current Core (SW-18)

**Kind:** Qualification. **Priority:** Medium.

`make test-all` (container suites, run with Podman), the SQL Server suite, and
the Trading and Learning applications have not been run against the current
Core with persistence. The two SQL Server tests in the main test run need
`DRASI_MSSQL_TEST_MODE` and fail without it; the SQL Server container image is
x86_64-only.

<a id="sw-35"></a>

### Observability tests need hand-built Core fixtures (SW-35)

**Kind:** Issue. **Priority:** Medium. **Effort:** Small.

The `observability_tracing` and `observability_metrics` tests load plugin
libraries built from the Core checkout, but no make target builds them or sets
their paths. They are `#[ignore]`d, so the normal test run skips them, and
they fail under `--include-ignored` unless a developer first runs, in Core:

`cargo build -p drasi-source-http -p drasi-computation-source-http -p drasi-computation-source-grpc --features dynamic-plugin`

and `cargo build -p drasi-computation-plugin-sdk --example telemetry_fixture`,
then sets `DRASI_OBS05_FIXTURES_DIR=<core>/target` and
`DRASI_OBS04_NATIVE_FIXTURE=<core>/target/debug/examples/libtelemetry_fixture.dylib`.

**Done when:** one make target builds the fixtures and runs the tests with
these variables set, CI uses it, and a missing fixture produces a clear
message.

<a id="sw-21"></a>

### Complete the Server workflow and recovery checks (SW-21)

**Kind:** Qualification.
**Priority:** High. **Effort:** Large.
**Depends on:** Core [CW-36](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-36), and only the affected Core [CW-44](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-44) and [SW-20](#sw-20) capabilities.
**Work:** Cover the remaining combinations of delayed or failed startup, saved
configuration, reconstruction, reconnecting clients, old readers, native providers
and interrupted delivery. Include unavailable secrets/provider recipes, rejected
versions, lost acceptance replies, concurrent snapshot/configuration changes,
slow observers and reconnect after a result-history gap. A client must obtain
an authoritative replacement snapshot or explicit failure, not retain stale rows.
Record the actual Server and plugin versions used.

Include [SW-20](#sw-20)'s stale previews, rejected acceptance, delayed/failed
realization, receipt lookup after lost replies and two instances created from
the same template. For reconstructible recipes, cover missing
factories, incompatible versions, unavailable secrets, failed construction and
cancelled restoration, with truthful retryable declarations and no orphaned
resources.

**Completed boundary:** Configured and API-created ordinary queries use explicit
native registration without a linked fallback. [CW-78](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-78) owns current and
historical execution totals, including the query-progress OTLP, plugin-dependent
E2E and crash-parent cases. Resolver reconstruction is not automatically
packaged-server/network crash coverage. Keep the actual stock-binary check
identified in [SW-22](#sw-22), and live tutorial acceptance in [SW-23](#sw-23).

**Finish when:** The selected current-version workflows have trustworthy
end-to-end results, including failure and restart, rather than only matching
intermediate captures.

<a id="sw-22"></a>

### Make test entrypoints and smoke results honest (SW-22)

**Kind:** Confirmed runner defects.
**Priority:** High. **Effort:** Small-medium. **Confidence:** High, except the conditional performance-launcher consolidation.

| Existing entrypoint | Confirmed problem | Required change |
|---|---|---|
| `tests/run_all.sh:53-91` | All four scripts it conditionally seeks are absent; `FAILED=0` can print "All tests passed" after zero execution. | Retire it or forward to one owned, documented entrypoint with nonempty selection and execution checks. |
| `tests/run_all_cargo_tests.sh:6,25-67` | Under `set -e`, the first successful `((PASSED++))` returns a failing status. It names absent `api`, `api_create_query_joins` and `library_integration` targets, then repeats selected work with broad `cargo test`. | Replace the obsolete target list/aggregator rather than preserve misleading success/count logic. |
| `tests/plugin_smoke_test.sh:124-160,176-201,281-315,370-397` | Generated config uses removed root `id/sources/queries/reactions`; `make build-dynamic` is absent; `--skip-build` maps static/dynamic labels to the same binary; arbitrary non-2xx creation responses become skips. | Use current `instances` configuration, one identified artifact set, explicit required plugins and failure on unexpected HTTP responses. Keep genuine external-prerequisite outcomes distinct. |
| `Makefile:191-211` and `CLAUDE.md:44-51,381-385` | `make test-all` calls that smoke script and promises VS Code execution it does not perform; the guidance names absent `tests/run_working_tests.sh`. | Align commands, documentation and actual selected suites. The successful Cargo baseline is not proof that this aggregate command works. |
| `learning-drasi-server/tutorials/trading/app/package.json:30` | `npm test` is a successful `echo 'No tests configured yet'`. | Remove the success-shaped no-op or replace it with real assertions; do not attribute the separate Server Trading suite to this app. |

Preserve an actual stock-binary test rather than substituting only Router tests.
`single_runtime_test::native_binary_loads_plugins_and_processes_http_events`
already loads independent HTTP/log libraries and checks exact query rows and
reaction output (`tests/single_runtime_test.rs:658-827`). Its
"native" label identifies the graph runtime, not the HTTP/log plugin ABI family.
Current `DrasiServerConfig` rejects unknown root fields
(`src/config/types.rs:92-164`).

**Conditional candidate:** `drasi-core/query-perf/run_perf_tests` is a legacy
diagnostic launcher, not a qualification gate. It invokes development-profile
`cargo run` 12 times; the program discards output correctness, and an unknown
scenario exits successfully (`query-perf/src/main.rs:49-59,187-212`).
Do not delete it until [CW-40](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-40) maps and preserves its four scenarios and
memory/RocksDB/Redis distinctions. Confidence in consolidation benefit is
medium; measure first.

**Finish when:** Each advertised runner executes its declared scope, fails on
empty/unexpected selection and real regressions, and reports unavailable
prerequisites separately. Obsolete paths are removed or intentionally forwarded,
with their useful coverage preserved.

<a id="sw-23"></a>

### Strengthen live example and tutorial acceptance (SW-23)

**Kind:** Confirmed weak assertions and application qualification.
**Priority:** High for required examples. **Effort:** Medium-large.
**Depends on:** The missing example implementations in [SW-25](#sw-25).

The real Server getting-started CI script leaves bootstrap and aggregation
correctness unchecked: `test_query_results` waits five seconds and reads query
configuration, and `test_aggregation_results` checks only query ID/success.
The data assertion inserts Alice, sleeps five seconds and greps for "Alice."
Because the initial seed contains different people, that is a real but shallow
CDC smoke, not an automatically vacuous assertion.

**Work:** Strengthen that existing test with literal seeded filtered rows and
histogram, a uniquely identified insert, exact update/delete/count transitions,
bounded result polling/readiness, HTTP status and parsed JSON. Remove its
unconditional sleeps without weakening diagnostics. Evidence:
`tests/integration/getting-started/run-integration-test.sh:156-220`,
`setup-postgres.sh:81-93`, `config.yaml:57-83` in the same directory, and
`.github/workflows/integration-test-getting-started.yml:52-64`.

Retain the seven migrated tutorials' existing scenario checks and automate
uncovered walkthrough/CI cases with exact rows, effects and visible updates,
bounded waits and repeatable reset. Reuse existing tools rather than introduce another
tutorial framework. Building Comfort's verifier already derives an independent
source-data arithmetic oracle and checks all six query/dashboard snapshots
(`learning-drasi-server/tutorials/building-comfort/scripts/verify_demo.py:34-130`).

**Recorded Learning acceptance:** All seven tutorials have matching local-build
evidence. Getting Started covers real source changes, joins, aggregates, timers
and full Server restart; SSE CLI exercises the real stream and its binary-dependent
checks. Building Comfort and Curbside cover exact query/dashboard data, real
database changes and rendered live panels, including alert retraction and
timer-only changes. High-risk Containers uses actual Kubernetes Pods and
PostgreSQL risk rows. OTel uses the real Collector/store services for rollouts,
SLO cycles, absent-exporter timing and dependency expiry/recovery. Trading covers
all eleven queries, fifty prices, eight holdings, query-chain snapshot/live
updates, real UI CRUD, SSE stop/restart with an offline change and reload without
reprovisioning. Building Comfort's open browser also resynchronizes after a full
Server restart. Owned disposable infrastructure is cleaned up.

This is local macOS/Podman evidence, not a Linux CI/devcontainer run, optional
external Discord delivery or a new proof of every recovery profile. The weak
Server shell assertions above and broader application/platform qualification
remain separate work. Runnable Stockroom book variants and native network
lessons are also checked against the current local dependency lockfile.

Move-a-Wall, the embedded GPU Lab and tutorial migrations have earlier live
evidence. Re-run affected acceptance against the current bundle, including
snapshot/restart; do not reopen completed migrations or label old evidence as
a current run. Preserve GPU's real handover/atomic paired-value/SIGKILL/slot/
missing-history checks and Trading's real provisioning/CRUD/live/reconnect
tests; the Trading live test must first move to the native path
([SW-40](#sw-40)). In-process projection/UI units cannot substitute for them.

The six SSE CLI subprocess methods require `DRASI_SSE_CLI`; all skip without it
(`learning-drasi-server/tutorials/sse-cli/tests/test_cli.py:1-20,86-195`).
[CW-85](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-85) owns CI selection and explicit environment jobs.

**Finish when:** Required examples build and run normally with observable
readiness, exact data/effects, safe reset/shutdown/recovery and recorded
current-source provenance. Compilation alone, a success marker or a request
returning 200 does not establish this.

<a id="sw-26"></a>

### Server suites missing from CI (SW-26)

**Kind:** Issue. **Priority:** High. **Effort:** Medium.

`.github/workflows/test.yml` runs only `make test-tooling` and
`cargo test --locked`, so ignored and end-to-end tests never run (about 54
`#[ignore]` sites under `tests/` and `src/`, each with a reason). Other suites no workflow runs:

- `npm --prefix ui test` (`react-package.yml` builds the UI but doesn't test it);
- the VS Code extension tests (`dev-tools/vscode/drasi-server`, `npm test`);
- `examples/move-a-wall/ui` and `examples/gpu-cluster-lab/shared/ui`
  (`vitest run`);
- the stock-binary case
  `single_runtime_test::native_binary_loads_plugins_and_processes_http_events`,
  which needs `DRASI_NATIVE_TEST_PLUGINS`.

**Done when:** each suite runs in a named PR or scheduled job, or is listed
as deliberately excluded with the reason. Related: [SW-18](#sw-18),
[SW-22](#sw-22).

<a id="sw-27"></a>

### Test helper self-tests run about 95 times (SW-27)

**Kind:** Improvement. **Priority:** Low. **Effort:** Small.

`tests/test_support/solution_helpers.rs:594-640` holds five `#[cfg(test)]`
tests, such as `test_create_test_solution_template`. Every test file that
declares `mod test_support;` (20 of them) compiles and runs these five again.

**Done when:** they run in one test target, and assert on parsed YAML/JSONL
instead of string presence.

<a id="sw-28"></a>

### Trading dialog accessibility test fails in parallel (SW-28)

**Kind:** Investigation. **Priority:** Low. **Effort:** Small.

`examples/trading/app/test/integration/dialogAccessibility.test.tsx` is
reported to fail when run in parallel with other tests. **Unverified:** not
reproduced.

**Done when:** it is reproduced, and the shared state it depends on is
isolated so it passes in parallel without being excluded.

<a id="sw-39"></a>

### Building the Trading example rewrites a tracked file (SW-39)

**Kind:** Issue. **Priority:** Low. **Effort:** Small.

`examples/trading/app/scripts/export-native-recipe.ts` regenerates the tracked
`examples/trading/app/public/native-computation.json`, and the generator assigns
new component UUIDs on every run. Each build of the example, including the
learning-drasi-server Trading tutorial's build, therefore leaves a large diff in
that file. The app (`app/src/drasi/config.ts:31`) and the price generator
(`mock-generator/simple_price_generator.py:103`) read the generated file.

**Done when:** building the example leaves the checkout unchanged, for example
by deriving stable component IDs from names or by writing the bundle to an
untracked location that the app and the price generator read.

<a id="sw-40"></a>

### Trading's live browser test still targets the legacy plugins (SW-40)

**Kind:** Issue. **Priority:** Medium. **Effort:** Medium.

`npm run test:live` in `examples/trading/app` runs in
`.github/workflows/react-package.yml:212-221` on pull requests to `main` that
touch `src/**` or `examples/trading/**`. It predates native Trading and the
component-ID changes, and cannot pass against the current Server:

- `test/live/run.ts:246-262` configures ordinary `kind: 'http'` and
  `kind: 'postgres'` sources with a `scriptfile` bootstrap, and `run.ts:270-279`
  installs pinned, signed registry plugins. The current Server answers
  `Missing plugins required by configuration: source/http (referenced by source 'price-feed')`.
  Trading now starts an empty instance (`server/trading-sources-only.yaml`) and
  loads the native plugins.
- `run.ts:320-321` waits for a source whose `id` is `price-feed`. The sources
  API returns UUID `id`s with a separate `name`, so readiness never passes.
- `test/live/trading.live.spec.ts:111` posts prices to
  `/sources/price-feed/events`. The native HTTP source's route takes the
  source's UUID and answers 400.
- `trading.live.spec.ts:135-140` expects 11 `POST …/queries` requests. The app
  installs its components in one `POST …/computation/components`
  (`app/src/drasi/ensureTradingResources.ts:242`) and never creates queries
  that way (`:267`).
- `run.ts:58` requires the `docker` CLI, and `run.ts:65-71` runs
  `test/live/source_provenance.py`, which rejects path dependencies
  (`:29-34`). `test/live/runtime-pins.json` pins Server 0.2.1 and SDK 0.10.0.
  A checkout that uses the local `drasi-core` path dependency cannot pass this
  gate.

The spec's name-based REST reads still work, because Server resolves names in
REST paths.

**Work:** rebuild `run.ts` on the native path that
`examples/postgres/qualify-trading.py` uses: an empty instance, local native
plugins, `native-postgres settings` and the app's setup step. Look up the price
source's UUID by name before posting, and replace the query-request assertion
with the computation-components request. Keep the provisioning, CRUD, live and
reconnect coverage that [SW-23](#sw-23) preserves.
