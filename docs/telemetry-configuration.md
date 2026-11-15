# Telemetry configuration and runtime ownership (OBS-02/03)

Telemetry is a startup-only, Server-wide policy. There are no
per-instance overrides or writable telemetry API. Change configuration and
restart to apply it. `logLevel` and `RUST_LOG` continue to control ordinary
logging independently of telemetry profiles.

## Availability

OBS-02 implements typed configuration, validation, reference-preserving
persistence and the existing `drasi-server init --output ...` wizard. OBS-03
adds standalone logging composition, explicit borrowed bindings and retained
construction/running cleanup. It does
**not** install new subscribers, providers, exporters, scrape routes, inspection
routes or process collectors. Production tracing is OBS-04, production metrics
are OBS-05, and process/inspection support is OBS-06.

`drasi-server validate --config ...` checks the intended configuration and
reports unavailable runtime capabilities separately. Successful validation does
not mean this build can activate them. The wizard similarly validates output
and explains staged availability.

Normal startup rejects every effectively enabled, unavailable capability
before loading plugins or constructing components. It never silently accepts
an exporter or inspection request and does nothing. Omitted telemetry, an
empty section with no destinations, or Off with inspection disabled can start.
Off suppresses new tracing and metrics even if their destinations are retained.
Inspection remains independent; Off plus inspection still fails startup until
the inspection runtime is implemented.
Retained inactive destination settings are still validated; configuring them
never starts an additional backend.

## Defaults and compatibility

| Configuration | Behavior |
|---|---|
| Neither section | Existing logging, graph/log/SSE APIs and inspection behavior remain; no new exporters or listeners |
| Root `metrics` only | Existing query-progress OTLP/HTTP JSON export remains unchanged |
| `telemetry` only | Resolve the new policy below; check runtime readiness at startup |
| Both root `metrics` and any `telemetry` section | Reject with migration guidance, including `{}`, Off, tracing-only and inspection-only telemetry |

Omission and an empty section differ: omission preserves legacy defaults;
`telemetry: {}` selects Basic, but enables no trace export, metric collection
or inspection because it has no destinations and metrics defaults to None.

The legacy `metrics` contract remains HTTP/JSON with a full metrics URL,
10,000 ms default interval (100..=3,600,000) and 5,000 ms default timeout
(100..=30,000). `DrasiServer::with_metrics` and the
`drasi.query.progress.sequence` gauge are unchanged.

| New setting | Default | Validation |
|---|---|---|
| `profile` | `basic` | `off`, `basic`, `debug`, `persistence` |
| `inspection.enabled` | `false` | Independent of collection/profile |
| `tracing.endpoint` | absent | Empty resolved string also disables trace export |
| `tracing.protocol` | absent | Explicit `http/protobuf` required for a nonempty endpoint |
| `tracing.serviceName` | `drasi-server` | Nonempty, no control characters; shared OTLP resource identity, including metrics-only policy |
| `tracing.sampleRatio` | `1.0` | Finite number in [0, 1]; parent-based root sampling |
| `metrics.backend` | `none` | `none`, `prometheus`, `otlp`; exactly one active backend |
| `metrics.processMetrics` | `false` | Requires non-Off profile and an enabled backend |
| `metrics.prometheus.path` | `/metrics` | Absolute literal route, no trailing slash, parameters, escaping, query, fragment or dot segments |
| `metrics.prometheus.port` | absent | 1..=65,535; omitted shares management listener |
| `metrics.otlp.endpoint` | absent | Required when OTLP metrics is active |
| `metrics.otlp.protocol` | absent | Explicit `http/protobuf` |
| `metrics.otlp.exportInterval` | `30` seconds | Integer 1..=3,600 seconds |
| `shutdownTimeoutSeconds` | `5` seconds | Integer 1..=300 seconds |
| `filters.exclude` | `[]` | At most 64 literal metric-name substrings, 256 bytes each |

The shutdown setting is one shared telemetry drain-plus-flush budget, capped
by the remaining overall termination deadline. It is not a new budget per
provider or a replacement for component cleanup. OBS-03 enforces it for the
Server's owned collection/logging; borrowed collection is never flushed or
closed by Server.

Profiles reuse Core's catalogue rules: Debug adds Debug details to Basic;
Persistence adds supported storage/recovery details to Basic, not all Debug
details or every engine statistic. No profile enables handled-branch latency.
Exclusions only suppress metrics: `drasi.plugin.` is a substring; `*` is
literal, not a wildcard. An empty substring excludes every metric.

## Examples

These are valid policies, **not working exporter configurations in OBS-02**.

```yaml
telemetry:
  profile: "${DRASI_TELEMETRY_PROFILE:-basic}"
  inspection:
    enabled: false
  tracing:
    endpoint: "${DRASI_TRACE_ENDPOINT:-https://collector.example/v1/traces}"
    protocol: http/protobuf
    serviceName: drasi-server
    sampleRatio: 0.1
  metrics:
    backend: prometheus
    processMetrics: false
    prometheus:
      path: /metrics
  shutdownTimeoutSeconds: 5
```

