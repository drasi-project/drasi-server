# Telemetry configuration and runtime ownership

Telemetry is a startup-only, Server-wide policy. There are no
per-instance overrides or writable telemetry API. Change configuration and
restart to apply it. `logLevel` and `RUST_LOG` continue to control ordinary
logging independently of telemetry profiles.

## Availability

OBS-02 implements typed configuration, validation, reference-preserving
persistence and the existing `drasi-server init --output ...` wizard. OBS-03
adds standalone logging composition, explicit borrowed bindings and retained
construction/running cleanup. Configured OTLP metrics now use one explicit
recorder/context and retained HTTP/protobuf exporter. Prometheus uses one
guarded recorder/context with controlled scrape routing and retained upkeep.
Server installs no global metrics recorder or provider. Configured tracing uses
one owned HTTP/protobuf exporter for host operations and Core's completed native
spans, sharing the new OTLP metrics resource identity. Process collectors and
inspection remain unavailable. Native-plugin
wire parity and the end-to-end observability demo require separate qualification;
the [example](../examples/observability/README.md) provides explicit live
pipelines (an ordinary HTTP source, a native query and a log reaction, plus a
PostgreSQL CDC variant) alongside its unchanged Off/empty default.
Recorded evidence and deferred gates are in the
[baseline](observability-baseline.md#obs-05-delivery-status).

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

Tracing retains at most 2,048 completed spans and 16 MiB of accounted span data,
including in-flight exports, after Core's separately bounded native handoff.
Exports contain at most 256 spans, run every second or when a batch is ready,
and use a five-second request timeout capped by the remaining shutdown deadline.
Full/oversized records and failed batches are dropped with loss diagnostics;
there is no automatic retry or durable trace storage. Admission losses (full,
oversized, closed) and export failures/rejections are each warned at most once
per 30 seconds, with counts only, and appear in the shutdown summary. Trace
shutdown reports failure only for spans lost after it began: a failed or
rejected final export, or spans still pending when the shutdown deadline
expires (reported separately). Earlier losses and warning-only collector
responses remain in the counters and summary. These bounds are not a
hard RSS guarantee. Trace sampling does not suppress metrics or ordinary logs.
Prepared and failed/never-run Servers send no trace exports; activation follows
the same serving/publication boundary as metrics. Shared collection remains
open when an individual instance stops. Native/plugin qualification and the
deferred query-observation blocker are recorded separately in the baseline.

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
| `metrics.otlp.exportTimeout` | `5` seconds | Integer 1..=30 seconds |
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

## Metric data and bounds

Core metrics use scope `drasi.core`. Counters are cumulative monotonic sums,
gauges retain their value semantics, and histograms retain native units (`1`
for counts, `s` for durations). Values are not multiplied into milliseconds.
Both backends use these finite histogram boundaries (Prometheus also has `+Inf`):

```text
[0.000001, 0.000005, 0.00001, 0.00005, 0.0001, 0.0005, 0.001,
 0.005, 0.01, 0.05, 0.1, 0.5, 1, 5, 10, 100, 1000]
```

For example `drasi.query.events_processed` and `drasi.query.results_emitted`
are counters; `drasi.queue.depth` is a gauge; `drasi.query.engine.duration`
and Persistence's `drasi.query.commit.duration` are second-valued histograms.
Prometheus normalizes dots to underscores without adding `_total` or unit
suffixes; histograms generate `_bucket`, `_sum`, `_count`. OTLP preserves dotted
names and units with non-cumulative explicit bucket counts within each cumulative
histogram point. Commit counts include actual evaluation/publication and delivery
acknowledgement commits, not a fabricated one-commit-per-input invariant.

All actual instances/graphs/static/native producers share one recorder/context.
Core attributes include actual `instance_id`, `graph_id`, `component_id`,
`component_kind` and catalogue-specific labels such as `query_id` / `queue_id`.
Names are not substituted for assigned component UUIDs. Physical queue identity
is provider-owned; unsupported observations are omitted rather than synthesized.
Generic component and specialized counters are different views, not additive
totals. No metric exports query text, record payloads or credentials.

The shared Core context limits definitions to **1,024** and series to **16,384**
with its existing name/description/label/byte limits. New OTLP Core streams honor
that limit rather than the SDK's smaller default. Prometheus rejects normalized
family/generated-histogram-name or label-key collisions transactionally before
recorder registration, through the same optional context guard used by native
bindings. OTLP's unguarded names remain unchanged. These are admission limits,
**not a hard RSS or throughput guarantee**.

### Same-provider progress migration

New OTLP registers the existing `drasi.query.progress.sequence` observable gauge
once, under scope `drasi.progress`, on the same provider as Core metrics.
Do not add legacy root `metrics` to get progress; the combination is rejected.
Progress retains its existing labels, stages (`input.committed` versus
`output.produced`), generation/operation/configuration-hash identity, signed-range
protection and unsupported/unready-query omission. Input which changes no result
can advance committed input without advancing produced output. A collector's old
sample is not evidence that a stopped query was observed again. Prometheus has
no progress callback. Exclude `drasi.query.progress.` to omit it from new OTLP.

Both OTLP scopes share an explicit resource: configured service name (default
`drasi-server`), Server version, PID, and one runtime/process-start-unique
`service.instance.id`. All instances share that resource but retain their own
graph/component attributes. A restart gets a new identity; no automatic
`OTEL_RESOURCE_ATTRIBUTES`/`OTEL_SERVICE_NAME` merge occurs. Prometheus retains
Core identity labels; it does not invent an OTLP progress or resource series.

The facade adapter has an approved same-version local patch:
[`metrics-opentelemetry` provenance](../patches/metrics-opentelemetry/README.md).
It preserves full metric-key equality when cached hashes collide, for counters,
gauges and histograms. Original upstream BSL-1.0 license/notices and commit
`12be6ec64d25aa99992c750c488338c68e65b281` are retained; package version
0.24033.1 and instrument/label semantics are unchanged. This is not a limit
increase, second recorder or shared Cargo-cache edit.

## Examples

This policy serves Prometheus on the management listener without enabling
tracing:

```yaml
telemetry:
  profile: "${DRASI_TELEMETRY_PROFILE:-basic}"
  inspection:
    enabled: false
  tracing:
    serviceName: drasi-server
  metrics:
    backend: prometheus
    processMetrics: false
    prometheus:
      path: /metrics
  shutdownTimeoutSeconds: 5
```

Configured OTLP metrics do not enable tracing:

```yaml
telemetry:
  profile: persistence
  metrics:
    backend: otlp
    otlp:
      endpoint: https://collector.example/v1/metrics
      protocol: http/protobuf
      exportInterval: 30
      exportTimeout: 5
      headers:
        authorization: "${DRASI_METRICS_AUTHORIZATION}"
```

Provide `DRASI_METRICS_AUTHORIZATION` through the deployment's environment
secret injection. Never put its resolved value or a plaintext fallback into
the configuration.

### Preparation, activation and ownership

Configured file startup (including the CLI's already initialized standalone
logger) and the programmatic full configuration prepare collection before
constructing any Core. Primary, additional and API-created instances share
that context through the actual instance registry, with their own instance
and graph identities. No second metrics instance map is created.

`build()`/`prepare().finish()` retain observations without exporting or binding
listeners until `DrasiServer::run()` starts serving. All required management
and scrape sockets must bind before serving or metric export activation; a
bind failure rolls back the same retained owner and releases partial bindings.
`build_with_owned_handles()` activates after Core startup/publication and before
returning. With Prometheus this result requires an explicit separate scrape
port, which serves before publication even when the management API is disabled.
It never starts a management API. Shared scraping requires an API-enabled
`build()`/`run()` result; incompatible result/API combinations reject before
factories. Start still acknowledges construction/startup, not component
readiness. Failed or never-run preparation closes its backend without an export
request. Active shutdown has one bounded
telemetry drain/final-export budget; cancellation never discards the worker.

Configured telemetry and borrowed `TelemetryBindings` are mutually exclusive.
Borrowed recorders/providers remain caller-owned and are never flushed or
closed by Server. An unconsumed result or caller may retain a closed Core
wrapper after cleanup: successful Core shutdown/resource disposal, not the
last `Arc` disappearing, is the cleanup-completion boundary.

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
policy Debug output and validation diagnostics. The new metrics HTTP seam sends
the resolved URL/headers explicitly; it does not accept conflicting SDK header
or endpoint environment overrides. Actual TLS/auth boundary proof is a focused
test with a test-only trust anchor, not a production custom-CA feature or evidence
of a qualified external collector. See the baseline for its execution status;
OBS-01's in-memory trace proof was not transport qualification.

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
report failures at startup.

### Prometheus HTTP contract

Only the configured literal path is registered (default `/metrics`); a custom
path does not also register `/metrics`. Omitted `port` merges that single route
into the management router. An explicit port serves **only** the scrape router:
no health, management API, UI or authentication routes are copied to it.
Off/None creates no scrape route, separate listener or upkeep worker.

A successful GET returns `200` with
`Content-Type: text/plain; version=0.0.4; charset=utf-8`, including for an empty
Server. Counter values and histogram observations retain their native units;
unit suffixes are disabled and normalized metric names are unchanged.
At most four renders are admitted with no waiting queue. Overload, quiescing,
or expiration of the five-second response wait returns `503` with the standard
JSON `ErrorResponse` code `TELEMETRY_SCRAPE_UNAVAILABLE`. Render failure returns
`500` with `TELEMETRY_RENDER_FAILED`; internal errors are not exposed in the
message. Cancellation or timeout does not free a render slot until its actual
off-executor work is joined.

Shutdown closes admission on both listeners and on scraping together. Both
listeners share the original five-second API drain cap, not five seconds each;
actual connections and accepted renders remain owned until joined. Upkeep
continues while producers stop, then joins in the shared collection phase.
A fatal serving error requests the same sticky cleanup operation and remains
a failed Server outcome. No final scrape is required or promised.

Upkeep drains histogram samples every five seconds even without a scraper.
There is never overlapping upkeep or a pending upkeep backlog. Rendering and
upkeep run in retained off-executor workers, not `block_in_place`. Aggregation
and bounded registration do not establish a hard memory cap under sustained
producer load.

### OTLP scheduling and failure behavior

New OTLP constructs one SDK provider/ManualReader and one retained blocking
collection/export loop. Preparation sends nothing; activation follows actual
startup publication and all required listener binds. A never-run or failed
preparation closes without exporting. Interval/timeout are explicitly resolved
seconds, not SDK environment defaults. One export runs at a time; missed
interval ticks are skipped, with no pending snapshot backlog. Transport work
does not run on the Tokio executor.

Export failures are reported at most once per 30 seconds, with suppressed-count
and recovery reporting; an unavailable receiver does not stop actual source/query
processing. After activation, producer stop precedes one bounded final export/
close. Cancellation or an expired wait retains actual workers, one physical
close and historical deadline errors; it neither claims completion nor grants
a new final-export budget. Borrowed providers remain caller-owned.

### Incoming trace context and the source-data proxy

Exporting to an OTLP endpoint does not propagate incoming context. Supported
native HTTP/gRPC sources extract W3C `traceparent`/`tracestate` at ingress and
carry it as transient execution context, never in persisted events.

`POST /api/v1/instances/{instanceId}/sources/{id}/push` remains a JSON proxy.
It forwards only a validated `traceparent` and `tracestate`, regardless of
telemetry policy, and never baggage, credentials or other caller headers. It
starts no Server span. `traceparent` is limited to 512 bytes; repeated
`tracestate` fields are joined in wire order and limited to 512 bytes and 32
members. A missing parent forwards no context; `tracestate` without a parent is
dropped silently, matching Core source ingress. Duplicate, malformed or
oversized metadata is dropped with a rate-limited diagnostic, and
the data is still forwarded. The proxy does not follow redirects.

The proxy forwards to the source's assigned ID. It reads the source's reported
`host`, `port` and `endpoint`; static values and environment references
(`${VAR:-default}` or structured `EnvironmentVariable`) resolve from the Server
process environment. Secret references are never resolved by the proxy, and
an unresolvable or invalid configured value fails with `Source proxy target
unavailable` rather than falling back to the default port.

Ordinary source plugins without negotiated portable context can receive these
headers, but cannot continue the trace across their plugin boundary. Do not
assume end-to-end propagation through them. The JSON proxy does not adapt HTTP
input to gRPC sources.

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
