# Observability compatibility baseline (OBS-01)

[Issue #234](https://github.com/drasi-project/drasi-server/issues/234) establishes
the compatibility prerequisite for OBS-02 through OBS-08. It does not enable new
exporters, change startup logging, install process-global providers, or add
telemetry configuration/routes.

## Local source and plugin ownership

This branch consumes the existing sibling `../drasi-core` through Cargo path
dependencies, including uncommitted changes. The integration checkout is
`otel-conflict-resolution`: observability HEAD
`aed40c90aeaeccad962e9396b4c4f74030c422c5`, merged with graph/native-query revision
`4b8597440294f095f6a5b31f1ef8c9358e3629b3`.

Those two hashes identify the merge inputs, **not an immutable integrated
revision**. Before CI/release qualification, finish the Core validation, commit
the integration, and record its full resulting SHA. `Cargo.lock` does not pin
the contents of a path dependency. Do not fetch, switch or reset an existing
developer checkout during build preparation, or claim that a runner checking
out only Server can reproduce this source pair.

Ordinary plugins use ABI **0.18**; native computation plugins use base ABI **2.0**
and envelope wire format **2**. Telemetry extensions are separately negotiated.
Older base layouts cannot be made compatible by their telemetry extension
version. Rebuild plugins from the same Cargo-selected SDK/Host/FFI workspace:
path dependencies alone do not rebuild already installed shared libraries.
Registry plugin downloads are not a fallback for this local baseline.

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

## Running the checks

```bash
# Requires the matched sibling Core checkout and its locked graph.
# Reuse one target directory instead of maintaining two large build caches.
CARGO_TARGET_DIR="$PWD/target" make test-observability-baseline
```

The target rebuilds Core's context/telemetry fixtures before running its native
telemetry tests, rebuilds Server's local test bundle (including the native query
plugin), checks the locked Server library, reruns legacy loopback HTTP JSON
metrics and explicit query-factory selection tests, and runs all adapter proofs
including the cdylib test. It fails
explicitly if the Cargo-selected plugin source is not local.

For a quick, no-service check without rebuilding plugins:

```bash
cargo test --locked --test observability_compatibility
cargo test --locked --lib telemetry::tests
```

### Current local validation

The focused baseline target passed with rebuilt ordinary/native query libraries,
Core's native telemetry/context/logging and handled-latency suites (including
the no-default-feature metric path), legacy loopback HTTP JSON export, and all
five Server adapter/plugin proofs. The complete Server library unit suite now
passes **398 tests** after updating older fixtures to the explicit name/UUID,
deferred resource construction and accepted-configuration contracts. Core's
targeted observation/backpressure suite passes **78 tests**, with one local
timing characterization intentionally ignored. These results do not qualify
the unimplemented native query observation handoff or the broader integration
test matrix below.

## Remaining completion gates

The local merge is not a release-qualified integrated revision. Do not close
#234 merely because adapters or a query result work:

- Complete the native-query observer/phase/metric migration, including durable,
  bootstrap, scheduled and replay paths, and prove no host/plugin double counts.
  Query result parity alone does not establish evaluator-time metric parity.
- Run the integrated Core graph/native-query and affected OTel suites; the
  interrupted full Core unit run is not a passing result.
- Repair or establish baseline evidence for Server's older integration fixtures:
  the broad `cargo check --lib --tests --locked` currently reports outdated
  name/UUID, component-name, receipt and configuration-store APIs in existing
  native/reconstruction/init tests. These are not treated as passing or excluded
  silently by the focused target.
- Commit the tested Core integration and wire that exact source revision into
  reproducible CI setup before the existing locked release jobs can qualify it.
- The strict Clippy check currently reports large-error and argument-count
  findings in untouched Server handlers/services. A focused check of the new
  proof with those two existing lint categories allowed is not a clean result
  for the repository-wide strict lint gate.

Later issues own production configuration/runtime/backend wiring, inspection,
performance budgets and operational examples. No new per-instance telemetry
policy, management HTTP tracing or external-backend query proxy is introduced.
