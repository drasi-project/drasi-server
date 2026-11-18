# Observability: Drasi Server traces and metrics

## Showcase: one command, three links

The showcase shows two things with real tools running in podman:

1. **Tracing**: follow each change through Server in Jaeger (and in Grafana).
2. **Metrics**: Drasi pipeline metrics next to Server's whole-process
   (system) metrics, scraped by Prometheus and graphed in Grafana.

It runs the HTTP source → native query → log reaction pipeline from
[`live`](#2-run-the-http-pipeline-send-input-and-scrape) and sends it one
change about every 2 seconds, so graphs and traces move without another step.

You need podman (on macOS, a running `podman machine`), Python 3.11+, curl, and
the Rust toolchain and sibling `../drasi-core` checkout described under
[Advanced](#advanced). Build the matched local bundle once, and again after any
source change, then start the showcase from the repository root:

```sh
bash examples/observability/demo.sh build-plugins
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 bash examples/observability/demo.sh showcase
```

The first start pulls three pinned images. When it prints `Demo is live`, open
the printed links:

| Link (defaults) | What to look at |
|---|---|
| **Grafana** `http://127.0.0.1:3000/d/drasi-server-observability?var-trace_id=…` | The **Drasi Server observability** dashboard (also Grafana's home page), refreshed every 5 s over the last 15 minutes. The pipeline row shows changes flowing through source, query and reaction; the Server process row shows CPU, memory, threads, file descriptors and uptime; the Traces row opens one real input trace. |
| **Jaeger** `http://127.0.0.1:16686/search?service=drasi-server-observability` | One trace per change: `source.publish` → `query.process` → `sink.handle`, plus the Debug profile's query phases (`query.evaluate`, `query.commit`, …), with `instance_id`, `graph_id` and `component_id` attributes. |
| **Prometheus** `http://127.0.0.1:9090/targets` | The `drasi-server` target is **UP**; query any family below, for example `rate(process_cpu_seconds_total[1m]) * 100`. |

Send a burst from another terminal and watch the rates jump:

```sh
bash examples/observability/demo.sh send 20     # N changes; default 5, at most 1000
```

Press Ctrl+C to stop. The launcher stops, in order, the input generator, then
Server, then removes only its own containers (with their anonymous volumes)
and network, and confirms that every port it used is closed.

### What the dashboard shows

The PromQL uses the names Server's Prometheus backend actually exposes, without
`_total` or unit suffixes:

| Panel | Families |
|---|---|
| Source events dispatched (per second) | `drasi_source_events_dispatched` |
| Query events processed (per second) | `drasi_query_events_processed` |
| Query results emitted, by `change_kind` | `drasi_query_results_emitted` |
| Reaction results and deliveries | `drasi_reaction_results_processed`; `drasi_component_inputs_processed` for the graph's sink (`completion="handled"`) |
| Query engine duration p50 / p95 | `drasi_query_engine_duration` histogram (`_bucket`), seconds |
| Queue depth: pending and peak | `drasi_queue_depth`, `drasi_queue_depth_max` |
| CPU, % of one core | `process_cpu_seconds_total`, as `rate(process_cpu_seconds_total[1m]) * 100` |
| Resident memory, threads | `process_resident_memory_bytes`, `process_threads` |
| File descriptors: open vs max | `process_open_fds`, `process_max_fds` (absent when unlimited) |
| Uptime | `process_start_time_seconds`, as `time() - process_start_time_seconds` |
| Trace | Jaeger trace for the **Trace ID** box |

Things that look odd but are expected:

- `drasi_reaction_results_processed` stays 0: the log reaction does not report
  its work through Core's reaction worker. The graph's deliveries to the
  reaction (`sink.handle`) move instead.
- Drasi counter panels show an info icon, "metric might not be a counter":
  Prometheus 3 expects counter names to end in `_total`. The rates are correct.
- Grafana's bundled Jaeger data source opens traces by ID, but it cannot search
  Jaeger 2.21 or later, which removed the v1 search API
  ([grafana/grafana-jaeger-datasource#87](https://github.com/grafana/grafana-jaeger-datasource/issues/87)).
  Its health check therefore reports 404. Search in the Jaeger UI, then paste a
  trace ID into the dashboard's **Trace ID** box.

### How the showcase runs

- **Pipeline:** the same matched local bundle, receipt checks and trust opt-in
  as `live`, in a new owned run directory under
  `target/observability-example/runs/`. Its `showcase/` subdirectory holds the
  Server and generator logs, the container ID files and the generated
  Prometheus target file. Like other run directories, it is retained.
- **Telemetry:** metrics backend `prometheus` at `/metrics` on the management
  listener, `processMetrics: true`, trace export to Jaeger's OTLP/HTTP
  endpoint, profile `debug` by default (so traces include query phases),
  sample ratio 1.0 and inspection on.
- **Input:** a generator posts one change about every 2 seconds through
  Server's source-data proxy, `POST /api/v1/instances/{instance}/sources/observations/push`.
  It inserts ten readings (`reading-1` … `reading-10`), then only updates them,
  so the query's result set stays bounded. `send` posts N updates to the same
  ten readings; it finds the running instance through `GET /api/v1/instances`
  and refuses (exit 1) when no demo answers. It also works against `live`.
- **Containers**, on the owned podman network
  `drasi-server-observability-showcase`, labelled
  `io.drasi.demo=server-observability-showcase`:

  | Container | Pinned image | Published on 127.0.0.1 |
  |---|---|---|
  | `drasi-server-observability-showcase-jaeger` | `cr.jaegertracing.io/jaegertracing/jaeger:2.22.0` | 4318 (OTLP/HTTP from Server), 16686 (UI) |
  | `drasi-server-observability-showcase-prometheus` | `docker.io/prom/prometheus:v3.16.0` | 9090 |
  | `drasi-server-observability-showcase-grafana` | `docker.io/grafana/grafana:13.3.1` | 3000 |

- **Prometheus** ([`showcase/prometheus.yml`](showcase/prometheus.yml)) scrapes
  Server every 5 s through the podman host alias
  `host.containers.internal:<management port>`. Server itself stays bound to
  127.0.0.1.
- **Grafana** allows anonymous **Viewer** access only. It creates no admin
  user, and both the login form and basic authentication are disabled. Its
  provisioned data sources are Prometheus (the default) and Jaeger, reached by
  container name on the owned network
  ([`showcase/grafana/provisioning/`](showcase/grafana/provisioning/)). The
  [dashboard](showcase/grafana/dashboards/drasi-server-observability.json) is
  also the home dashboard. Analytics, update checks and plugin preinstalls
  are off. No configuration file contains credentials.

The launcher refuses to start, before creating a run directory, if podman does
not work, if any of its ports is already in use, or if one of its container
names or its network already exists. It never replaces or removes anything it
did not create in that run. If an earlier run was killed without cleanup,
inspect and remove the leftover yourself, for example with
`podman rm -f drasi-server-observability-showcase-jaeger`.

Overrides:

| Variable | Default | Purpose |
|---|---|---|
| `DRASI_DEMO_PORT` | `18081` | Server management API and `/metrics` (also used by `send`) |
| `DRASI_DEMO_SOURCE_PORT` | `18090` | HTTP source listener |
| `DRASI_DEMO_OTLP_PORT` | `4318` | Jaeger OTLP/HTTP receiver |
| `DRASI_DEMO_JAEGER_PORT` | `16686` | Jaeger UI |
| `DRASI_DEMO_PROMETHEUS_PORT` | `9090` | Prometheus UI and API |
| `DRASI_DEMO_GRAFANA_PORT` | `3000` | Grafana |
| `DRASI_DEMO_PROFILE` | `debug` | `basic`, `debug` or `persistence` |

`DRASI_DEMO_SAMPLE_RATIO`, `DRASI_DEMO_INSPECTION`, `DRASI_DEMO_EXCLUDE` and
`DRASI_DEMO_SHUTDOWN_TIMEOUT` work as in [`live`](#state-ownership-and-environment).
The metrics backend (`prometheus`), process metrics (on) and the trace
endpoint (the owned Jaeger) are fixed. The dashboard's Jaeger UI link assumes
the default port; the printed link always uses the actual one.

## Advanced

The rest of this page covers the configuration-only default, the `live` modes
and an optional OpenTelemetry Collector walkthrough. The default remains
**configuration-only**: Off, no instances, no plugins. Two optional live modes
use real local plugins and exactly one metrics backend, Prometheus or
OTLP/HTTP protobuf, plus optional OTLP/HTTP protobuf traces:

- `live`: **HTTP source → native query → log reaction**; you send the input.
- `live-postgres`: **PostgreSQL CDC source → native query → log reaction**
  against a database you already run and supply.

Live modes also enable whole-process metrics and read-only inspection by
default; handled-branch latency stays disabled. Unlike the showcase, they
install or start no dashboard, container, database or collector service.

These are reproducible instructions, **not a record of successful OBS-05 or
OBS-06 live qualification**. See the
[baseline](../../docs/observability-baseline.md) for
recorded results, the deferred native-query blocker and outstanding gates.
Health alone proves no metrics or traces.

### Prerequisites and unchanged default

Use the supported Rust toolchain (currently 1.95.0), native build dependencies,
and matching Cargo-selected sibling `../drasi-core`. Bash and Python 3.11+ are
needed for live mode; curl is useful for checks. Run from the repository root:

```sh
bash examples/observability/demo.sh             # same as start
bash examples/observability/demo.sh validate    # no sockets/collector/plugins
bash examples/observability/demo.sh init        # interactive wizard
```

Default startup is foreground, **127.0.0.1:18081**, UI off, `persistConfig: false`,
auto-install off and verification on. The launcher builds Server with
`--locked --offline` (dependencies must already be cached)
under `target/observability-server` and uses
`target/observability-example/empty-plugins`, never the user's plugin directory.
`GET /health` and `GET /api/v1/instances` return success; the latter has empty
`data`. `/metrics` and `/api/v1/telemetry` return 404. Stop with Ctrl+C.
`DRASI_DEMO_PORT=18082` chooses another loopback management port.

Validation checks policy; it neither binds ports nor contacts the destination.
Prometheus, OTLP metrics and OTLP traces are available:

```sh
DRASI_DEMO_PROFILE=basic DRASI_DEMO_METRICS_BACKEND=prometheus \
  bash examples/observability/demo.sh validate
DRASI_DEMO_PROFILE=persistence DRASI_DEMO_METRICS_BACKEND=otlp \
  DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
  bash examples/observability/demo.sh validate
```

Replacing `validate` with `start` enables the chosen backends, but still starts
**no pipeline**. Settings for the other metrics backend do not activate it.
Inspection is independent of Off: the command below serves `/api/v1/telemetry`
with profile `off`, backend `none` and `path: null`, while
`/api/v1/telemetry/metrics` returns 503 `TELEMETRY_METRICS_UNAVAILABLE`:

```sh
DRASI_DEMO_INSPECTION=true bash examples/observability/demo.sh start
```

`server-config.yaml` keeps `processMetrics: false`; process metrics are shown
with the live modes below.

Invalid profiles, sample ratio `1.5`, interval `0`, timeout `31` and shutdown
budget `301` fail validation even if a destination is retained but inactive.
The wizard writes ignored `wizard-config.yaml`, refusing overwrite. Skip
telemetry to omit it, choose Off (optionally with inspection) for logs only, or
choose tracing and one metrics backend with optional process metrics and
inspection. The launcher does not silently switch to wizard output.

### 1. Explicitly build one matching local scenario bundle

Stop other instances of this demo before rebuilding. This is the manual,
opt-in preparation command; neither ordinary startup nor live startup downloads
plugins or adopts an arbitrary existing executable:

```sh
python3 scripts/plugin_origin.py mode                 # must print local
bash examples/observability/demo.sh build-plugins     # HTTP scenario
bash examples/observability/demo.sh build-plugins postgres   # optional
```

Each command builds only Server plus that scenario's three libraries with
their `dynamic-plugin` features: `drasi-source-http` (or
`drasi-source-postgres`), `drasi-computation-transformer-query` and
`drasi-reaction-log`. Query bootstrap is disabled, so no bootstrap library is
bundled. It uses the currently selected Core sources, including local edits.
Cargo runs offline; missing cached dependencies are an explicit failure, not a
registry fallback. It runs **no tests**. Core and Server lockfile graphs use
separate target directories:

- Server: `target/observability-server`
- Core libraries: `target/observability-plugins`
- Example-owned copies: `target/observability-example/bundles/<scenario>/`.
  Nothing is published to `target/debug/plugins` or another live plugin folder.

`target/observability-example/<scenario>-build.json` records the scenario, its
exact plugin set, source fingerprints (including dirty/untracked source),
compiler identity and binary hashes **after that build**. Live startup rejects a
missing receipt, an older receipt format, another scenario's receipt, a
missing/replaced binary or changed Core/Server source; rerun the matching build
command. Matching names, ABI versions or an old successful build are
insufficient provenance. This is a developer-trusted local receipt, not a
cryptographic supply-chain attestation. The host still performs its normal
metadata/ABI/capability checks when loading. No library is opened by the
preparation helper. The local libraries are unsigned: live modes require
`DRASI_DEMO_TRUST_LOCAL_PLUGINS=1` and then pass `--skip-verification` for only
the three receipt-checked local files. This is an explicit development trust
decision, not signature verification. Normal configuration-only startup retains
verification. Never use this opt-in for untrusted or registry-downloaded libraries.
Another Cargo build/test can replace the Server executable with a different
feature set; rerun `build-plugins` if the receipt then rejects it.

### 2. Run the HTTP pipeline, send input and scrape

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 bash examples/observability/demo.sh live prometheus
```

The launcher prints its unique instance and owned run directory. The HTTP
source listens on loopback port `DRASI_DEMO_SOURCE_PORT` (default `18090`). In
another terminal, send several readings, either directly to the source or
through Server's instance-scoped source-data proxy:

```sh
INSTANCE=run-REPLACE_WITH_PRINTED_SUFFIX
API="http://127.0.0.1:18081/api/v1/instances/$INSTANCE"
# The source listener validates its assigned UUID, not the name `observations`.
SOURCE_ID=$(curl --noproxy '*' --fail -s "$API/sources" |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["data"][0]["id"])')
for value in 1 2 3 4 5; do
  curl --noproxy '*' --fail -H 'content-type: application/json' \
    -d "{\"operation\":\"insert\",\"element\":{\"type\":\"node\",\"id\":\"reading-$value\",\"labels\":[\"Reading\"],\"properties\":{\"value\":$value}}}" \
    "http://127.0.0.1:18090/sources/$SOURCE_ID/events"
done
# The same JSON through Server's proxy (path accepts the source name):
curl --noproxy '*' --fail -H 'content-type: application/json' \
  -d '{"operation":"insert","element":{"type":"node","id":"reading-6","labels":["Reading"],"properties":{"value":6}}}' \
  "$API/sources/observations/push"
```

Then inspect the pipeline and metrics:

```sh
curl --noproxy '*' --fail http://127.0.0.1:18081/api/v1/instances
curl --noproxy '*' --fail "http://127.0.0.1:18081/api/v1/instances/$INSTANCE/sources"
curl --noproxy '*' --fail "http://127.0.0.1:18081/api/v1/instances/$INSTANCE/queries"
curl --noproxy '*' --fail "http://127.0.0.1:18081/api/v1/instances/$INSTANCE/reactions"
curl --noproxy '*' --fail http://127.0.0.1:18081/metrics
```

Names are `observations`, `observed-values`, `observed-log`; all component IDs
are **assigned UUIDs**, not these names. Every API component path is scoped by
the explicit instance. The unique eligible native query factory is selected by
the existing loader; there is no linked-reference fallback or implicit instance.

Check real observations, not just HTTP 200:

| Prometheus family | Expected type / observation |
|---|---|
| `drasi_source_events_dispatched` | counter; positive, increasing for assigned `source_id` |
| `drasi_query_events_processed` | counter; positive `phase="steady"` for assigned `query_id` |
| `drasi_query_results_emitted` | counter; positive `change_kind="add"` for that query |
| `drasi_reaction_results_processed` | counter; positive for assigned reaction + query |
| `drasi_query_engine_duration` | histogram; positive `_count`, finite nonnegative `_sum` in **seconds** |
| `drasi_query_commit_duration` | histogram under Persistence only; evaluation/publication and delivery acknowledgement are distinct commits |
| `drasi_queue_depth` / `drasi_queue_depth_max` | gauges for actual supported `queue_id`, with actual instance/graph identity |

Scrape twice, sending input in between, to establish growth. Count actual
inputs; asynchronous in-flight work can briefly lag. Identity labels are
`instance_id`, `graph_id`, `component_id` and the specific
source/query/reaction/queue labels, not guessed names or fabricated zeros. Queue IDs name physical bindings; they need not be component UUIDs.
No `_seconds` or `_total` suffix is appended. Histograms expose `_bucket`,
`_sum`, `_count`; finite boundaries match the [shared bucket list](../../docs/telemetry-configuration.md#metric-data-and-bounds).
The query result API and log reaction output should advance while scraping.

Stop with Ctrl+C before each variant. Keep the explicit local trust opt-in
for this terminal's subsequent live commands:

```sh
export DRASI_DEMO_TRUST_LOCAL_PLUGINS=1
DRASI_DEMO_PROFILE=basic bash examples/observability/demo.sh live prometheus
DRASI_DEMO_PROFILE=debug bash examples/observability/demo.sh live prometheus
DRASI_DEMO_PROFILE=persistence bash examples/observability/demo.sh live prometheus
# Exclude a real family that is otherwise present, without stopping processing:
DRASI_DEMO_EXCLUDE=drasi.query.engine.duration \
  bash examples/observability/demo.sh live prometheus
```

Basic/Debug both retain evaluator metrics; Debug does **not** imply Persistence
commit metrics. Persistence adds supported commit/outbox/recovery observations,
not every Debug detail. In the exclusion run, evaluator histogram HELP/TYPE,
bucket/count/sum series must all be absent while query/result/reaction counters
still grow. `drasi_query_progress_sequence` and `drasi_pipeline_end_to_end_duration`
must be absent in every Prometheus variant. The `process_*` families are present
by default and absent with `DRASI_DEMO_PROCESS_METRICS=false`.
Progress is deliberately **OTLP-only**. The default exclusion is `drasi.plugin.`;
`DRASI_DEMO_EXCLUDE` replaces that one literal substring (not a glob).

#### Process metrics and read-only inspection

Live modes enable both by default. While the pipeline runs:

```sh
curl --noproxy '*' -s http://127.0.0.1:18081/api/v1/telemetry
curl --noproxy '*' -s http://127.0.0.1:18081/metrics | grep '^process_'
curl --noproxy '*' -s http://127.0.0.1:18081/api/v1/telemetry/metrics | grep '^process_'
```

With the defaults and no trace endpoint, the settings are:

```json
{"profile":"persistence","tracing":{"exportEnabled":false,"sampleRatio":1.0,"localInspection":false},"metrics":{"backend":"prometheus","processMetrics":true,"path":"/api/v1/telemetry/metrics"},"logs":{"pathTemplate":"/api/v1/instances/{instanceId}/{collection}/{componentId}/logs","streamPathTemplate":"/api/v1/instances/{instanceId}/{collection}/{componentId}/logs/stream"}}
```

The settings contain no endpoint, header or credential. Both metrics routes
show the same unlabelled `process_*` families next to the `drasi_*` families:
`process_cpu_seconds_total` (a fractional counter), resident/virtual memory
bytes, open/max file descriptors, threads and start time. They describe the
whole Server process, not this instance. `process_max_fds` is omitted when the
soft limit is unlimited; unavailable readings are omitted, never `0`. CPU usage
is `rate(process_cpu_seconds_total[1m])`. `DRASI_DEMO_EXCLUDE=process_` also
removes them (replacing the default `drasi.plugin.` exclusion), and
`DRASI_DEMO_INSPECTION=false` makes both inspection routes return 404.
Inspection has no authentication: anyone who can reach the management port can
read it, which is loopback here.

### 3. Use an existing OTLP receiver instead

Start a receiver you already have that accepts **HTTP/protobuf metrics** at a
full URL, then:

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 \
DRASI_DEMO_METRICS_ENDPOINT=http://127.0.0.1:4318/v1/metrics \
DRASI_DEMO_EXPORT_INTERVAL=2 DRASI_DEMO_EXPORT_TIMEOUT=1 \
  bash examples/observability/demo.sh live otlp
```

No receiver is started by the launcher. HTTP is for credential-free loopback
development only. `/metrics` must return 404 in this mode. Decode the receiver's
actual request as `ExportMetricsServiceRequest`, not JSON based on a 200 response.
Check the dotted counterparts of the families above under scope `drasi.core`:
monotonic cumulative sums, gauges and explicit cumulative histograms with units
`1` / `s`, the same bounds and actual identity attributes. No input text,
result values or credentials may appear in attributes.

The same provider/resource also exports exactly **one instrument** named
`drasi.query.progress.sequence` under scope `drasi.progress`. It is a gauge,
with multiple points (not duplicate instruments):

- `drasi.progress.stage=output.produced`, and `input.committed` with
  `drasi.input.kind` / `drasi.input.id`.
- `drasi.instance.id` and `drasi.query.id` match this run/API UUID; generation,
  operation, output generation, config hash and persistent flag are retained.
- Values increase with actual input/output and remain within signed 64-bit
  range. Unsupported/unready/stopped queries are omitted, not emitted as zero.
  Stopping the query by its assigned API UUID and observing later exports can
  demonstrate omission; collector-retained stale samples are not a fresh export.

Both scopes share `service.name=drasi-server-observability`, Server version,
PID and one process-start-unique `service.instance.id`; a restart changes that
ID. With process metrics enabled (the default), the same requests also carry up
to seven `process_*` instruments, with the same names and no attributes, under
scope `drasi.server.process` and the same resource. They are sampled once
immediately before each export. In this mode `/api/v1/telemetry` reports
backend `otlp` with `path: null`, and `/api/v1/telemetry/metrics` returns 503
`TELEMETRY_METRICS_UNAVAILABLE`. Run the Basic/Debug/Persistence and exclusion
variants above with `otlp`.
To exclude progress entirely use `DRASI_DEMO_EXCLUDE=drasi.query.progress.`.
To distinguish committed-but-unchanged input from produced output, use the
existing focused progress proof; this always-inserting workload is not that
proof and does not promise exactly-once completion.

For authentication, adapt a private copy of the config with environment-backed
headers and a normally trusted **HTTPS** receiver. Do not write resolved tokens,
use `--skip-verification` to mean TLS bypass (it is unrelated), or add custom-CA
production settings. Redirects are not followed. No automatic `OTEL_*` variables
override this new policy. The focused TLS boundary test uses a test-only trust
anchor; it does not qualify a real external authenticated collector.

### 4. Export traces

Add a receiver you already run that accepts **HTTP/protobuf traces** at a full
URL. Either metrics backend can be combined with trace export:

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 \
DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
DRASI_DEMO_SAMPLE_RATIO=1.0 DRASI_DEMO_PROFILE=debug \
  bash examples/observability/demo.sh live prometheus
```

Send input as in section 2. Decode the receiver's request as
`ExportTraceServiceRequest`. Expect actual graph operations such as
`source.publish`, `query.process` and `sink.handle` (the graph's delivery to
the reaction), plus lifecycle/control spans (Debug adds
`query.evaluate`/`query.commit` phases), with `instance_id`, `graph_id` and
`component_id` attributes, and the same resource identity as OTLP metrics.
`reaction.process` appears only when a reaction reports its own work through
Core's reaction worker; the ordinary log reaction here did not in the recorded
walkthrough.
Completed native plugin spans keep their original IDs, parents, events and
timestamps; Server does not wrap them in new spans.

`DRASI_DEMO_SAMPLE_RATIO=0` exports no new root traces, while metrics, query
results and ordinary logs continue. A sampled incoming parent is still honored,
and an unsampled parent is never resampled. Without a trace endpoint, no trace
exporter starts. Exports are bounded (2,048 spans / 16 MiB, batches of 256,
one request at a time, no retries); an unavailable receiver drops batches with
diagnostics and never blocks processing.

#### What incoming context can and cannot reach

Send a W3C parent with either input path:

```sh
curl --noproxy '*' --fail -H 'content-type: application/json' \
  -H 'traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01' \
  -d '{"operation":"insert","element":{"type":"node","id":"traced-1","labels":["Reading"],"properties":{"value":7}}}' \
  "http://127.0.0.1:18081/api/v1/instances/$INSTANCE/sources/observations/push"
```

The proxy forwards only a validated `traceparent`/`tracestate` (never baggage or
credentials) and starts no span of its own; invalid metadata is dropped without
rejecting the data. This scenario's HTTP source is an **ordinary** source plugin.
Ordinary source cdylibs without negotiated portable context receive the header,
but the context does not cross their plugin boundary: downstream operations
start a new trace. Do not expect the incoming trace ID here. Native
`drasi.network` HTTP/gRPC sources do continue incoming context through polling;
that is qualified by the focused Server/Core tests, not by this walkthrough.

### 5. PostgreSQL CDC against an existing database

`live-postgres` never starts, creates or deletes a database, replication slot
or publication. Supply an isolated database with logical replication enabled
and a table such as:

```sql
CREATE TABLE observation (id integer PRIMARY KEY, value integer NOT NULL);
```

```sh
export DRASI_DEMO_TRUST_LOCAL_PLUGINS=1
export DRASI_DEMO_PG_HOST=127.0.0.1 DRASI_DEMO_PG_DATABASE=observability
export DRASI_DEMO_PG_USER=drasi_demo DRASI_DEMO_PG_PASSWORD=...  # from your secret store
bash examples/observability/demo.sh build-plugins postgres
bash examples/observability/demo.sh live-postgres prometheus
```

The launcher refuses to start until host, database, user and password are set;
`DRASI_DEMO_PG_PORT`, `DRASI_DEMO_PG_SLOT` and `DRASI_DEMO_PG_PUBLICATION`
default to `5432`, `drasi_observability_slot` and `drasi_observability_pub`.
Credentials stay environment references and are never written to receipts or
run directories. Insert rows (for example `INSERT INTO observation VALUES (1, 10);`)
and inspect results, metrics and traces as above. Each CDC change starts a new
trace root; it does not inherit the SQL client's trace. Remove the slot and
publication yourself when finished; the launcher does not.

### 6. See it in your own OpenTelemetry tool

A local OpenTelemetry Collector can stand in for your observability tool as the
receiver for sections 3 and 4. It needs no Docker and installs nothing
system-wide. Download the official `otelcol-contrib` release archive for your
platform into the ignored `target/observability-demo-tools/`, and verify it
before unpacking. Recent releases publish a `<archive>.sha256` file per
archive; older ones publish a combined
`opentelemetry-collector-releases_otelcol-contrib_checksums.txt`. The
walkthrough used v0.165.0 on macOS arm64:

```sh
VERSION=0.165.0 PLATFORM=darwin_arm64   # e.g. darwin_amd64, linux_amd64, linux_arm64
ARCHIVE="otelcol-contrib_${VERSION}_${PLATFORM}.tar.gz"
mkdir -p target/observability-demo-tools/download
cd target/observability-demo-tools/download
gh release download "v$VERSION" --repo open-telemetry/opentelemetry-collector-releases \
  --pattern "$ARCHIVE" --pattern "$ARCHIVE.sha256"
# Without gh: curl -fLO https://github.com/open-telemetry/opentelemetry-collector-releases/releases/download/v$VERSION/$ARCHIVE
# (and the same URL with .sha256 appended).
# On Linux, `sha256sum -c` can replace `shasum -a 256 -c`.
# Unpack only if the checksum prints OK:
printf '%s  %s\n' "$(cat "$ARCHIVE.sha256")" "$ARCHIVE" | shasum -a 256 -c &&
  tar -xzf "$ARCHIVE" -C ..
cd ../../..
target/observability-demo-tools/otelcol-contrib --version
```

Stop if the checksum does not match; do not unpack or run that archive.

[`otel-collector.yaml`](otel-collector.yaml) receives OTLP/HTTP on
`127.0.0.1:4318` (`/v1/metrics`, `/v1/traces`). It sends metrics to the `debug`
exporter and to a `prometheus` exporter on `127.0.0.1:8889`; traces go to
`debug` only. It is loopback-only and has no credentials or TLS, so use it for
development only. To adapt it, copy it into `target/observability-demo-tools/`.
Any new or edited non-Markdown file that Git does not ignore changes the live
bundle's source fingerprint, so the next live start then requires
`build-plugins` again.

```sh
target/observability-demo-tools/otelcol-contrib validate --config examples/observability/otel-collector.yaml
# Terminal 1: the Collector. `basic` (the file's default) logs only counts;
# `normal` also prints resource, scope, metric and span names.
target/observability-demo-tools/otelcol-contrib --config examples/observability/otel-collector.yaml \
  --set=exporters.debug.verbosity=normal
# Terminal 2: Server with OTLP metrics and traces sent to the Collector.
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 \
DRASI_DEMO_METRICS_ENDPOINT=http://127.0.0.1:4318/v1/metrics \
DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
DRASI_DEMO_EXPORT_INTERVAL=2 \
  bash examples/observability/demo.sh live otlp
```

Send five inputs as in section 2, then check:

```sh
curl --noproxy '*' -s http://127.0.0.1:18081/api/v1/telemetry
curl --noproxy '*' -s -w '\nHTTP %{http_code}\n' http://127.0.0.1:18081/api/v1/telemetry/metrics
curl --noproxy '*' -s http://127.0.0.1:8889/metrics | grep -E '^(process_|drasi_)'
sleep 10
curl --noproxy '*' -s http://127.0.0.1:8889/metrics | grep '^process_cpu_seconds_total'
```

- **Settings:** `/api/v1/telemetry` shows `"exportEnabled":true`,
  `"backend":"otlp"`, `"processMetrics":true` and `"path":null`.
  `/api/v1/telemetry/metrics` returns 503 `TELEMETRY_METRICS_UNAVAILABLE`
  because OTLP has no local read, and `/metrics` returns 404. Read the metrics
  from the Collector instead.
- **Collector scrape:** the `process_*` families appear next to `drasi_*`, and
  `process_cpu_seconds_total` grows between the two scrapes. The Collector's
  Prometheus exporter renames Drasi series as it re-exposes them: Drasi gauges
  with unit `1` get `_ratio` (for example `drasi_queue_depth_ratio`), Drasi
  counters get `_total` and duration histograms get `_seconds`. The `process_*`
  names arrive unchanged; their count units (`{file_descriptor}`, `{thread}`)
  add no suffix. It also adds `otel_scope_name` (`drasi.server.process`,
  `drasi.core` or `drasi.progress`). Server's own names are unchanged; the
  debug output and Server's Prometheus backend show them.
- **Shared identity:** both families carry the same `instance` (the resource's
  `service.instance.id`) and `job="drasi-server-observability"`
  (`service.name`). `target_info` carries `process_pid` and `service_version`.
  A Server restart changes `instance`. A Prometheus server that scrapes
  `:8889` renames these labels to `exported_job` / `exported_instance` unless
  its scrape job sets `honor_labels: true`.
- **Collector log:** each metrics export logs `ResourceMetrics #0
  service.version=… service.name=drasi-server-observability process.pid=…
  service.instance.id=…`, then `ScopeMetrics … drasi.server.process` followed
  by up to seven `process_*` lines with `{}` (no attributes), beside the
  `drasi.core` and `drasi.progress` scopes. Traces log `ScopeTraces #0
  drasi.core` with `source.publish`, `query.process` and `sink.handle` spans
  that carry `instance_id`, `graph_id` and `component_id`.
- **In your tool:** point a Prometheus-compatible scraper at
  `http://127.0.0.1:8889/metrics`. `rate(process_cpu_seconds_total[1m])` is
  CPU usage in cores (multiply by 100 for percent of one core), and
  `process_resident_memory_bytes` is resident memory. To export straight to your
  own tool instead, set the two endpoint variables to its OTLP/HTTP protobuf
  URLs; see section 3 for HTTPS and authentication.

Stop Server with Ctrl+C first, then the Collector. Confirm that ports 18081,
18090, 4318 and 8889 are closed, for example with
`lsof -nP -iTCP:4318 -iTCP:8889 -iTCP:18081 -iTCP:18090 -sTCP:LISTEN`.
While the Collector still runs, it keeps serving the last values for a few
minutes. Those are retained samples, not fresh exports.

### State, ownership and environment

Each live or showcase start creates `target/observability-example/runs/run-<random>/` and
starts Server **from that directory**. It contains an explicit demo ownership
marker and only copies of the three verified libraries. WAL files are beneath
that run's `data/<hex-instance-key>/wal/`. `persistIndex: false` and unique
instance/work directories avoid old index/WAL reuse with new component UUIDs.
`persistConfig: false` does not disable WAL. No state is deleted, no configured
slot is recreated, and no existing user's data is cleared. After stopping and
joining the foreground process, you may inspect/archive/remove only the exact
printed directory bearing `demo-owner.json`; never delete a generic `data/`.
This example grows memory/results/state while left running; keep runs bounded.

First SIGINT/SIGTERM closes admission and requests retained cleanup. Repeated
signals do not create a new budget. Wait for actual process exit, then confirm
the owned management/scrape port is closed before another run. A five-second
telemetry allowance is capped by the overall 30-second wait; these are bounded
waits, not permission to abandon stalled workers or a hard process-exit promise.

| Variable | Configuration-only default | Live default |
|---|---|---|
| `DRASI_DEMO_PORT` | `18081` | `18081` |
| `DRASI_DEMO_PROFILE` | `off` | `persistence` (Basic/Debug also supported) |
| `DRASI_DEMO_METRICS_BACKEND` | `none` | explicit live argument, default `prometheus` |
| `DRASI_DEMO_METRICS_ENDPOINT` | `http://127.0.0.1:4318/v1/metrics` | same |
| `DRASI_DEMO_EXPORT_INTERVAL` | `30` seconds | `2` seconds |
| `DRASI_DEMO_EXPORT_TIMEOUT` | `5` seconds | `1` second |
| `DRASI_DEMO_SHUTDOWN_TIMEOUT` | `5` seconds | `5` seconds |
| `DRASI_DEMO_INSPECTION` | `false` | `true` |
| `DRASI_DEMO_PROCESS_METRICS` | not used; disabled | `true` |
| `DRASI_DEMO_TRACE_ENDPOINT` | empty (no trace export) | same |
| `DRASI_DEMO_SAMPLE_RATIO` | `0.1` | `1.0` |
| `DRASI_DEMO_SOURCE_PORT` | not used | `18090` (HTTP scenario) |
| `DRASI_DEMO_PG_*` | not used | PostgreSQL scenario only; see section 5 |
| `DRASI_DEMO_EXCLUDE` | not used | `drasi.plugin.` |
| `DRASI_DEMO_INSTANCE` | not used | launcher-owned unique run name |

The showcase uses the live defaults except where its
[overrides table](#how-the-showcase-runs) differs.

These are ordinary `ConfigValue` references, not a
second environment system. Existing environment values win over defaults;
unset overrides to restore baseline behavior. The CLI loads optional `.env`
without overriding existing values; live mode runs in a new directory without
one. New OTLP interval/timeout ranges are 1..=3,600 / 1..=30 **seconds**; legacy
root `metrics` remains HTTP/JSON and **milliseconds**. Never combine both.

Loopback is intentional. There is no new authentication system; production
scraping needs a trusted proxy/restricted network, and CORS is not authentication.
See [telemetry configuration](../../docs/telemetry-configuration.md) for bounds,
scrape overload, ownership and startup rollback.
