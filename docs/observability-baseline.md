# Observability compatibility baseline (OBS-01)

[Issue #234](https://github.com/drasi-project/drasi-server/issues/234) establishes
the compatibility prerequisite for OBS-02 through OBS-08. It does not enable new
exporters, change startup logging, install process-global providers, or add
telemetry configuration/routes.

OBS-02 now supplies [typed configuration and init support](telemetry-configuration.md).
OBS-03 adds explicit construction/running ownership and borrowed integration;
its focused local verification is recorded below. Earlier results
below describe only the recorded OBS-01 baseline, not the new ownership batch.
OBS-04 now implements the selected Prometheus/OTLP metrics paths; see
[delivery status](#obs-04-delivery-status) for focused evidence and pending live
qualification. Tracing/process/inspection remain gated. This does not change
the historical OBS-01 qualification claims.

## Local source and plugin ownership

This branch consumes the existing sibling `../drasi-core` through Cargo path
dependencies, including uncommitted changes. The integrated Core baseline is
[`5fc20eec0dc2669f38b928e3794daf8ea712fb71`](https://github.com/drasi-project/drasi-core/commit/5fc20eec0dc2669f38b928e3794daf8ea712fb71)
on `otel-drasi-server-work`. It includes the native query observation bridge
and callback-safety fixes for
[drasi-project/drasi-core#1011](https://github.com/drasi-project/drasi-core/issues/1011).

Local qualification uses Rust 1.95.0 and the sibling Core checkout at that
revision, rebuilding matched plugins using both repositories' lockfiles.
Local path dependencies still compile local edits; `Cargo.lock` does not pin
their contents. CI setup is deferred. The general/release workflows must not be
assumed to reproduce this source pair merely because their Cargo commands use
`--locked`. Do not fetch, switch or reset an existing developer checkout during
build preparation.

Ordinary plugins use ABI **0.18**; native computation plugins use base ABI **2.0**
and envelope wire format **2**. Telemetry extensions are separately negotiated.
Older base layouts cannot be made compatible by their telemetry extension
version. Rebuild plugins from the same Cargo-selected SDK/Host/FFI workspace:
path dependencies alone do not rebuild already installed shared libraries.
Registry plugin downloads are not a fallback for this local baseline.
The optional query-observation extension is negotiated through telemetry V4;
the base ABI and frozen telemetry V1/V2/V3 layouts are unchanged. SDK support
alone is insufficient: the selected query factory must explicitly advertise
instrumentation support. The real-plugin proofs require this capability.

## Qualified dependency selection

| Component | Selected version | Features/purpose |
|---|---|---|
| OpenTelemetry API / SDK | 0.33.1 | Existing metrics; experimental custom/ManualReader for owned legacy loop; trace/testing for compatibility tests |
| OTLP exporter | 0.33.1 | Existing HTTP JSON metrics with blocking reqwest/rustls |
| tracing adapter | 0.34.0 | Core's optional context/completed-span adapter and tests |
| `metrics` facade | 0.24.6 | Core recorder and metric handles |
| Prometheus recorder | 0.17.2 | No default HTTP server/exporter features |
| `metrics-opentelemetry` | 0.24033.1 | Facade-to-OTel recorder, including absolute counters and mutable gauges |

The original OBS-01 recorder/tracing adapters were qualification dependencies.
OBS-04 promotes the metrics adapters to production; tracing qualification remains
test-only. Core keeps its OTel integration behind `otel-context`; ordinary
logging remains independent. Unrelated older OTel versions used transitively by
other libraries do not provide Server's telemetry types.

The approved local `metrics-opentelemetry` 0.24033.1 override fixes cached-hash
collisions using full `metrics::Key` equality across all three handle caches.
It preserves the upstream BSL-1.0 license/notices and exact source commit
`12be6ec64d25aa99992c750c488338c68e65b281`.
See [patch provenance](../patches/metrics-opentelemetry/README.md).
Version/dependencies and exported names/labels are unchanged; the shared Cargo
cache was not modified. The focused real protobuf collision/2,001-series
regressions passed during Task 3; that is not native-bundle requalification.

Do not substitute `metrics-exporter-opentelemetry`: its documented unsupported
operations include `Counter::absolute` and gauge increment/decrement. An SDK
meter provider alone is not a `metrics::Recorder`.

The executable proof is
[`observability_compatibility.rs`](../tests/observability_compatibility.rs).
It checks:

- Host and already completed plugin spans share a batch processor exactly once,
  retaining IDs, parent, trace flags/state, links, event times/attributes, status,
  start/end times and host-assigned identity. It exercises both Tokio flavors.
- Closing the completed-span collector does not close a caller-owned provider.
- Raw counter updates `+3`, absolute `10`, `+2`, absolute `8`, absolute `10`
  produce `12`, not `22` or a decreasing total.
- Scoped generation A absolute `100`, replacement B absolute `10` produce
  `110`. Repeated/lower absolutes do not double count; retired handles cannot
  update the new generation.
- Gauges support set/increment/decrement and reset on retirement.
- Duration observations stay in seconds; explicit buckets include microseconds.
  The proof's small bucket set is a deterministic test fixture, not a deployment
  default or a performance budget.
- Metrics remain active without traces and exclusions prevent registration.
- Rebuilt ordinary and native libraries load through Server's shared discovery
  path, and the explicitly selected native query factory processes real data.
- The real query's input/result counters and second-valued evaluator/commit
  histograms reach the selected OTel recorder with tracing disabled. One input
  produces one result and one evaluation, not host/plugin duplicate counts.
- Under the Debug profile, real evaluator/preparation/finalization spans and
  host commit spans reach the shared trace processor with component identity
  and causal parentage intact. The same run exports query counters and evaluator
  durations through Prometheus, without enabling Persistence-only commit
  histograms.

The managed query workload commits twice: once for evaluation/publication and
once for delivery acknowledgement. Each real commit is observed once, under its
own processing/acknowledgement parent. An empty bootstrap confirmation is not
fresh input or an extra evaluation. These checks complement Core's durable,
bootstrap, scheduled, replay and callback-safety tests; they do not duplicate
the evaluator or manufacture durations around the whole plugin operation.

The Prometheus recorder normalizes dots to underscores but does not add a
`_total` suffix to an existing catalogue name. Unit suffixing is disabled by
default; the proof preserves this setting and verifies duration values/counts,
sum and cumulative bucket counts. OTel exports the same duration values with
unit `s` and non-cumulative explicit bucket counts. Recorder bounds must be set
before the first histogram handle is registered.

## Approved legacy metrics decision

The initial migration policy was explicitly approved for this implementation:

| Configuration | Contract for OBS-02 |
|---|---|
| Neither section | Ordinary logs remain; no exporter is created |
| Root `metrics` only | Preserve the existing published behavior |
| New `telemetry` only | Resolve the new policy in OBS-02 |
| Root `metrics` and any `telemetry` section | Reject with migration guidance |

The rejection also covers tracing-only, inspection-only and `profile: off`
telemetry configurations. There is no silent precedence or second export of
the same observations. OBS-01 records the decision; OBS-02 implements the
typed policy and its validation. Accepted configuration is distinct from
production runtime capability availability.

Preserve `DrasiServer::with_metrics` and the existing
`drasi.query.progress.sequence` gauge, including its scope, labels, generation
and configuration-hash attributes, signed-integer range protection and omission
of unavailable queries. Legacy interval/timeout values remain **milliseconds**:
defaults 10,000/5,000, not the proposed new policy's seconds-valued defaults.

## Endpoint, authentication and transport contract

Legacy progress export remains **OTLP/HTTP JSON**, using the supplied complete
metrics URL (normally `/v1/metrics`). It does not append another path or switch
to protobuf. HTTP(S) endpoints reject embedded credentials, query strings and
fragments. Redirects are disabled; HTTPS verifies certificates. Authentication
can use the OTLP exporter's existing header environment variables. Never log
resolved headers or persist them instead of their configuration references.

For the new signals, use explicit full signal-specific endpoint URLs and an
explicitly selected transport; do not infer gRPC or rewrite signal paths from
a legacy endpoint. OBS-04/OBS-05 must qualify the actual wire transports before
advertising them. In-memory span/metric qualification is not evidence that
custom CA, mTLS, gRPC or an external backend has been tested. Early credential
resolution cannot depend on a plugin secret provider that has not been created.

## Embedding handoff for OBS-03

Compose existing Core capabilities rather than infer ownership from globals:

| Capability | Existing handoff |
|---|---|
| Component logs | `ComponentLogRuntime`, passed with `with_component_logging` |
| Metrics | One `MetricRegistrationContext` per recorder, shared via `TelemetryOptions::with_metric_context` |
| Distributed context | `OpenTelemetryContextAdapter`, passed with `with_context_adapter` |
| Completed plugin spans | `NativeSpanCollector`, passed with `with_native_span_collector` |
| Shared trace processor | Provider gets `OpenTelemetrySpanProcessor` and its sampler; collector gets a clone |

Standalone Server owns providers it constructs. Embedded callers retain
ownership of supplied providers; Server drains only its scopes/workers.
Producers stop before collector drain; providers shut down only after their
users stop. A cancelled wait does not transfer cleanup ownership. Do not ignore
global installation conflicts or silently adopt an unknown provider.

Prebuilt DrasiLib instances must already carry compatible capabilities. OBS-03
must reject unsupported handoffs rather than retrofit live cached handles.
The explicit Server bindings/preparation/shutdown APIs are described in
[OBS-03 runtime ownership](telemetry-configuration.md#obs-03-ownership-apis).
They do not expand this earlier compatibility proof's qualification.

The managed-instance tracing proof explicitly installs a process-wide subscriber
composing component logging with the chosen OTel layer. Core's management
reconciliation task uses that process subscriber; a scoped subscriber around
only the calling future does not cover it. OBS-03 must qualify the embedding
subscriber strategy as well as provider ownership, rather than silently relying
on an unrelated global subscriber.

### OBS-03 local focused qualification

Final local checks on 2026-11-13:

- Selected Server library, CLI, init-output, compatibility, ownership and
  signal suites after the final review fixes: **601 passed, 0 failed, 6 ignored**. Ignored subprocess
  entry points are exercised by their parent tests; two real-plugin proofs
  remain unqualified.
- Host SDK watcher tests: **9 passed** with native watcher features disabled
  and **9 passed** with them enabled.
- Server type check and both repositories' whitespace checks passed.
- Server lint passed with only the existing `result_large_err` and
  `too_many_arguments` categories allowed; strict lint retains seven baseline
  findings. Host SDK own-package lint passed with `--no-deps`.
- Changed-code formatting passed; two existing query-handler/route formatting
  hunks were left untouched.

The full Server test command remains blocked by unchanged older integration
fixtures and examples. Across the final runs, these included
`api_integration_test`, `api_contract_test`, `dto_camelcase_test`,
`enum_serialization_test`, `native_computation_test`, `server_integration_test`,
`snapshot_endpoint_test`, `basic_setup`, `native-computation` and `native-mssql`.
These are not a claim of full-suite, cross-platform or release qualification.

`observability_runtime` adds an isolated caller-owned composed logger/subscriber,
trace processor/provider, native collector and in-memory Prometheus
recorder/context. Two public Servers drive real deferred application-source
processors and Core lifecycle observations on both Tokio flavors. Stopping A
must leave B processing/emitting; caller collection remains open, provider
shutdown calls remain zero, and the caller closes once after B. This is not an
exporter/cdylib parity claim or a replacement for query/source business semantics.

The retained-owner regressions cover management reaping, Core Complete-with-error
versus Incomplete, cancellation/overall/phase budgets, actual graceful HTTP/SSE
connections, actual stalled legacy HTTP/JSON export, and delayed/repeated Unix
signal cleanup. Host SDK owned polling tests cover size/mtime events and retained
joins with native watcher features both enabled and disabled. Existing ignored
plugin-dependent proofs remain ignored; no matched plugin bundle was rebuilt
and requalified for this ownership batch.

The application-owned ManualReader loop removes the inaccessible SDK periodic
thread from Server ownership. Server's owned polling path similarly creates no
notify/debouncer backend. Legacy native SDK entrypoints still exist, without any
new claim of opaque native-thread completion. All borrowed providers/collectors
stay caller-owned. This is the historical OBS-03 qualification: production
metrics belong to the separate OBS-04 record below, tracing remains OBS-05 and
process/inspection remain OBS-06. The example now preserves its
configuration/logs-only default and adds an explicit live metrics mode.

## OBS-04 delivery status

The uncommitted local implementation selects exactly one guarded Prometheus
recorder or explicit HTTP/protobuf OTLP provider/context, shared across actual
instances and native construction paths. The controlled scrape route/listener
and no-scraper upkeep, same-provider OTLP progress, retained blocking workers,
pre-activation rollback and owned-handle restrictions are described in
[telemetry configuration](telemetry-configuration.md).

Recorded focused results from Tasks 1–5 (not new Task 6 runs):

- Core guard: 50 telemetry tests passed, with no-default-features/style checks.
  The static/native-facing common registration boundary is covered; this is
  not a new rebuilt native-callback proof.
- Task 3 OTLP selector: **17 passed, 1 isolated child ignored**; its ordinary
  hostile-environment parent exercised that child. Legacy telemetry **16
  passed**; config **12**, init **3**, exact adapter retirement **1** passed.
  Includes actual protobuf units/buckets, 2,001 distinct series without overflow,
  hash-collision regressions, progress stages/exclusion, redirects, failures/
  timing and retained shutdown. Lib/bin check and scoped style passed.
- Task 4 actual file/initialized/API/store-continuity parents: **4 passed,
  1 isolated child ignored**, both Tokio flavors; public metrics **5 passed,
  1 isolated child ignored** at that checkpoint. The final unpublished-
  preparation regression passed in both flavors.
- Task 5 public metrics: **15 passed, 1 isolated child ignored**; lifecycle
  **72 passed, 2 isolated child entry points ignored**. Signal suite **4
  passed** initially; the changed diagnostic regression was restored and that
  exact test passed again. Directly affected capability/router **31 passed,
  1 ignored**, init **3 unit + 2 integration** passed; lib/bin check, scoped
  formatting and whitespace passed.

These are separate checkpoint counts, **not an aggregate suite total**.
Ignored subprocess entry points were exercised by parents; genuinely
plugin-dependent ignored tests are not counted as passed.

Task 6 now adds the explicit live mock → native query → log example and a
focused actual TLS client/auth boundary test with a test-only trust anchor.
The test exercises the production resolved HTTP seam and the untrusted-peer
failure path; it does not add custom-CA production configuration.
The narrow ignored `native_guard_rejection_precedes_recorder_registration`
proof uses the existing real SDK `telemetry_fixture` to reject a registration
across the actual callback before recorder calls, then accept it on the same
context. It requires a freshly built fixture path supplied explicitly; ignored
is not passed, and no ABI or Core fixture source was changed.
The local-bundle helper has focused provenance/state-isolation tests in the
existing Python runner. **Task 6 tests, shell checks, matching rebuild, actual
Prometheus/OTLP observations and owned-process/port cleanup are pending
controller execution. No new Task 6 pass is recorded here.**

Use the [same observability example](../examples/observability/README.md) for
the final bounded walkthrough. `build-plugins` builds only Server plus the
three matching local libraries, offline with separate target graphs. Live
startup checks build receipts/source/binary fingerprints and copies only those
libraries into a unique owned working/WAL directory. It does not trust old
`target/debug/plugins` contents by filename or download registry packages.
Component names reference Core-assigned UUIDs; there is no implicit instance,
fixed external ID reuse, old-index recovery shortcut or user-state deletion.

New production-format native parity is to be observed there, complemented by
the existing exact-count real-native tests and the narrow callback proof above.
No additional broad synthetic native matrix duplicates Tasks 1–5. The existing ignored native OTel proof
still uses an in-memory adapter; its passing historical result alone cannot
qualify the new HTTP/protobuf exporter. Actual demo request/scrape evidence
must record profiles, counts/types/units, identities, exclusion and OTLP-only
progress before claiming delivery qualification.

Per the user's 2026-11-16 execution update, **do not run a full Server/Core/
workspace suite or the full baseline target** for this handoff. Full native
workspace, cross-platform, CI, release, overhead and hard-RSS qualification
remain unclaimed. The historical baseline recipe below is retained for
reference, not the current Task 6 command list.

## Running the checks

```bash
# Requires the matched sibling Core checkout and its locked graph.
CARGO_TARGET_DIR="$PWD/target" make test-observability-baseline
```

The supplied target directory holds Core's build artifacts; Server qualification
uses its `observability-server/` subdirectory. The two lockfiles can resolve
different dependency versions, and native `lib`/`cdylib` artifacts must not share
one output directory across those graphs. This separation avoids stale crate
identity errors without deleting a developer's existing cache. The copied
runtime test bundle remains in Server's `target/debug/plugins/`.

The target rebuilds Core's context/telemetry fixtures before running its native
telemetry tests, rebuilds Server's local test bundle (including the native query
plugin), checks the locked Server library, reruns legacy loopback HTTP JSON
metrics and explicit query-factory selection tests, and runs all adapter proofs
including the cdylib test. It fails
explicitly if the Cargo-selected plugin source is not local.
The Server library unit suite and Core's observation, graph-observability,
query-metric and callback regression suites were run separately for local
qualification.

For a quick, no-service check without rebuilding plugins:

```bash
CARGO_TARGET_DIR="$PWD/target/observability-server" cargo test --locked --test observability_compatibility
CARGO_TARGET_DIR="$PWD/target/observability-server" cargo test --locked --lib telemetry::tests
```

### Historical OBS-01 local validation

All **six Server adapter/plugin proofs** pass against freshly rebuilt libraries
from the recorded Core revision, including the actual native-query handoff through
the OTel and Prometheus adapters. Local qualification against that revision:

| Gate | Result |
|---|---|
| `make test-observability-baseline` | Passed, including locked Server library check and legacy HTTP JSON tests |
| Core real native query plugin, with OTel / no default features | 15 / 11 passed |
| Server adapter/plugin proofs | 6 passed |
| Server library unit tests | 398 passed |
| Core library observation unit tests | 72 passed, 1 intentionally ignored timing characterization |
| Core graph-observability and query-metric integration tests | 11 passed |
| Core query bridge/callback tests | 8 passed |
| Adapter proof Clippy | Passed with only the two existing library lint categories allowed |

These are not full workspace or release qualification.

## Remaining completion gates

The historical integrated Core revision has the local OBS-01 qualification
results above; current path dependencies also compile subsequent local changes.
CI setup remains deferred; these results are not full release qualification:

- Complete broader Core workspace/recovery qualification as needed. The
  affected suites above pass, but the previously interrupted full Core unit run
  is not a passing result.
- Repair or establish baseline evidence for Server's older integration fixtures:
  `cargo check --locked --lib --tests` still fails in untouched fixtures.
  Examples include component references in
  [`query_input_dto_test.rs`](../tests/query_input_dto_test.rs), name/ID fields in
  [`dto_camelcase_test.rs`](../tests/dto_camelcase_test.rs), deferred factories in
  [`solution_helpers.rs`](../tests/test_support/solution_helpers.rs), async
  configuration persistence in
  [`single_runtime_test.rs`](../tests/single_runtime_test.rs) and
  [`computation_node_creation_test.rs`](../tests/computation_node_creation_test.rs),
  and the removed receipt helper in
  [`native_sqlite_test.rs`](../tests/native_sqlite_test.rs).
  They are not treated as passing or silently excluded from the broader gate.
- Before PR/release qualification, configure CI to check out the matching Core
  revision and rerun the checks on a clean runner. Existing release/cross-platform
  jobs still need their own matched-source qualification.
- `make clippy` still reports seven findings in untouched handlers/services:
  six `result_large_err` and one `too_many_arguments`. The focused adapter proof
  passes with only those categories allowed; this is not a clean result for the
  repository-wide strict lint gate.
- `cargo fmt -- --check` still reports formatting differences in untouched
  handlers, routing/authoring code and older integration fixtures. The changed
  adapter proof is formatted, and `git diff --check` passes. These existing
  differences are not reformatted as part of OBS-01.

OBS-02/03/04 now provide configuration, ownership and production metrics wiring
as separately recorded above. Later issues still own tracing export, process/
inspection, and performance budgets. No new per-instance telemetry policy,
management HTTP tracing or external-backend query proxy is introduced.
