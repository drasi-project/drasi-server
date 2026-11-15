# Drasi Server remaining work

This is the active list of known gaps, divergences and open decisions for Drasi
Server on the ABI 4 drasi-core. Core-owned work is tracked in the
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
| Runtime and restart | [Query start after restart](#sw-10), [baseline test failures](#sw-11) |
| Configuration and plugins | [Stream binding mismatch](#sw-12), [README graph example](#sw-13), [ABI admission rule](#sw-14), [plugin rebuilds and examples](#sw-15), [Pause gating](#sw-16), [ABI documentation](#sw-29), [secret references](#sw-30), [stored-procedure examples](#sw-31) |
| Native workflows and clients | [Server, UI and client workflows](#sw-20), [required examples](#sw-25), [native plugin logs](#sw-32), [custom plugin deployment](#sw-33) |
| Build and qualification | [Workspace clippy](#sw-17), [container suites](#sw-18), [workflow and recovery checks](#sw-21), [honest test runners](#sw-22), [example and tutorial acceptance](#sw-23), [CI selection](#sw-26), [repeated helper tests](#sw-27), [Trading dialog test](#sw-28) |

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
| Older-ABI plugin | Behavior of a plugin from an earlier ABI 4 minor version | Plugins are not yet ported ([SW-15](#sw-15)) |
| Environment-variable secret envelopes | `{kind: EnvironmentVariable}` values are stored unresolved and never written to YAML resolved | Only stored-secret references are tested |
| Identity provider attachment | The provider is attached to the rebuilt component after restart | The test checks only the stored reference |

## Runtime and restart

<a id="sw-10"></a>

### Queries fail to start after a server restart (SW-10)

**Kind:** Issue (Core). **Priority:** High.

After a restart that reloads configuration,
`reaction_removal_operational_test::matched_server_removes_sse_and_log_checkpoints_and_recreates_public_names`
fails deterministically. `POST queries/<id>/start` returns 500 with
`native activation failed: {...: NotCreated}`, because the query's nodes are
still waiting for the source's resources when activation runs. The likely
cause is the ordering of reload and registration in Core's restart and
reconcile path. This defect predates the ABI 4 port.

**Work:** investigate in Core and record it as a CW item; then rerun the test.
The persisted-instance restart tests do not hit it, but it applies to any
persisted server with native queries.

<a id="sw-11"></a>

### Baseline test failures (SW-11)

**Kind:** Issue. **Priority:** Medium.

The full test run fails 17 tests. The same tests failed before the ABI 4 port
and before the persistence work. They include:

- 6 `native_computation_test::shared_storage` tests: "live resource updates
  must retain their identity, role, ownership and binding interface";
- 6 `native_computation_test::native_services` tests: retirement rules
  ("recovery configuration cannot change without verified drain"), a 503
  `CONFIGURATION_UNCONFIRMED` where the test expects 409, and one elapsed
  deadline;
- the 2 crash-worker child tests;
- the restart defect in [SW-10](#sw-10);
- 2 SQL Server tests that need `DRASI_MSSQL_TEST_MODE`.

**Work:** align the Server test expectations with Core's update and retirement
rules, or fix the rules in Core where the tests describe the intended
behavior.

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

`src/plugin_install.rs:218-224` installs a native plugin only when its ABI
string exactly equals the host's `ABI_VERSION`. Core's loader accepts the same
major version with an equal or older minor version (`accepts_abi_version`).
Both are 4.0.0 today. At the first 4.x minor release, Server would refuse
plugins that Core can load.

**Decision:** keep exact matching, or call Core's rule so that install and load
agree.

<a id="sw-15"></a>

### Plugin rebuilds, registry artifacts and examples (SW-15)

**Kind:** Issue. **Priority:** High. **Owner:** the plugin work stream.

- `target/debug/plugins` must be rebuilt for ABI 4 with
  `make build-local-plugins-debug`. ABI 3 libraries are rejected with
  `native error 8`.
- Registry downloads (`autoInstallPlugins`) publish only ABI 3 builds and fail
  until ABI 4 builds are published.
- `examples/move-a-wall` and `examples/gpu-cluster-lab/shared/crates/native`
  need the lifecycle port and fixes for older API drift. The example
  lockfiles are stale.
- The native HTTP and gRPC sources and the SSE sink already work. Configure
  them under `computation.graph`, and use `--skip-verification` when the
  plugins directory has no lockfile.

<a id="sw-16"></a>

### Some sources keep working while paused (SW-16)

**Kind:** Issue (Core and plugins). **Priority:** Medium.

Server inherits Core
[CW-96](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-96).
The Bridge, MySQL and SQLite sources, and the profiler, keep their own workers
running while paused; SQL Server finishes the query already in flight. The
PostgreSQL, HTTP and gRPC sources and the SSE sink gate on Pause.

<a id="sw-29"></a>

### Plugin documentation states the wrong ABI versions (SW-29)

**Kind:** Issue. **Priority:** High.

`docs/plugin-architecture.md:13-16` lists the ordinary ABI as 0.17 (also
accepting 0.16) and the native ABI as 1.0; the host uses ordinary 0.19.0 and
native 4.0.0. `README.md:1139` says plugin management needs a
`dynamic-plugins` feature, which `README.md:3453` and
`docs/plugin-architecture.md:9` correctly say does not exist. A plugin author
following the table builds against the wrong ABI, and the plugin is refused at
load.

**Work:** generate or test the compatibility table against the ABI constants,
remove the `dynamic-plugins` note, and document the name/UUID receipt and
explicit wiring flow for native components (binding by a future name remains
Core [CW-93](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-93)).

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

<a id="sw-31"></a>

### Stored-procedure sink examples hide templates in a secret (SW-31)

**Kind:** Issue. **Priority:** Low. **Depends on:** Core
[SP-04](../../drasi-core/lib/docs/computation-graph-remaining-work.md#sp-04).

The native MySQL and SQL Server examples and the SQL Server test pass each
stored-procedure sink's whole `settings` object as one secret JSON reference
(`examples/native-mysql.rs:246`, `examples/native-mssql.rs:192`,
`tests/native_mssql_test.rs:41-65`), because the sink factories mark all of
`settings` secret. Operators therefore cannot read a sink's templates in the
Server configuration.

**Work:** once SP-04 marks only credentials and TLS keys secret, generate plain
`settings` with per-field references, for example
`settings: {host: …, password: secret:DRASI_MYSQL_PASSWORD, default_template: {…}}`
instead of `settings: env-json:DRASI_MYSQL_SINK_SETTINGS`, and update the test
assertions.

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

Two example sets do not yet run on the native ABI 4 stack:

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

### Native plugin logs do not reach the component log API (SW-32)

**Kind:** Issue. **Priority:** Medium. **Depends on:** Core
[CW-24](../../drasi-core/lib/docs/computation-graph-remaining-work.md#cw-24).

The per-component log endpoints (`/sources/:id/logs`, `/queries/:id/logs` and
their streams, `src/api/v1/routes.rs:127-145`) receive ordinary-plugin logs
through the legacy log callback. The native ABI has no log callback, so a
native source that logs "connection refused" shows nothing at
`GET /sources/<id>/logs`.

**Work:** once Core forwards native diagnostics, route them to the
per-component log API, add a troubleshooting sequence (discovery,
compatibility, validation, waiting resources, initialization, activation,
processing), and test that a native plugin log line appears.

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

## Build and qualification

<a id="sw-17"></a>

### Whole-crate clippy fails on existing code (SW-17)

**Kind:** Issue. **Priority:** Low.

`cargo clippy --all-targets` and `make clippy` fail on errors in:

- `src/server.rs` around line 1339 (identical `if` blocks);
- `tests/test_support/solution_helpers.rs` and
  `tests/test_support/mock_components.rs`;
- `tests/reaction_removal_operational_test.rs` and `api_persistence_test`
  (`unwrap`, `println!`, `vec!`).

The library, binaries and persisted-instance tests are clippy-clean.

**Work:** fix these warnings so that the whole-crate clippy run can gate CI.

<a id="sw-18"></a>

### Container and application suites not run on ABI 4 (SW-18)

**Kind:** Qualification. **Priority:** Medium.

`make test-all` (container suites, run with Podman), the SQL Server suite, and
the Trading and Learning applications have not been run against ABI 4 Core
with persistence. They depend on [SW-15](#sw-15).

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
bounded waits and repeatable reset. The Learning getting-started evaluator
already has expected-output scenarios; that is distinct from the weak Server
shell assertions above. Reuse existing tools rather than introduce another
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
tests. In-process projection/UI units cannot substitute for them.

The agentic tutorial workflow is supplementary, not an independent sole gate.
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
`cargo test --locked`, so ignored and end-to-end tests never run (about 37
`#[ignore]` sites under `tests/` and `src/`). Other suites no workflow runs:

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

`tests/test_support/solution_helpers.rs:592-640` holds five `#[cfg(test)]`
tests, such as `test_create_test_solution_template`. Every test file that
declares `mod test_support;` (19 of them) compiles and runs these five again.

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
