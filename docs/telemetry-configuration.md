# Telemetry configuration (OBS-02)

Telemetry is a startup-only, process-wide Server policy. There are no
per-instance overrides or writable telemetry API. Change configuration and
restart to apply it. `logLevel` and `RUST_LOG` continue to control ordinary
logging independently of telemetry profiles.

## Availability

OBS-02 implements typed configuration, validation, reference-preserving
persistence and the existing `drasi-server init --output ...` wizard. It does
**not** install new subscribers, providers, exporters, scrape routes, inspection
routes or process collectors. These belong to OBS-03 through OBS-06.

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
provider or a replacement for component cleanup. OBS-02 validates this contract;
runtime deadline enforcement belongs to OBS-03.

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