Metrics-only policy does not enable tracing:

```yaml
telemetry:
  profile: persistence
  metrics:
    backend: otlp
    otlp:
      endpoint: https://collector.example/v1/metrics
      protocol: http/protobuf
      exportInterval: 30
      headers:
        authorization: "${DRASI_METRICS_AUTHORIZATION}"
```

Provide `DRASI_METRICS_AUTHORIZATION` through the deployment's environment
secret injection. Never put its resolved value or a plaintext fallback into
the configuration.

## Endpoints, credentials and access boundary

New OTLP supports **HTTP/protobuf only**. Specify a full signal-specific URL;
standard examples use port 4318 with `/v1/traces` or `/v1/metrics`. Custom
collector paths are allowed. No path is appended, protocol inferred from a
port, or gRPC service URL silently translated. gRPC, HTTP/JSON for new signals,
custom CA and mTLS settings are not supported. The existing legacy HTTP/JSON
exporter is separate.

Endpoints require HTTP(S), a host and a non-root path, without embedded
credentials, query strings, fragments or whitespace. Credential-free HTTP
is allowed for development. Configured headers require HTTPS with normal
certificate verification. Header values must be environment references without
plaintext defaults; HTTP routing/payload header overrides and invalid names or
values are rejected. Resolved destinations and credentials are omitted from
policy Debug output and validation diagnostics. Later exporter implementation
must preserve these transport/authentication rules and qualify the actual wire
behavior; OBS-01's in-memory trace proof was not that qualification.

Use the existing `${VAR}` / `${VAR:-default}` or structured `EnvironmentVariable`
references. Explicit mapper overrides take precedence over environment values,
which take precedence over defaults. The CLI loads the config directory's
`.env` without overriding existing environment values. No additional automatic
`OTEL_*` configuration precedence is introduced for the new policy.

Structured `Secret` references are retained by serialization, but resolving
them for early telemetry is explicitly unsupported: plugin secret providers
are constructed later. Use environment injection instead. Saves retain the
original references rather than replacing them with resolved environment values
or credentials. Saving unrelated API mutations does not re-resolve telemetry
against a changed environment or change its startup policy.

Inspection and Prometheus scraping add no authentication system. Their
supported deployment boundary is a trusted management proxy or restricted
network; CORS is **not** authentication. Inspection defaults disabled. A
separate scrape port is explicit opt-in, inherits the Server bind address and
must be protected separately: a proxy protecting the management port does not
protect another listener automatically.

On the shared listener, `/health`, the `/api` namespace, `/`, and the `/ui`
namespace when UI is enabled are reserved. Separate-listener port collisions
are checked against the final management port, including CLI overrides.
Configuration validation does not probe occupied operating-system ports or
instantiate opaque plugins to discover their listeners; actual binding must
report failures when those capabilities are implemented.

## Init

The existing wizard offers telemetry after Server settings. Skipping omits
the section. Off offers inspection but no exporter/process questions. Other
profiles offer optional tracing and one metrics backend, then only meaningful
path/port, OTLP interval and process questions. Sampling and shutdown keep the
documented defaults. Generated YAML passes the same policy validator as
startup; invalid selections or values fail rather than silently defaulting.

For a runnable, collector-free walkthrough, see the
[observability example](../examples/observability/README.md).

## OBS-03 ownership APIs

Standalone startup installs ordinary console logging, component capture and
the `log` bridge before plugin/component construction. `RUST_LOG` wins; absent
it, the fallback is `<logLevel>,oci_client=error`. Off is a collection policy,
not a log filter. Invalid filters and unrelated installed logger/subscriber
globals fail explicitly; Server never adopts an unknown global integration.
Global installation cannot be undone, so retry that installation in a fresh
process. No new global provider or metrics recorder is installed.

Embedding applications compose their own subscriber, logger, recorder/context
and optional native-span collector. Pass `TelemetryBindings` with
`DrasiServerBuilder::with_telemetry_bindings` or
`DrasiServer::prepare_new_with_telemetry`. One `MetricRegistrationContext` per
recorder can be shared by multiple Servers. Install the caller's composed
subscriber before construction: a scoped subscriber around only `build` does
not cover Core's real management/reconciliation background work.

```rust,no_run
use drasi_server::{DrasiServerBuilder, TelemetryBindings};

async fn embedded(bindings: TelemetryBindings) -> anyhow::Result<()> {
    let preparation = DrasiServerBuilder::new()
        .with_id("analytics")
        .with_telemetry_bindings(bindings)
        .prepare();
    let cleanup = preparation.cleanup_handle();
    let server = preparation.finish().await?;
    server.shutdown().await?; // Also valid before run().
    assert_eq!(cleanup.status().pending_workers, 0);
    // Caller collection is still open. Caller closes it after all users stop.
    Ok(())
}
```

