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
spans, sharing the new OTLP metrics resource identity. OBS-06 adds optional
[process metrics](#process-metrics) and [read-only inspection](#read-only-inspection)
routes. Native-plugin
wire parity and the end-to-end observability demo require separate qualification;
the [example](../examples/observability/README.md) provides explicit live
pipelines (an ordinary HTTP source, a native query and a log reaction, plus a
PostgreSQL CDC variant) alongside its unchanged Off/empty default.
Recorded evidence and deferred gates are in the baseline
([OBS-05](observability-baseline.md#obs-05-delivery-status),
[OBS-06](observability-baseline.md#obs-06-delivery-status)).

`drasi-server validate --config ...` checks the intended configuration and
reports unavailable runtime capabilities separately. Every telemetry capability
is available in this build, so it reports `No unavailable telemetry capabilities
requested`. It prints this Telemetry section for configurations that omit
`telemetry` too, because they resolve to the default policy; legacy
root-`metrics` configurations are the exception. Listener requirements, such as
explicitly enabled inspection needing the management API, are checked at
startup. The wizard validates its output the same way.

Normal startup checks telemetry before loading plugins or constructing
components: an invalid policy, an unmet listener requirement or an
[instance ID](#instance-ids) that active telemetry cannot scope fails explicitly.
Omitted telemetry, an empty section with no destinations, or Off can start.
Off suppresses new tracing and metrics even if their destinations are retained.
Inspection remains independent: Off with default inspection serves the settings
route, which reports profile `off`, backend `none` and `path: null`; with
explicit `inspection.enabled: true` its metrics route returns 503.
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
| Neither section | Same as `telemetry: {}`: profile Basic, no trace export, metrics backend `none`, and the read-only settings route `GET /api/v1/telemetry` on the management listener; no exporters or new listeners |
| Root `metrics` only | Legacy behavior, unchanged: Server telemetry Off, the existing query-progress OTLP/HTTP JSON export and no inspection; startup logs one info line suggesting migration to `telemetry` |
| `telemetry` only | Resolve the new policy below; check runtime readiness at startup |
| Both root `metrics` and any `telemetry` section | Reject with migration guidance, including `{}`, Off, tracing-only and inspection-only telemetry |

Omitting the section means exactly what `telemetry: {}` means. Both select
Basic, but send nothing anywhere: there is no trace endpoint and metrics
defaults to `none`, so no trace export or metric collection runs. Sending data
is opt-in through a destination, and `profile: off` is the explicit opt-out.
The only default surface is the read-only settings route
([inspection](#read-only-inspection)). Saving never writes `telemetry: {}` or a
defaulted `inspection.enabled` into the file. Console log lines are unchanged:
Basic adds no span prefixes ([console logging](#obs-03-ownership-apis)).

Programmatic Servers follow the same default:

- `DrasiServerBuilder` without `with_telemetry` or `with_telemetry_bindings`
  still borrows Core's default logging service, but applies the default policy
  (Basic, no destinations) to every Core it builds and to instances created
  through the API. This replaces telemetry options set on a `DrasiLibBuilder`
  passed to `add_instance_builder`; use `with_telemetry_bindings` for custom
  telemetry. With the API enabled, `build()` and `prepare()` mount the
  settings route.
- `DrasiServer::from_core` and `from_cores` own none of their cores'
  telemetry. Instances created through the API follow DrasiLib's own default
  (Basic, with metrics forwarded to the current `metrics` recorder), and no
  inspection route is mounted.

The legacy `metrics` contract remains HTTP/JSON with a full metrics URL,
10,000 ms default interval (100..=3,600,000) and 5,000 ms default timeout
(100..=30,000). `DrasiServer::with_metrics` and the
`drasi.query.progress.sequence` gauge are unchanged. `with_metrics` is allowed
when telemetry was omitted and rejected with explicit telemetry or borrowed
bindings. Cores already built with the default keep it; their metrics are off,
so nothing is exported twice.

| New setting | Default | Validation |
|---|---|---|
| `profile` | `basic` | `off`, `basic`, `debug`, `persistence` |
| `inspection.enabled` | unset: settings route only | Independent of collection/profile. `true` adds the metrics route and requires the management API listener and `build()`/`run()`; `false` mounts neither route ([inspection](#read-only-inspection)) |
| `tracing.endpoint` | absent | Empty resolved string also disables trace export |
| `tracing.protocol` | absent | Explicit `http/protobuf` required for a nonempty endpoint |
| `tracing.serviceName` | `drasi-server` | Nonempty, no control characters; shared OTLP resource identity, including metrics-only policy |
| `tracing.sampleRatio` | `1.0` | Finite number in [0, 1]; parent-based root sampling |
| `metrics.backend` | `none` | `none`, `prometheus`, `otlp`; exactly one active backend |
| `metrics.processMetrics` | `false` | Requires non-Off profile and an enabled backend ([process metrics](#process-metrics)) |
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

### Instance IDs

While trace export or a metrics backend is active (a trace endpoint or a
`prometheus`/`otlp` backend is configured and the profile is not `off`), every
instance ID must be nonempty and contain no whitespace or control characters.
This is Core's telemetry scope rule, checked early:

- `drasi-server validate` and startup check every configured instance before
  loading plugins or constructing components. The error names the instance,
  quoted with control characters escaped, and suggests renaming it or setting
  `telemetry.profile: off`.
- `POST /api/v1/instances` rejects such an ID with 400 `INVALID_REQUEST`; the
  ID is in `details.component_id`.
- `DrasiServerBuilder` relies on Core's build error, which fires under the same
  condition. Opt out with `with_telemetry_profile(TelemetryProfile::Off)`, or,
  for `build_core` and `build_with_handles`, with `with_telemetry_bindings`
  carrying Off settings.
- In a `DrasiServer::from_core`/`from_cores` Server, instances created through
  the API use DrasiLib's default, which enables metrics, so an invalid ID gets
  Core's build error as `INSTANCE_CREATE_FAILED`, with the ID and guidance in
  its technical details.

The default (no destinations), `profile: off` and legacy root `metrics` do not
check IDs, so existing configurations keep starting until a destination is
added.

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
Names are not substituted for assigned component UUIDs; they are published
alongside them as separate info series (see
[component names and queue edges](#component-names-and-queue-edges)). Physical queue identity
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

### Component names and queue edges

Two Basic gauges describe the graph so that metrics can be labelled by name.
No other series changes:

| Series | Labels besides `instance_id`, `graph_id` | Value |
|---|---|---|
| `drasi.component.info` | `component_id`, `component_kind`, `component_name` | 1 for the component's current name; 0 for a name it no longer has after a rename or removal |
| `drasi.queue.edge_info` | `queue_id`, `from_component_id`, `to_component_id` | 1 for each graph edge bound to the physical queue; 0 once that edge or the queue's binding is gone |

The recorder cannot delete a series, so retired entries stay at 0; select
current ones with `== 1`. Each new name a component takes keeps its own series,
so a rename to a new name permanently uses one series of the 16,384 budget
(renaming back reuses it). Both series are metadata: when one cannot be
registered (budget exhausted, a name longer than the 1,024-byte label-value
limit, or a rejected registration), a warning is logged once per component or
edge, and the rename, addition or data flow continues without that series. OTLP
exports both as gauges with the same attributes.

A query's `query_id` is its component ID, and a query's input queue is the
queue whose edge ends at the query (`to_component_id`); its output queue is the
one whose edge starts there (`from_component_id`). Names are unique only within
a graph, and queue IDs only within an instance's graph, so carry `instance_id`
and `graph_id` through every join. In PromQL:

```promql
# Query events per second, by query name
sum by (instance_id, graph_id, component_name) (
  rate(drasi_query_events_processed[1m])
  * on (instance_id, graph_id, query_id) group_left (component_name)
    label_replace(
      max by (instance_id, graph_id, component_id, component_name) (
        drasi_component_info{component_kind="query"} == 1),
      "query_id", "$1", "component_id", "(.*)")
)

# Items waiting in each query's input queue, by query name
max by (instance_id, graph_id, component_name) (
  max by (instance_id, graph_id, queue_id, component_name) (
    label_replace(drasi_queue_edge_info == 1, "component_id", "$1", "to_component_id", "(.*)")
    * on (instance_id, graph_id, component_id) group_left (component_name)
      max by (instance_id, graph_id, component_id, component_name) (
        drasi_component_info{component_kind="query"} == 1)
  )
  * on (instance_id, graph_id, queue_id) group_left drasi_queue_depth
)
```

The edge side of that join comes first because one shared queue can feed more
than one query; there is only one depth series per queue.

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

The facade adapter is the published `metrics-opentelemetry` 0.24033.1, which
identifies cached series by the `metrics` key hash alone. That hash does not
mix in the length of label values of 1–3 bytes, so with the OTLP backend two
otherwise identical series whose differing value shares its first, middle and
last byte (for example instance IDs `a` and `aa`, or plugin label values `1`
and `11`) are exported as one series. Use label values, including instance IDs,
of four or more characters. UUID component IDs and Core's fixed label values
are not affected, and the Prometheus backend compares full keys.

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
network; CORS is **not** authentication. The inspection settings route is on
by default and returns no destinations or credentials; the inspection metrics
route requires explicit `inspection.enabled: true`. A
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
With `inspection.enabled: true`, the management listener also serves the same
render at `/api/v1/telemetry/metrics` ([inspection](#read-only-inspection)).
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

## Process metrics

`metrics.processMetrics: true` adds seven unlabelled measurements of the whole
Server process to the active metrics backend. It requires a non-Off profile and
the `prometheus` or `otlp` backend. Both backends use these exact names:

| Name | Prometheus type | OTLP instrument | Unit | Linux | macOS | Windows | FreeBSD |
|---|---|---|---|---|---|---|---|
| `process_cpu_seconds_total` | counter (fractional) | f64 observable counter, monotonic | `s` | x | x | x | x |
| `process_resident_memory_bytes` | gauge | observable gauge | `By` | x | x | x | x |
| `process_virtual_memory_bytes` | gauge | observable gauge | `By` | x | x | x | x |
| `process_open_fds` | gauge | observable gauge | `{file_descriptor}` | x | x | x | x |
| `process_max_fds` | gauge | observable gauge | `{file_descriptor}` | x | x | x | x |
| `process_threads` | gauge | observable gauge | `{thread}` | x | x |  | x |
| `process_start_time_seconds` | gauge | observable gauge | `s` | x | x | x | x |

Platform columns follow `metrics-process` 2.4.3; OpenBSD is not a qualified
target. `process_virtual_memory_max_bytes` and `process_heap_bytes` are not
emitted. CPU usage, in cores, is `rate(process_cpu_seconds_total[1m])`.
The count gauges use curly-brace annotation units rather than `1`, so
OTLP-to-Prometheus translation (for example a Collector's Prometheus exporter)
keeps their names instead of appending `_ratio`.

- A measurement the platform cannot provide, or a reading that fails, is
  omitted from that scrape or export. It is never published as `0` or as a
  stale earlier value. Non-finite readings are also omitted.
- `process_max_fds` is omitted when the soft limit is unlimited; that is a
  valid reading, not a missing one. Windows has no per-process limit and the
  crate reports a fixed 16,777,216 there: treat it as a platform constant.
- The series describe the OS process. There is no per-instance, graph,
  component or plugin attribution; several instances in one Server share one set.
- `filters.exclude` applies to these names. `process_` excludes all of them,
  and then no snapshot is taken.
- When an expected measurement is missing, a warning names the metrics only:
  immediately, then at most once per 300 seconds with a suppressed count, plus
  an info line on recovery. Collection never fails a scrape, an export or the
  Drasi metrics.

Prometheus takes one snapshot per render and appends the process families after
the Drasi families. Every scrape is current: the shared route, a separate
`prometheus.port` listener and the inspection metrics route all use the same
render job, admission, wait and content type. OTLP registers observable
instruments under meter scope `drasi.server.process` on the existing provider,
so they share the resource (`service.name`, `service.instance.id` and the rest)
of the Drasi metrics. One snapshot is taken immediately before each export
collection, including the final one; there is no separate timer or task.

Exactly one set exists per OS process. Process metrics require configured
telemetry, which owns process-global logging, so a second configured-telemetry
Server in the same process fails startup with `a host logger is already
installed`. Borrowed `TelemetryBindings` have no process metrics.

## Read-only inspection

Inspection serves read-only routes on the management listener only, never on
a separate Prometheus listener. It is independent of the profile and backend,
so `profile: off` still serves the settings route, which reports `off`.

| `inspection.enabled` | Routes |
|---|---|
| unset (default) | `GET /api/v1/telemetry` only; it reports `metrics.path: null` |
| `true` | `GET /api/v1/telemetry` and `GET /api/v1/telemetry/metrics` |
| `false` | none |

An environment reference such as `"${VAR:-false}"` counts as an explicit value.
The default applies wherever Server owns telemetry: an omitted section,
`telemetry: {}`, any section that leaves `inspection.enabled` unset, and
`DrasiServerBuilder` without `with_telemetry_bindings`. It is skipped quietly
where no management listener exists: when the management API is disabled, with
the owned-handles builder result, or with `build_core`. Explicit `true` instead
fails startup when the management API is disabled or with the owned-handles
result: `telemetry.inspection.enabled requires the management API listener;
enable the API and use build() and run(), or disable inspection`.
Borrowed `TelemetryBindings`, the legacy root-`metrics` case and
`DrasiServer::from_core`/`from_cores` mount nothing; Server cannot accurately
report settings it does not own.

The settings route returns a bare camelCase object, not the `ApiResponse`
wrapper. It is computed once at startup, because telemetry settings are
startup-only. For example, with `inspection.enabled: true` and the Prometheus
backend:

```json
{
  "profile": "basic",
  "tracing": { "exportEnabled": true, "sampleRatio": 1.0, "localInspection": false },
  "metrics": { "backend": "prometheus", "processMetrics": true, "path": "/api/v1/telemetry/metrics" },
  "logs": {
    "pathTemplate": "/api/v1/instances/{instanceId}/{collection}/{componentId}/logs",
    "streamPathTemplate": "/api/v1/instances/{instanceId}/{collection}/{componentId}/logs/stream"
  }
}
```

| Field | Meaning |
|---|---|
| `profile` | `off`, `basic`, `debug` or `persistence` |
| `tracing.exportEnabled` | A trace destination is configured and the profile is not `off`; not collector health |
| `tracing.sampleRatio` | Configured root sampling ratio in [0, 1] |
| `tracing.localInspection` | Always `false`; no trace history is stored |
| `metrics.backend` | Effective `none`, `prometheus` or `otlp` (`none` under `off`) |
| `metrics.processMetrics` | Effective setting; per-OS availability is documented above, not returned |
| `metrics.path` | `"/api/v1/telemetry/metrics"` when `inspection.enabled` is `true` and the Prometheus backend is active, otherwise `null` |
| `logs.pathTemplate`, `logs.streamPathTemplate` | Constant templates for the existing log routes; `{collection}` is `sources`, `queries` or `reactions` |

All fields are required; only `metrics.path` is nullable. Endpoint URLs, hosts,
header names or values, credentials, raw YAML and environment values are never
returned.

The metrics route delegates to the Prometheus scrape job. It returns the same
text and content type, including process families, and shares the four-render
admission and five-second wait with the scrape route, even when that uses a
separate port. It never installs a backend to manufacture data.

| Situation | `/api/v1/telemetry` | `/api/v1/telemetry/metrics` |
|---|---|---|
| `inspection.enabled: false`, legacy root `metrics`, or borrowed `TelemetryBindings` | 404 | 404 |
| `inspection.enabled` unset (default) | 200, `path: null` | 404 |
| `inspection.enabled: true`, Prometheus backend active | 200 | 200, Prometheus text |
| `inspection.enabled: true`, OTLP, `none` or `off` | 200, `path: null` | 503 `TELEMETRY_METRICS_UNAVAILABLE` |
| Four renders in flight or five-second wait elapsed | 200 | 503 `TELEMETRY_METRICS_UNAVAILABLE` |
| Render failure | 200 | 500 `TELEMETRY_INSPECTION_FAILED` |
| Server shutting down | 503 `SERVER_SHUTTING_DOWN` | 503 `SERVER_SHUTTING_DOWN` |
| POST, PUT, PATCH or DELETE | 405 | 405 |

Under the default the metrics route is not mounted, so a request to it gets the
router's ordinary 404, as when inspection is disabled; the remaining rows apply
only to mounted routes.

A metrics read admitted just before shutdown may find the scrape runtime
already closing; it then receives 503 `TELEMETRY_METRICS_UNAVAILABLE` rather
than `SERVER_SHUTTING_DOWN`.

The 503 and 500 responses use the standard `ErrorResponse` body; 404 and 405
have empty bodies, and 405 includes an `Allow` header. Access equals
reachability of the management listener (the OBS-02 decision): there is no
authentication and the API's CORS policy applies. Restrict it with a trusted
proxy or restricted network. **Setting `inspection.enabled: true` exposes
Prometheus metrics on the management port even when `/metrics` uses a separate
`prometheus.port`**; deployments requiring strict metrics-port isolation must
not set it. The default settings route exposes no metrics.
Inspection stores no trace history, queries no external metrics or trace
backend, and never changes policy or YAML.

## Init

The existing wizard offers telemetry after Server settings. Skipping omits
the section, which selects the default: Basic, no destinations and the settings
route. A chosen profile is followed by a three-way inspection choice: settings
only (the default, which leaves `inspection.enabled` unset), settings and
metrics (`true`) or none (`false`). Off asks no exporter/process questions.
Other profiles offer optional tracing and one metrics backend, then only
meaningful path/port, OTLP interval and process questions. Sampling and
shutdown keep the documented defaults. Generated YAML passes the same policy
validator as startup; invalid selections or values fail rather than silently
defaulting. Because the settings route is on by default, generated YAML always
includes the tip to protect inspection and scrape listeners with a trusted
proxy or restricted network.

For a runnable, collector-free walkthrough, see the
[observability example](../examples/observability/README.md).

## OBS-03 ownership APIs

Standalone startup installs ordinary console logging, component capture and
the `log` bridge before plugin/component construction. `RUST_LOG` wins; absent
it, the fallback is `<logLevel>,oci_client=error`. Off is a collection policy,
not a log filter. Invalid filters and unrelated installed logger/subscriber
globals fail explicitly; Server never adopts an unknown global integration.
Global installation cannot be undone, so retry that installation in a fresh
process. No new global provider or metrics recorder is installed. The console
layer, like Core's default one, hides Drasi telemetry spans (target
`drasi::telemetry`), so telemetry adds no span prefixes to log lines; log
events inside those spans still print.

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
`cleanup_handle` before initialization. Its bindings carry the default policy
(Basic, no exporters), not Off. `DrasiServer::prepare_new_with_runtime`
transfers that actual owner, not just a clone of its bindings, into Server.
With legacy root `metrics` and no `telemetry` section, Server sets that owner's
policy to Off before constructing any Core.

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
