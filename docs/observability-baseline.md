# Observability compatibility baseline (OBS-01)

[Issue #234](https://github.com/drasi-project/drasi-server/issues/234) establishes
the compatibility prerequisite for OBS-02 through OBS-08. It does not enable new
exporters, change startup logging, install process-global providers, or add
telemetry configuration/routes.

## Local source and plugin ownership

This branch consumes the existing sibling `../drasi-core` through Cargo path
dependencies, including uncommitted changes. The integrated Core baseline is
[`5fc20eec0dc2669f38b928e3794daf8ea712fb71`](https://github.com/drasi-project/drasi-core/commit/5fc20eec0dc2669f38b928e3794daf8ea712fb71)
on `otel-drasi-server-work`. It includes the native query observation bridge
and callback-safety fixes for
[drasi-project/drasi-core#1011](https://github.com/drasi-project/drasi-core/issues/1011).

The [Observability Baseline workflow](../.github/workflows/observability-baseline.yml)
checks out Server and that exact Core commit into sibling directories, selects
Rust 1.95.0, and rebuilds the matched plugins using both repositories' lockfiles.
It does not replace the source in an existing developer checkout. Local path
dependencies still compile local edits; `Cargo.lock` does not pin their contents.
The general/release workflows must not be assumed to reproduce this source pair
merely because their Cargo commands use `--locked`.

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
| OpenTelemetry API / SDK | 0.33.1 | Existing production metrics; trace/testing for compatibility tests |
| OTLP exporter | 0.33.1 | Existing HTTP JSON metrics with blocking reqwest/rustls |
| tracing adapter | 0.34.0 | Core's optional context/completed-span adapter and tests |
| `metrics` facade | 0.24.6 | Core recorder and metric handles |
| Prometheus recorder | 0.17.2 | No default HTTP server/exporter features |
| `metrics-opentelemetry` | 0.24033.1 | Facade-to-OTel recorder, including absolute counters and mutable gauges |

Server's new recorder/tracing qualification dependencies are test-only. Core
keeps its OTel integration behind `otel-context`; ordinary logging remains
independent. Unrelated older OTel versions used transitively by other libraries
do not provide Server's telemetry types.

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
typed policy and its validation because Server does not yet accept the new
section.

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
The public Server builder API and complete shutdown wiring belong to OBS-03,
not this compatibility proof.

The managed-instance tracing proof explicitly installs a process-wide subscriber
composing component logging with the chosen OTel layer. Core's management
reconciliation task uses that process subscriber; a scoped subscriber around
only the calling future does not cover it. OBS-03 must qualify the embedding
subscriber strategy as well as provider ownership, rather than silently relying
on an unrelated global subscriber.

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
The CI job also runs the Server library unit suite and Core's observation,
graph-observability and query-metric regression suites.

For a quick, no-service check without rebuilding plugins:

```bash
CARGO_TARGET_DIR="$PWD/target/observability-server" cargo test --locked --test observability_compatibility
CARGO_TARGET_DIR="$PWD/target/observability-server" cargo test --locked --lib telemetry::tests
```

### Current local validation

All **six Server adapter/plugin proofs** pass against freshly rebuilt libraries
from the pinned Core revision, including the actual native-query handoff through
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
| New CI workflow syntax | `actionlint` passed |
| Adapter proof Clippy | Passed with only the two existing library lint categories allowed |

These are not full workspace or release qualification.

## Remaining completion gates

The integrated source now has an immutable revision and a focused CI setup.
Do not treat these as full release qualification:

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
- Run the pinned Observability Baseline workflow on a clean CI runner. Its
  source setup is explicit; existing release/cross-platform jobs still need
  their own matched-source qualification.
- `make clippy` still reports seven findings in untouched handlers/services:
  six `result_large_err` and one `too_many_arguments`. The focused adapter proof
  passes with only those categories allowed; this is not a clean result for the
  repository-wide strict lint gate.
- `cargo fmt -- --check` still reports formatting differences in untouched
  handlers, routing/authoring code and older integration fixtures. The changed
  adapter proof is formatted, and `git diff --check` passes. These existing
  differences are not reformatted as part of OBS-01.

Later issues own production configuration/runtime/backend wiring, inspection,
performance budgets and operational examples. No new per-instance telemetry
policy, management HTTP tracing or external-backend query proxy is introduced.