`prepare` and `prepare_with_owned_handles` expose the cleanup handle before
awaiting construction; cancelling a waiter never discards accepted work.
After construction registers its result, admission is released independently
of that waiter. Keeping an unconsumed preparation or suspended `finish` does
not prevent shutdown from joining its workers and cleaning up its resources.
Publication is separately fenced: `finish` rejects after shutdown, including
when construction had already succeeded.
`prepare_with_owned_handles` returns started Core handles with a retained
`shutdown` owner; creation/start acknowledgements are not node readiness.
Configured owned telemetry cannot be returned bare through `build_core` or
the legacy two-field `build_with_handles` result. Borrowed bindings remain
supported there. `from_core_with_telemetry`/`from_cores_with_telemetry` check
already configured Cores, rather than replacing live cached instruments.
Opaque recorder/exporter compatibility remains the preconfiguring caller's
obligation. Configured YAML policy and borrowed policy are mutually exclusive.

`StandaloneTelemetry::prepare` provides a logs-only owner and
`cleanup_handle` before initialization. `DrasiServer::prepare_new_with_runtime`
transfers that actual owner, not just a clone of its bindings, into Server.

### One cleanup attempt, bounded waiting

`DrasiServer::run(self)` remains consuming. Keep `cleanup_handle()` before
calling it. `DrasiServer::shutdown(&self)` and `ServerCleanupHandle::request_shutdown`
address the same attempt as SIGINT/SIGTERM, run cancellation and startup failure.
First request closes admission/HTTP ingress and anchors one absolute 30-second
wait. Subsequent requests/waits do not start new teardown.

Accepted management/startup work (including unpublished instances) is joined.
The watcher consumer and actual polling backend stop; actual Core producer
retirement fences callbacks. HTTP graceful Serve retains all its connections:
its five-second cap is capped by the remaining overall deadline. If necessary,
Core shutdown proceeds after that cap, but Serve is still joined before
collection closes. Normal source/query/reaction log SSE bodies end when their
Server closes admission, without closing shared logging channels or another
Server's subscriptions. Direct routers without a Server lifecycle context
retain their channel-driven behavior. No abort/drop is treated as a connection
join. Owned legacy metrics then drain/close; owned logging drains and joins last.
The default shared collection allowance is five seconds, configurable 1..300,
always capped by that same attempt's remaining overall deadline.

`wait_until(deadline)` bounds the observer, not physical cleanup. On timeout
`ServerCleanupError` retains its typed causes and reachable handle. `status()`
distinguishes `Complete` with historical errors from pending workers and
`retry_required`. `wait_for_completion()` waits for accepted physical joins
without replenishing phase allowances; it returns at an explicit-retry boundary.
`retry_until` alone retries failed unfinished operations. Borrowed logging,
recorders, collectors and providers are never flushed or shut down.

The CLI reports incomplete cleanup immediately, retains Tokio/its handle until
accepted joins finish, then exits nonzero for any missed deadline/historical
failure. A failed unfinished phase requiring explicit retry remains retained and
reported, not busy-looped. This is not a hard process-exit guarantee: a permanently
stalled producer/connection can keep the process alive. SIGKILL cannot clean up.

### Polling and legacy reader boundaries

Server hot reload uses the additive Host SDK `prepare_owned_polling` handoff,
even when native watcher features are enabled. The initial directory snapshot
emits nothing; once per configured debounce/scan interval, size/mtime changes
produce Added/Changed/Removed events for plugin binaries. Rapid edits coalesce
to that snapshot; unchanged-size rebuilds are detected by mtime. Failed scans
report typed errors, never fabricated removals. Scanning runs off Tokio and the
actual loop is stop/join-owned. Server creates no notify/debouncer backend.
The owned handle is single-use: stop intent rejects new starts, and completed
cleanup (even before the first start) returns a typed finalized-handle error.
Pending submission cannot publish completion; cancelled/expired joins retain
the original worker.
Legacy SDK `start`/`start_polling`/`stop` callers retain their old signatures;
this does not claim their opaque native threads are joined.

Legacy root `metrics` now uses the SDK's existing
`experimental_metrics_custom_reader`/`ManualReader` API and one application-owned
blocking loop, not SDK `PeriodicReader`'s opaque thread. The same provider,
resource/service identity, callback, scope, gauge, labels, signed-range guard,
HTTP/JSON exporter, rustls verification and header-environment behavior remain.
Interval and timeout remain milliseconds; exporter temporality and serial
interval scheduling match legacy behavior. Cancellation/timeout retain the real
worker; one actual SDK/exporter shutdown runs there, with all network/SDK/Drop
work off the executor. No fresh final collection/flush starts after phase expiry;
already accepted work and safe close/join remain owned and expiry is an error.
