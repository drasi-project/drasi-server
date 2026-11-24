# Drasi Server observability example

This example shows how to observe a running Drasi Server with standard
open-source tools. Drasi Server has built-in telemetry. It can:

- export **traces** over OpenTelemetry (OTLP/HTTP), one per change that flows
  through a pipeline;
- expose **metrics** for Prometheus to scrape, or push them over OTLP;
- report **process metrics** (CPU, memory, threads, file descriptors) for the
  whole Server process;
- serve a read-only **inspection API** that describes the active telemetry
  settings.

The example runs a small continuous-query pipeline so that each of these has
real data to show. The fastest way to see everything is the
[showcase](#quick-start-run-the-showcase): one command starts the pipeline with
Jaeger, Prometheus and Grafana and feeds it steady input. The rest of this page
explains the configuration, walks through what to look at, and shows how to
point Server at your own tools.

## Contents

- [What the example shows](#what-the-example-shows)
- [The Drasi Server configuration](#the-drasi-server-configuration)
- [Prerequisites](#prerequisites)
- [Quick start: run the showcase](#quick-start-run-the-showcase)
- [Explore the showcase](#explore-the-showcase)
- [How Jaeger, Prometheus and Grafana are set up](#how-jaeger-prometheus-and-grafana-are-set-up)
- [Run the example other ways](#run-the-example-other-ways)
- [How the launcher works](#how-the-launcher-works)
- [Environment variables](#environment-variables)
- [Troubleshooting](#troubleshooting)
- [Security notes](#security-notes)
- [Further reading](#further-reading)

## What the example shows

The example runs one Drasi Server instance with a four-component pipeline:

- **Source `observations`** (HTTP): receives changes to `Reading` nodes.
- **Query `observed-values`**: the continuous Cypher query
  `MATCH (n:Reading) RETURN n.value AS value`. Drasi keeps its result up to
  date as readings are inserted, updated or deleted.
- **Query `high-readings`**: `MATCH (n:Reading) WHERE n.value > 30 RETURN n.value AS value`.
  A reading enters or leaves its result as the value crosses 30. With two
  queries on the same source, you can compare per-query metrics and queues side
  by side.
- **Reaction `observed-log`**: writes every change to either query's result to
  Server's log.

While changes flow through the pipeline, you can examine:

| Telemetry | What you see | Where |
|---|---|---|
| Traces | One trace per change and query: `source.publish` → `query.process` → `sink.handle` | Jaeger and Grafana, or any OTLP/HTTP trace receiver |
| Pipeline metrics | `drasi_*` families: events dispatched and processed, results emitted, query engine duration, queue depth | Prometheus and Grafana, or any OTLP/HTTP metrics receiver |
| Process metrics | `process_*` families: CPU, memory, threads, file descriptors, start time | Same as pipeline metrics |
| Inspection API | The active telemetry settings, without endpoints or credentials | `GET /api/v1/telemetry` on Server's management port |

In the showcase, the pieces connect like this. Each arrow points from the side
that starts the request:

```text
input generator ── POST, one change every ~2 s ──▶ Drasi Server
                                                   source → query → reaction
                                                     │                  ▲
                                 pushes each trace   │                  │  scrapes /metrics
                                 (OTLP/HTTP)         ▼                  │  every 5 s
                                                   Jaeger           Prometheus
                                                   stores traces    stores metrics
                                                     ▲                  ▲
                                 fetches one trace   │                  │  runs each panel's
                                 by ID               │                  │  query every 5 s
                                                     └──── Grafana ─────┘
                                                              ▲
                                                              │  you open the dashboard
                                                           browser
```

Server sends traces to Jaeger, and Prometheus collects metrics from Server.
Grafana stores nothing itself: it reads from Jaeger and Prometheus whenever the
dashboard refreshes. Server runs directly on your machine, bound to 127.0.0.1.
Jaeger, Prometheus and Grafana run as podman containers, published only on
127.0.0.1.

### Files in this folder

| File | Purpose |
|---|---|
| [`drasi-server.yaml`](drasi-server.yaml) | Server configuration for the HTTP pipeline, used by `showcase` and `live` |
| [`postgres-config.yaml`](postgres-config.yaml) | The same pipeline with a PostgreSQL change-data-capture source, used by `live-postgres` |
| [`server-config.yaml`](server-config.yaml) | A Server with no instances and Server's default telemetry (basic, no destinations), used by `start` and `validate` |
| [`demo.sh`](demo.sh) | The launcher for every mode |
| [`local_bundle.py`](local_bundle.py) | Builds and checks the local plugin bundle |
| [`showcase/`](showcase/) | Prometheus configuration, Grafana provisioning and dashboard, and the input generator `traffic.py` |
| [`otel-collector.yaml`](otel-collector.yaml) | Optional OpenTelemetry Collector configuration |

## The Drasi Server configuration

Everything Server does in the example comes from one file,
[`drasi-server.yaml`](drasi-server.yaml). It has two parts:

- The `instances` block defines the pipeline: an HTTP source that receives
  input, two Cypher queries over it, and a log reaction that prints both
  queries' changes.
- The `telemetry` block turns on the traces and metrics that the rest of the
  example displays.

Below is the file with the showcase's values filled in. It is abridged: the
launcher replaces each `${VAR:-default}` placeholder, and the instance ID is the
name of the run directory it creates. Every setting is commented in the file
itself.

```yaml
apiVersion: drasi.io/v1
host: "127.0.0.1"
port: 18081                     # management API and /metrics
persistConfig: false
enableUi: false

instances:
  - id: "<run-id>"
    sources:
      - kind: http              # input arrives here
        name: observations
        host: "127.0.0.1"
        port: 18090
    queries:
      - name: observed-values
        query: "MATCH (n:Reading) RETURN n.value AS value"
        queryLanguage: Cypher
        inputs: [observations]
      - name: high-readings     # only readings above 30
        query: "MATCH (n:Reading) WHERE n.value > 30 RETURN n.value AS value"
        queryLanguage: Cypher
        inputs: [observations]
    reactions:
      - kind: log               # prints every query result change
        name: observed-log
        queries: [observed-values, high-readings]

telemetry:
  profile: debug                # adds query phases (query.evaluate, query.commit, …) to traces
  inspection:
    enabled: true               # serves GET /api/v1/telemetry and /api/v1/telemetry/metrics
  tracing:
    endpoint: http://127.0.0.1:4318/v1/traces   # Jaeger's OTLP/HTTP receiver
    protocol: http/protobuf
    serviceName: drasi-server-observability
    sampleRatio: 1.0            # keep every trace
  metrics:
    backend: prometheus         # served at /metrics, scraped by Prometheus
    processMetrics: true        # CPU, memory, threads, file descriptors, uptime
    prometheus:
      path: /metrics
```

The **profile** controls how much detail Server collects:

| Profile | What it collects |
|---|---|
| `off` | Nothing. The settings route stays on unless `inspection.enabled: false`. |
| `basic` | The standard pipeline metrics and spans |
| `debug` | `basic`, plus debug details such as query phase spans (`query.evaluate`, `query.commit`, …) |
| `persistence` | `basic`, plus storage and recovery details such as `drasi_query_commit_duration`; it does not include the `debug` details |

Server uses `basic` when the `telemetry` section or `profile` is omitted; `off`
must be chosen explicitly. The showcase defaults to `debug` and the `live` modes
to `persistence`; set `DRASI_DEMO_PROFILE` to choose another. For every field
and its limits, see
[telemetry configuration](../../docs/telemetry-configuration.md).

## Prerequisites

Run every command on this page from the repository root. You need:

- The Rust toolchain pinned in `rust-toolchain.toml` (currently 1.95.0) and the
  native build dependencies.
- The sibling `../drasi-core` checkout that this repository's Cargo
  dependencies point to. `python3 scripts/plugin_origin.py mode` must print
  `local`.
- Cargo dependencies already in the local cache: the example builds offline
  (`--locked --offline`).
- Bash, Python 3.11 or later, and curl.
- For the showcase: podman. On macOS, start it first with
  `podman machine start`.
- For the PostgreSQL variant only: an existing database with logical
  replication enabled.

## Quick start: run the showcase

1. Build Server and the three plugins the pipeline uses. Do this once, and again
   after any change to Server or Core source:

   ```sh
   bash examples/observability/demo.sh build-plugins
   ```

2. Start the showcase:

   ```sh
   DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 bash examples/observability/demo.sh showcase
   ```

   The plugins you just built are unsigned. `DRASI_DEMO_TRUST_LOCAL_PLUGINS=1`
   confirms that you trust them; see
   [Local plugin bundle and trust](#local-plugin-bundle-and-trust).

3. Wait for `Demo is live`. The first start pulls three pinned images. The
   launcher then prints its links:

   ```text
   Grafana dashboard: http://127.0.0.1:3000/d/drasi-server-observability?var-trace_id=<trace id>
   Jaeger traces:     http://127.0.0.1:16686/search?service=drasi-server-observability
   Prometheus:        http://127.0.0.1:9090/targets
   More input:        bash examples/observability/demo.sh send 20
   High readings:     bash examples/observability/demo.sh send-high 20
   ```

4. Explore, as described in the next section.

5. Press Ctrl+C to stop. If the showcase runs in the background, where Ctrl+C
   does not reach it, stop it from another terminal:

   ```sh
   pkill -f 'demo.sh showcase' || echo 'No showcase is running.'
   ```

   Either way, the launcher stops the input generator, then Server, then
   removes only its own containers (with their anonymous volumes) and network,
   and confirms that every port it used is closed.

## Explore the showcase

The commands and links below use the default ports. If you changed them, use
the links the launcher prints.

### Grafana: the dashboard

Open the Grafana link. The **Drasi Server observability** dashboard shows the
last 15 minutes and refreshes every 5 seconds. It has three rows.

**Drasi pipeline** shows how changes move through the pipeline. Each change
the generator sends appears, in order, in the panels below. The source hands
every change to each query separately, so one change counts as one event per
query, and the per-query panels show one line for each query.

| Panel | What it means | Metric |
|---|---|---|
| Source events dispatched | Events per second that the source handed to the pipeline: one per change and query, so with two queries about twice the input rate | `drasi_source_events_dispatched` |
| Query events processed | Changes per second that each query finished processing | `drasi_query_events_processed` |
| Query results emitted | Changes per second to each query's result, by `change_kind`: `add` (a new row), `update` (a changed row) or `delete` (a removed row) | `drasi_query_results_emitted` |
| Reaction results and deliveries | Results per second delivered to the reaction, from both queries | `drasi_component_inputs_processed` for the pipeline's sink (`completion="handled"`), and `drasi_reaction_results_processed` |
| Query engine duration | How long the query engine took to evaluate one change, as p50 and p95 for each query | `drasi_query_engine_duration` histogram, in seconds |
| Queue depth: pending | Changes waiting in each queue right now | `drasi_queue_depth` |
| Queue depth: peak | The most changes each queue has held since it was created | `drasi_queue_depth_max` |

**Reading the legends.** Each line is labelled with its query's name, such as
`observed-values` or `high-readings`. Each query has two queues, labelled with
the query name and the queue's role:

- `<query> input`: the queue where changes from the source wait for the query;
- `<query> output`: the queue where the query's result changes wait to be
  delivered toward the reaction.

Drasi's metrics identify components by the UUID Server assigns at startup, so
the dashboard looks names up from two series Drasi publishes alongside them:
`drasi_component_info` (each component's current name) and
`drasi_queue_edge_info` (which components each queue connects). See
[Prometheus: query the metrics](#prometheus-query-the-metrics) for how the
join works. A queue attached to no query would appear under its raw `queue_id`.

The generator inserts ten readings, then only updates them. **Query results
emitted** therefore shows a short burst of `add` from both queries, then a
steady line of `update` from `observed-values`. `high-readings` keeps emitting
`add` and `delete` as readings cross 30, plus `update` for readings that change
while staying above 30. The **peak** queue depth is a high-water mark: it only
goes up while Server runs, so a burst from `send` leaves it raised. A burst
also shows how differently the queues fill: in one run, `send 300` raised one
query's input queue to 128 waiting changes and the other's to 56, while both
output queues stayed at 1.

**Reading p50 and p95.** Every evaluation of a query is timed. **p50** (the
median) is the time within which half of the evaluations finished; **p95** is
the time within which 95% finished, so it shows the slow tail. A p50 of 1 ms and
a p95 of 4 ms mean a typical change takes about 1 ms and only 1 change in 20
takes longer than 4 ms. The time covers only the query engine's evaluation in
steady state (`phase="steady"`): not committing results or delivering them to
the reaction. Treat both lines as estimates:

- Each point is computed over a short sliding window (Grafana's
  `$__rate_interval`, about 20 seconds here). At one change every 2 seconds,
  that is only about ten evaluations, so the lines jump. A burst from `send`
  gives more samples.
- Prometheus estimates percentiles by interpolating inside histogram buckets.
  The bucket boundaries near these values are 500 µs, 1 ms, 5 ms and 10 ms, so a
  p95 of 4 ms means "the slowest few evaluations took between 1 and 5 ms", not
  exactly 4 ms.

**Server process** shows the whole `drasi-server` process, including the
plugins loaded into it, not one instance:

| Panel | What it means | Metric |
|---|---|---|
| CPU | CPU time the process uses, as a percentage of one core (100% is one core fully busy) | `process_cpu_seconds_total`, as `rate(process_cpu_seconds_total[1m]) * 100` |
| Resident memory | Physical memory the process occupies | `process_resident_memory_bytes` |
| Threads | Operating-system threads in the process | `process_threads` |
| File descriptors: open vs max | Open files and sockets, against the process's soft limit; max is absent when the limit is unlimited | `process_open_fds`, `process_max_fds` |
| Uptime | Time since Server started | `process_start_time_seconds`, as `time() - process_start_time_seconds` |

**Traces (Jaeger)** shows one trace, selected by the **Trace ID** box at the top
of the dashboard. The launcher's link fills the box with a real trace. To view
another, copy a trace ID from Jaeger into the box.

Things that look odd but are expected:

- `drasi_reaction_results_processed` stays 0: the log reaction does not report
  its work through Core's reaction worker. The pipeline's deliveries to the
  reaction (`sink.handle`) move instead.
- Drasi counter panels show an info icon, "metric might not be a counter":
  Prometheus 3 expects counter names to end in `_total`. The rates are correct.
- Grafana's bundled Jaeger data source opens traces by ID, but it cannot search
  Jaeger 2.21 or later, which removed the v1 search API
  ([grafana/grafana-jaeger-datasource#87](https://github.com/grafana/grafana-jaeger-datasource/issues/87)).
  Its health check therefore reports 404. Search in the Jaeger UI, then paste a
  trace ID into the dashboard's **Trace ID** box.

### Jaeger: follow one change

Open the Jaeger link and choose any trace for the service
`drasi-server-observability`. Each change produces one trace per query, so with
two queries every change appears twice. A trace contains:

- `source.publish`: the source hands the change to that query.
- `query.process`: the query processes the change. With the `debug` profile,
  child spans show its phases, such as `query.evaluate` and `query.commit`.
- `sink.handle`, twice, when the change altered the query's result: once for the
  query's output sink and once for the log reaction. When the result did not
  change, for example a `high-readings` update that stays below 30, the trace
  ends at `query.process`.

Each span has `instance_id`, `graph_id` and `component_id` attributes, which
identify the instance and component that did the work. A `query.process` span's
`component_id` is the query's ID; `drasi_component_info` maps it to the name.
The span durations show where the time for that change went. Jaeger also lists
lifecycle and control spans, for example from startup.

### Prometheus: query the metrics

Open the Prometheus link. The **Target health** page should show the
`drasi-server` job as **UP**. (Its endpoint link does not open in a browser; see
[Prometheus (metrics)](#prometheus-metrics).) Then try these on the **Query**
page:

```promql
sum(rate(drasi_source_events_dispatched[1m]))
sum by (query_id, change_kind) (rate(drasi_query_results_emitted[1m]))
histogram_quantile(0.95, sum by (query_id, le) (rate(drasi_query_engine_duration_bucket{phase="steady"}[1m])))
drasi_queue_depth_max
rate(process_cpu_seconds_total[1m]) * 100
```

They show, in order: changes per second entering the pipeline, each query's
result changes per second by kind, each query's p95 engine duration in seconds,
the peak depth of every queue (two per query), and Server's CPU as a percentage
of one core.

Drasi metric names have no `_total` or unit suffix. Histograms expose `_bucket`,
`_sum` and `_count` series, with durations in seconds. Series carry
`instance_id` and `graph_id` labels, plus labels such as `component_id`,
`query_id` or `queue_id` that identify the component or queue. Component IDs
are UUIDs that Server assigns, not the names in the configuration.

Names come from two extra series:

- `drasi_component_info{component_id, component_kind, component_name}` is 1 for
  each component's current name. A name a component no longer has, after a
  rename or removal, stays at 0, so select current names with `== 1`.
- `drasi_queue_edge_info{queue_id, from_component_id, to_component_id}` is 1
  for each graph edge bound to a queue. A query's input queue is the one whose
  edge ends at the query; its output queue is the one whose edge starts there.

Join them to label any per-query metric by name. A query's `query_id` is its
component ID:

```promql
sum by (instance_id, graph_id, component_name) (
  rate(drasi_query_events_processed[1m])
  * on (instance_id, graph_id, query_id) group_left (component_name)
    label_replace(
      max by (instance_id, graph_id, component_id, component_name) (
        drasi_component_info{component_kind="query"} == 1),
      "query_id", "$1", "component_id", "(.*)")
)
```

Names are unique only within one instance's graph, so the join carries
`instance_id` and `graph_id`. The showcase dashboard runs a single instance and
leaves them out.

The dashboard's queue panels use the same idea through `drasi_queue_edge_info`;
open a panel's menu and choose **Edit** to see its queries.

### Server's own APIs

Server's management API listens on port 18081. Its interactive API reference
is at `http://127.0.0.1:18081/api/v1/docs/`. From a terminal:

```sh
API=http://127.0.0.1:18081/api/v1
curl --noproxy '*' -s "$API/instances"
INSTANCE=$(curl --noproxy '*' -s "$API/instances" |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["data"][0]["id"])')
curl --noproxy '*' -s "$API/instances/$INSTANCE/queries"
curl --noproxy '*' -s "$API/instances/$INSTANCE/queries/observed-values/results"
curl --noproxy '*' -s "$API/instances/$INSTANCE/queries/high-readings/results"
curl --noproxy '*' -s "$API/telemetry"
curl --noproxy '*' -s http://127.0.0.1:18081/metrics | grep -E '^(drasi|process)_'
```

- The instance ID is `run-<random>`; each start creates a new one.
- The queries listing shows each query's `name` next to its assigned `id`, the
  ID that metrics use.
- The results calls return each query's current rows, one `{"value": …}` per
  reading (for `high-readings`, only readings above 30). Component paths accept
  either a name or the assigned UUID.
- `/api/v1/telemetry` returns the active telemetry settings (see
  [Process metrics and inspection](#process-metrics-and-inspection)).
- `/metrics` is the same text Prometheus scrapes.
- `--noproxy '*'` stops curl from sending these local requests through a proxy
  set in your environment.

The log reaction's output is in Server's log, in the run directory the launcher
prints: `target/observability-example/runs/<instance>/showcase/server.log`.

### Add load

Send a burst of changes from another terminal and watch the rates jump within a
few seconds:

```sh
bash examples/observability/demo.sh send 20     # N changes; default 5, at most 1000
```

`send` finds the running instance through `GET /api/v1/instances` and updates
the same ten readings. It exits with status 1 if no demo answers. It also works
with the `live` modes below.

To make `high-readings` busy, send changes that cross its threshold:

```sh
bash examples/observability/demo.sh send-high 40   # N changes; default 20, at most 1000
```

`send-high` moves a reading above 30 and then the same reading below 30, in
turn for each reading. Each pair adds the reading to the `high-readings` result
and removes it again, so that query's `add` and `delete` lines jump. No change
can reach only one query: the source hands every change to both, so
`observed-values` processes the same changes and reports them as `update`.

## How Jaeger, Prometheus and Grafana are set up

Server pushes traces to a receiver and serves metrics for a scraper. Each tool
needs only a little setup to take part. `demo.sh showcase` does all of it for
you; to use your own instances of these tools, repeat the same steps.

### Jaeger (traces)

Jaeger needs no configuration file. The showcase runs the stock Jaeger 2 image
with its default configuration, which accepts OTLP and keeps traces in memory.
Two things connect it to Server:

- Publish Jaeger's OTLP/HTTP receiver (container port 4318) and its UI
  (container port 16686).
- Point Server's `telemetry.tracing.endpoint` at the receiver's full URL,
  `http://127.0.0.1:4318/v1/traces`, with `protocol: http/protobuf`. The
  `serviceName` setting is the service you select in Jaeger's search.

### Prometheus (metrics)

Prometheus pulls metrics, so it must be told where Server is:

- In Server's configuration, set `telemetry.metrics.backend: prometheus`.
  Server then serves metrics at `/metrics` on its management port (18081).
- In Prometheus, add a scrape job for that address.
  [`showcase/prometheus.yml`](showcase/prometheus.yml) defines the job
  `drasi-server`, which scrapes `/metrics` every 5 seconds. Its target comes
  from a small file that the launcher writes at start:
  `host.containers.internal:<management port>`, the name a podman container
  uses to reach your machine.

`host.containers.internal` resolves only inside the containers. The endpoint
link on Prometheus's **Target health** page therefore does not open in your
browser; open `http://127.0.0.1:18081/metrics` instead. A Prometheus that runs
directly on your machine would use `127.0.0.1:18081` as its target.

### Grafana (dashboard)

Grafana keeps no metrics or traces of its own, and Server never sends anything
to it. Each time the dashboard refreshes (every 5 seconds), Grafana asks the
two tools that do store data:

- **Metrics come from Prometheus.** Every metric panel holds a PromQL query,
  for example `sum(rate(drasi_source_events_dispatched[$__rate_interval]))`.
  Grafana sends each query to Prometheus's query API and draws the time series
  it returns. Prometheus answers from the samples it scraped from Server's
  `/metrics`, so the graphs trail Server by a few seconds.
- **Traces come from Jaeger.** The trace panel asks Jaeger for the single trace
  whose ID is in the dashboard's **Trace ID** box, and draws its spans. It
  looks traces up by ID only and cannot search, which is why you copy IDs from
  the Jaeger UI.
- **Grafana's server makes these requests, not your browser** (the data
  sources use `access: proxy`). That is why the data sources can use container
  names such as `http://drasi-server-observability-showcase-prometheus:9090`,
  which resolve only on the showcase's podman network. Your browser talks only
  to Grafana, on port 3000.

For that, Grafana needs two data sources and a dashboard. The showcase
provisions them from files, so nothing is set up by hand:

- [Data sources](showcase/grafana/provisioning/datasources/drasi-showcase.yaml):
  Prometheus (the default) and Jaeger, each reached by container name on the
  showcase's podman network. The Prometheus data source declares the 5-second
  scrape interval (`timeInterval: 5s`), which Grafana uses to size each
  panel's `$__rate_interval` window.
- A [dashboard provider](showcase/grafana/provisioning/dashboards/drasi-showcase.yaml)
  loads [`drasi-server-observability.json`](showcase/grafana/dashboards/drasi-server-observability.json),
  which is also set as Grafana's home dashboard.
- Environment settings on the container allow anonymous read-only
  (**Viewer**) access with no admin user or login form, and turn off analytics
  and update checks.

To use the dashboard in your own Grafana, add a Prometheus data source with UID
`drasi-prometheus` and a Jaeger data source with UID `drasi-jaeger` (the UIDs
the dashboard refers to), then import the dashboard JSON.

## Run the example other ways

The showcase is one of several modes of the same launcher:

| Command | What it runs |
|---|---|
| `start` (the default) | A Server with no instances and Server's default telemetry (basic, no destinations, settings route only) |
| `validate` | Checks `server-config.yaml` and its telemetry settings without starting Server |
| `init` | An interactive wizard that writes `wizard-config.yaml` |
| `build-plugins [http\|postgres]` | Builds Server and one scenario's plugins (default `http`) |
| `live [prometheus\|otlp]` | The HTTP pipeline with one metrics backend and optional traces; you send the input |
| `live-postgres [prometheus\|otlp]` | The same pipeline with a PostgreSQL CDC source, against your own database |
| `showcase` | The HTTP pipeline with Jaeger, Prometheus, Grafana and steady input |
| `send [N]` | Sends N reading updates (default 5, at most 1000) to the running demo |
| `send-high [N]` | Sends N changes that move readings above and then below 30 (default 20, at most 1000), so `high-readings` adds and deletes rows |

Unlike the showcase, the `live` modes start no containers, databases or
collectors, and send no input of their own. They turn on process metrics and
inspection by default.

### A Server without a pipeline: `start`, `validate`, `init`

```sh
bash examples/observability/demo.sh             # same as start
bash examples/observability/demo.sh validate    # no sockets, collector or plugins
bash examples/observability/demo.sh init        # interactive wizard
```

`start` runs Server in the foreground on **127.0.0.1:18081** with
[`server-config.yaml`](server-config.yaml): no instances, UI off,
`persistConfig: false`, plugin auto-install off and signature verification on.
The launcher builds Server offline under `target/observability-server` and uses
an empty plugin directory, `target/observability-example/empty-plugins`, never
your own. `GET /health` and `GET /api/v1/instances` succeed (the latter with
empty `data`). `/metrics` returns 404 because no Prometheus backend is active.
`/api/v1/telemetry` returns the settings: profile `basic`, backend `none` and
`path: null`. `/api/v1/telemetry/metrics` returns 404 because the default mounts
only the settings route. `DRASI_DEMO_PORT=18082` chooses another port. Stop with
Ctrl+C.

`validate` checks the telemetry settings without binding ports or contacting
any destination:

```sh
DRASI_DEMO_PROFILE=basic DRASI_DEMO_METRICS_BACKEND=prometheus \
  bash examples/observability/demo.sh validate
DRASI_DEMO_PROFILE=persistence DRASI_DEMO_METRICS_BACKEND=otlp \
  DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
  bash examples/observability/demo.sh validate
```

Out-of-range values fail validation, for example an unknown profile, sample
ratio `1.5`, export interval `0`, export timeout `31` or shutdown timeout `301`.
Replacing `validate` with `start` turns the chosen backends on, but still runs
no pipeline. Settings for the metrics backend you did not choose stay inactive.

Inspection works even with telemetry off. The command below serves
`/api/v1/telemetry` with profile `off`, backend `none` and `path: null`, while
`/api/v1/telemetry/metrics` returns 404 (only `inspection.enabled: true` mounts
it):

```sh
DRASI_DEMO_PROFILE=off bash examples/observability/demo.sh start
```

The wizard writes `wizard-config.yaml` (ignored by Git) and refuses to
overwrite an existing one. Skipping telemetry keeps Server's default (basic, no
destinations, settings route only). Otherwise you choose a profile, including
Off, and which inspection routes to serve: settings only (the default),
settings and metrics, or none. Any profile other than Off also offers tracing
and one metrics backend with optional process metrics. The launcher never
switches to the wizard's output on its own.

### The HTTP pipeline with Prometheus: `live`

Build the bundle as in the [quick start](#quick-start-run-the-showcase), then:

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 bash examples/observability/demo.sh live prometheus
```

The launcher prints the instance ID and run directory. Send input from another
terminal with `demo.sh send`, or post changes yourself, either straight to the
HTTP source (port 18090) or through Server's source-data proxy:

```sh
INSTANCE=run-REPLACE_WITH_PRINTED_SUFFIX
API="http://127.0.0.1:18081/api/v1/instances/$INSTANCE"
# The source listener checks its assigned UUID, not the name `observations`.
SOURCE_ID=$(curl --noproxy '*' --fail -s "$API/sources" |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["data"][0]["id"])')
for value in 1 2 3 4 5; do
  curl --noproxy '*' --fail -H 'content-type: application/json' \
    -d "{\"operation\":\"insert\",\"element\":{\"type\":\"node\",\"id\":\"reading-$value\",\"labels\":[\"Reading\"],\"properties\":{\"value\":$value}}}" \
    "http://127.0.0.1:18090/sources/$SOURCE_ID/events"
done
# The same JSON through Server's proxy, which accepts the source name:
curl --noproxy '*' --fail -H 'content-type: application/json' \
  -d '{"operation":"insert","element":{"type":"node","id":"reading-6","labels":["Reading"],"properties":{"value":6}}}' \
  "$API/sources/observations/push"
```

Then inspect the pipeline and its metrics:

```sh
curl --noproxy '*' --fail "$API/sources"
curl --noproxy '*' --fail "$API/queries"
curl --noproxy '*' --fail "$API/reactions"
curl --noproxy '*' --fail "$API/queries/observed-values/results"
curl --noproxy '*' --fail "$API/queries/high-readings/results"
curl --noproxy '*' --fail http://127.0.0.1:18081/metrics
```

The listings report each component's assigned UUID as its `id`, next to its
name. In `/metrics`, look for:

| Prometheus family | What to expect |
|---|---|
| `drasi_source_events_dispatched` | Counter that grows with each change, labelled with the source's ID |
| `drasi_query_events_processed` | Counter with `phase="steady"` that grows for each query (`query_id`) |
| `drasi_query_results_emitted` | Counter per query by `change_kind`, for example `add` for new readings |
| `drasi_reaction_results_processed` | Counter for the reaction and query; stays 0 with the log reaction (see [expected quirks](#grafana-the-dashboard)) |
| `drasi_query_engine_duration` | Histogram per query, in seconds, whose `_count` grows |
| `drasi_query_commit_duration` | Histogram, only with the `persistence` profile |
| `drasi_queue_depth`, `drasi_queue_depth_max` | Gauges, one series per queue (`queue_id`): an input and an output queue for each query |

Scrape twice with input in between to see counters grow; in-flight work can lag
briefly. Histogram bucket boundaries are listed in
[telemetry configuration](../../docs/telemetry-configuration.md#metric-data-and-bounds).

#### Compare profiles and filters

Stop Server with Ctrl+C between runs:

```sh
export DRASI_DEMO_TRUST_LOCAL_PLUGINS=1
DRASI_DEMO_PROFILE=basic bash examples/observability/demo.sh live prometheus
DRASI_DEMO_PROFILE=debug bash examples/observability/demo.sh live prometheus
DRASI_DEMO_PROFILE=persistence bash examples/observability/demo.sh live prometheus
# Drop one metric family without affecting processing:
DRASI_DEMO_EXCLUDE=drasi.query.engine.duration \
  bash examples/observability/demo.sh live prometheus
```

- `basic` and `debug` both include the query engine metrics; `debug` does not
  add the `persistence` commit metrics.
- `persistence` adds commit, outbox and recovery metrics such as
  `drasi_query_commit_duration`, but not every `debug` detail.
- With the exclusion, every `drasi_query_engine_duration` series disappears
  while the other counters keep growing. `DRASI_DEMO_EXCLUDE` replaces the
  default exclusion, `drasi.plugin.`; it is a literal substring, not a pattern.
- `drasi_query_progress_sequence` never appears in Prometheus: progress is
  exported only over OTLP. `drasi_pipeline_end_to_end_duration` does not appear
  either, because no profile turns on handled-branch latency.
- The `process_*` families appear by default and disappear with
  `DRASI_DEMO_PROCESS_METRICS=false`.

#### Process metrics and inspection

The `live` modes turn both on by default. While the pipeline runs:

```sh
curl --noproxy '*' -s http://127.0.0.1:18081/api/v1/telemetry
curl --noproxy '*' -s http://127.0.0.1:18081/metrics | grep '^process_'
curl --noproxy '*' -s http://127.0.0.1:18081/api/v1/telemetry/metrics | grep '^process_'
```

With the `live` defaults and no trace endpoint, `/api/v1/telemetry` returns:

```json
{"profile":"persistence","tracing":{"exportEnabled":false,"sampleRatio":1.0,"localInspection":false},"metrics":{"backend":"prometheus","processMetrics":true,"path":"/api/v1/telemetry/metrics"},"logs":{"pathTemplate":"/api/v1/instances/{instanceId}/{collection}/{componentId}/logs","streamPathTemplate":"/api/v1/instances/{instanceId}/{collection}/{componentId}/logs/stream"}}
```

The settings contain no endpoint, header or credential. Both metrics routes
return the same `process_*` families, without labels, next to the `drasi_*`
families: `process_cpu_seconds_total` (a fractional counter), resident and
virtual memory bytes, open and maximum file descriptors, threads and start
time. They describe the whole Server process. `process_max_fds` is omitted when
the soft limit is unlimited, and a reading that is unavailable is omitted rather
than reported as `0`. `DRASI_DEMO_EXCLUDE=process_` removes them, and
`DRASI_DEMO_INSPECTION=false` makes both inspection routes return 404.

### Metrics over OTLP

Start an OTLP receiver you already have that accepts **HTTP/protobuf metrics**,
or the [OpenTelemetry Collector](#send-to-an-opentelemetry-collector) below,
then:

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 \
DRASI_DEMO_METRICS_ENDPOINT=http://127.0.0.1:4318/v1/metrics \
DRASI_DEMO_EXPORT_INTERVAL=2 DRASI_DEMO_EXPORT_TIMEOUT=1 \
  bash examples/observability/demo.sh live otlp
```

Server now pushes metrics every 2 seconds instead of serving them, so
`/metrics` returns 404, `/api/v1/telemetry` reports backend `otlp` with
`path: null`, and `/api/v1/telemetry/metrics` returns 503
`TELEMETRY_METRICS_UNAVAILABLE`. Your receiver gets:

- The pipeline metrics under the scope `drasi.core`, with dotted names (for
  example `drasi.query.engine.duration`): cumulative counters, gauges and
  explicit-bucket histograms with units `1` or `s`, and the same identity
  attributes. No input text, result values or credentials appear in
  attributes.
- One gauge, `drasi.query.progress.sequence`, under the scope `drasi.progress`.
  Its points have `drasi.progress.stage` set to `output.produced`, or to
  `input.committed` together with `drasi.input.kind` and `drasi.input.id`, and
  carry the instance and query IDs. Values grow as input is committed and
  output produced. Queries that are stopped or not ready are omitted, not
  reported as zero. `DRASI_DEMO_EXCLUDE=drasi.query.progress.` removes it.
- With process metrics on, up to seven `process_*` metrics, without
  attributes, under the scope `drasi.server.process`, sampled just before each
  export.

All scopes share one resource: `service.name=drasi-server-observability`, the
Server version, the process ID and a `service.instance.id` that changes on every
restart. The profile and exclusion variants above work with `otlp` too.

The launcher starts no receiver. Plain HTTP is for local development only. For
an authenticated receiver, use a private copy of the configuration with
headers taken from environment variables and an **HTTPS** endpoint; never write
resolved tokens into the file. Server follows no redirects, and `OTEL_*`
environment variables do not override these settings. `--skip-verification`
concerns plugin signatures and has nothing to do with TLS.

### Traces to your own receiver

Any receiver that accepts **HTTP/protobuf traces** works, with either metrics
backend:

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 \
DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
DRASI_DEMO_SAMPLE_RATIO=1.0 DRASI_DEMO_PROFILE=debug \
  bash examples/observability/demo.sh live prometheus
```

Send input as in [`live`](#the-http-pipeline-with-prometheus-live). Each change
produces the spans described in [Jaeger](#jaeger-follow-one-change), with the
same resource identity as OTLP metrics. A `reaction.process` span appears only
for reactions that report their work through Core's reaction worker; the log
reaction does not. Spans completed inside native plugins keep their original
IDs, parents, events and timestamps.

- `DRASI_DEMO_SAMPLE_RATIO=0` starts no new traces, while metrics, results and
  logs continue. A sampled incoming parent is still honored, and an unsampled
  one is never resampled.
- Without a trace endpoint, no trace exporter starts.
- Export is bounded (2,048 queued spans or 16 MiB, batches of 256, one request
  at a time, no retries). If the receiver is down, batches are dropped with a
  diagnostic and processing never blocks.

**Incoming trace context.** You can send a W3C `traceparent` with a change:

```sh
curl --noproxy '*' --fail -H 'content-type: application/json' \
  -H 'traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01' \
  -d '{"operation":"insert","element":{"type":"node","id":"traced-1","labels":["Reading"],"properties":{"value":7}}}' \
  "http://127.0.0.1:18081/api/v1/instances/$INSTANCE/sources/observations/push"
```

The proxy forwards only a valid `traceparent` and `tracestate` (never baggage
or credentials) and starts no span of its own; invalid headers are dropped
without rejecting the change. This example's HTTP source is an ordinary
(non-native) plugin, and context does not cross that plugin boundary, so the
change still starts a new trace rather than joining yours. Native
`drasi.network` HTTP and gRPC sources do carry incoming context into the
pipeline.

### A PostgreSQL CDC source: `live-postgres`

This variant reads changes from a PostgreSQL table you provide. The launcher
never starts, creates or deletes a database, replication slot or publication.
Use an isolated database with logical replication enabled and a table such as:

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

The launcher refuses to start until host, database, user and password are set.
`DRASI_DEMO_PG_PORT`, `DRASI_DEMO_PG_SLOT` and `DRASI_DEMO_PG_PUBLICATION`
default to `5432`, `drasi_observability_slot` and `drasi_observability_pub`.
Credentials stay environment references; they are never written to build
receipts or run directories. Insert rows (for example
`INSERT INTO observation VALUES (1, 10);`) and inspect results, metrics and
traces as above. Each change starts a new trace; it does not inherit the SQL
client's trace. Remove the slot and publication yourself when you finish.

### Send to an OpenTelemetry Collector

A local OpenTelemetry Collector can stand in for your own observability tool as
the OTLP receiver. It needs no containers and installs nothing system-wide.
Download the `otelcol-contrib` release archive for your platform into the
ignored `target/observability-demo-tools/` directory, and check its checksum
before unpacking. The commands below use v0.165.0 for macOS on Apple silicon;
set `VERSION` and `PLATFORM` for yours. Recent releases publish an
`<archive>.sha256` file per archive; older ones publish one combined
`opentelemetry-collector-releases_otelcol-contrib_checksums.txt`.

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

If the checksum does not match, do not unpack or run the archive.

[`otel-collector.yaml`](otel-collector.yaml) receives OTLP/HTTP on
`127.0.0.1:4318` (`/v1/metrics` and `/v1/traces`). It sends metrics to the
`debug` exporter and to a `prometheus` exporter on `127.0.0.1:8889`; traces go
to `debug` only. It has no credentials or TLS, so use it for local development
only. To adapt it, copy it into `target/observability-demo-tools/`: any new or
edited file in this folder other than Markdown changes the bundle's source
fingerprint, and the next live start then requires `build-plugins` again.

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

Send a few changes (`bash examples/observability/demo.sh send 5`), then check:

```sh
curl --noproxy '*' -s http://127.0.0.1:18081/api/v1/telemetry
curl --noproxy '*' -s -w '\nHTTP %{http_code}\n' http://127.0.0.1:18081/api/v1/telemetry/metrics
curl --noproxy '*' -s http://127.0.0.1:8889/metrics | grep -E '^(process_|drasi_)'
sleep 10
curl --noproxy '*' -s http://127.0.0.1:8889/metrics | grep '^process_cpu_seconds_total'
```

- **Settings:** `/api/v1/telemetry` shows `"exportEnabled":true`,
  `"backend":"otlp"`, `"processMetrics":true` and `"path":null`, and
  `/api/v1/telemetry/metrics` returns 503 because OTLP has no local read. Read
  the metrics from the Collector instead.
- **Collector scrape:** the `process_*` families appear next to `drasi_*`, and
  `process_cpu_seconds_total` grows between the two scrapes. The Collector's
  Prometheus exporter renames Drasi series as it re-exposes them: gauges with
  unit `1` get `_ratio` (for example `drasi_queue_depth_ratio`), counters get
  `_total` and duration histograms get `_seconds`. The `process_*` names arrive
  unchanged. It also adds `otel_scope_name` (`drasi.server.process`,
  `drasi.core` or `drasi.progress`). Server's own names are unchanged; the
  debug output and Server's Prometheus backend show them.
- **Shared identity:** all series carry the same `instance` (the resource's
  `service.instance.id`) and `job="drasi-server-observability"`
  (`service.name`), and `target_info` carries `process_pid` and
  `service_version`. A Prometheus that scrapes `:8889` renames these labels to
  `exported_job` and `exported_instance` unless its scrape job sets
  `honor_labels: true`.
- **Collector log:** each metrics export logs the resource
  (`service.name=drasi-server-observability`, `service.version`,
  `process.pid`, `service.instance.id`), then the `drasi.server.process`,
  `drasi.core` and `drasi.progress` scopes. Traces log the `drasi.core` scope
  with `source.publish`, `query.process` and `sink.handle` spans.
- **In your own tool:** point a Prometheus-compatible scraper at
  `http://127.0.0.1:8889/metrics`, or set the two endpoint variables to your
  tool's OTLP/HTTP protobuf URLs to export to it directly.

Stop Server with Ctrl+C first, then the Collector. The Collector keeps serving
the last values for a few minutes after Server stops; those are retained
samples, not new exports. Confirm that the ports are closed, for example with
`lsof -nP -iTCP:4318 -iTCP:8889 -iTCP:18081 -iTCP:18090 -sTCP:LISTEN`.

## How the launcher works

### Local plugin bundle and trust

Server loads sources, queries and reactions as plugins. `build-plugins` builds
Server and exactly the three plugins that one scenario needs, with their
`dynamic-plugin` features, from your current Server and `../drasi-core`
source, including uncommitted edits:

- `drasi-source-http` (or `drasi-source-postgres`)
- `drasi-computation-transformer-query`
- `drasi-reaction-log`

Query bootstrap is off, so no bootstrap plugin is needed. Stop any running
instance of this example before rebuilding. The build runs offline and runs no
tests. It uses its own target directories and publishes nothing to a shared
plugin folder:

- Server: `target/observability-server`
- Core plugins: `target/observability-plugins`
- The example's copies: `target/observability-example/bundles/<scenario>/`

The build writes a receipt, `target/observability-example/<scenario>-build.json`,
recording the plugin set, source fingerprints (including uncommitted files),
compiler and binary hashes. Every `live` or `showcase` start checks it and
refuses if the receipt is missing or from another scenario, a binary was
replaced, or Server or Core source changed; rerun `build-plugins` to fix it.
The receipt is a local consistency check, not a cryptographic supply-chain
attestation. Server still runs its normal metadata, ABI and capability checks
when it loads the plugins.

Locally built plugins are unsigned. The `live` and `showcase` modes therefore
require `DRASI_DEMO_TRUST_LOCAL_PLUGINS=1`, and only then pass
`--skip-verification` for the three checked files. This is a development trust
decision, not signature verification. Never use it for plugins you did not
build yourself. `start` keeps signature verification on.

### Run directories and state

Each `live` or `showcase` start creates a new directory,
`target/observability-example/runs/run-<random>/`, and runs Server from it.
The directory name is also the instance ID. It holds:

- `demo-owner.json`, which marks it as this example's;
- copies of the three checked plugins;
- the write-ahead log, under `data/<hex instance key>/wal/` (it is always on;
  `persistConfig: false` does not turn it off);
- for the showcase, a `showcase/` folder with the Server and generator logs,
  container ID files and the generated Prometheus target file.

A new directory and `persistIndex: false` on each run keep old index or log
data from being reused. Nothing is deleted when a run ends, so you can inspect
it afterwards. Remove only directories that contain `demo-owner.json`, and never
a generic `data/` directory. A long-running example keeps accumulating
results and state, so keep runs short.

### Stopping

The first Ctrl+C (or SIGTERM) stops new work and starts an orderly shutdown;
repeating it does not extend the time allowed. Telemetry gets up to 5 seconds
to flush, within an overall 30-second wait. Wait for the process to exit and
the ports to close before starting another run.

### Showcase containers

The showcase creates, on its own podman network
`drasi-server-observability-showcase` and with the label
`io.drasi.demo=server-observability-showcase`:

| Container | Pinned image | Published on 127.0.0.1 |
|---|---|---|
| `drasi-server-observability-showcase-jaeger` | `cr.jaegertracing.io/jaegertracing/jaeger:2.22.0` | 4318 (OTLP/HTTP from Server), 16686 (UI) |
| `drasi-server-observability-showcase-prometheus` | `docker.io/prom/prometheus:v3.16.0` | 9090 |
| `drasi-server-observability-showcase-grafana` | `docker.io/grafana/grafana:13.3.1` | 3000 |

Prometheus keeps one day of data inside its container. Before creating anything,
the launcher checks that podman works, that every port it needs is free, and
that none of its containers or its network already exists. It never replaces or
removes anything it did not create in that run.

The input generator, [`showcase/traffic.py`](showcase/traffic.py), posts one
change about every 2 seconds through Server's source-data proxy,
`POST /api/v1/instances/{instance}/sources/observations/push`. It inserts ten
readings (`reading-1` … `reading-10`), then only updates them, so the queries'
results stay small.

## Environment variables

The configuration files read these through Drasi Server's normal
`${VAR:-default}` substitution. A value already set in your environment wins
over the default; unset it to return to the default. Server also loads an
optional `.env` file without overriding existing variables; `live` and
`showcase` runs start in a new directory that has none.

| Variable | `start` / `validate` | `live` / `live-postgres` | `showcase` |
|---|---|---|---|
| `DRASI_DEMO_TRUST_LOCAL_PLUGINS` | not used | must be `1` | must be `1` |
| `DRASI_DEMO_PORT` (management API, `/metrics`; also used by `send`) | `18081` | `18081` | `18081` |
| `DRASI_DEMO_PROFILE` | `basic` | `persistence` | `debug` |
| `DRASI_DEMO_METRICS_BACKEND` | `none` | the command's argument, default `prometheus` | always `prometheus` |
| `DRASI_DEMO_METRICS_ENDPOINT` | `http://127.0.0.1:4318/v1/metrics` | same | not used |
| `DRASI_DEMO_EXPORT_INTERVAL` (seconds) | `30` | `2` | not used |
| `DRASI_DEMO_EXPORT_TIMEOUT` (seconds) | `5` | `1` | not used |
| `DRASI_DEMO_PROCESS_METRICS` | not used (off) | `true` | always `true` |
| `DRASI_DEMO_INSPECTION` | not used (Server's default: settings route only) | `true` | `true` |
| `DRASI_DEMO_TRACE_ENDPOINT` | empty (no trace export) | empty | always the showcase's Jaeger |
| `DRASI_DEMO_SAMPLE_RATIO` | `0.1` | `1.0` | `1.0` |
| `DRASI_DEMO_EXCLUDE` | not used | `drasi.plugin.` | `drasi.plugin.` |
| `DRASI_DEMO_SHUTDOWN_TIMEOUT` (seconds) | `5` | `5` | `5` |
| `DRASI_DEMO_SOURCE_PORT` (HTTP source) | not used | `18090` (`live`) | `18090` |
| `DRASI_DEMO_OTLP_PORT` (Jaeger OTLP/HTTP) | not used | not used | `4318` |
| `DRASI_DEMO_JAEGER_PORT` (Jaeger UI) | not used | not used | `16686` |
| `DRASI_DEMO_PROMETHEUS_PORT` | not used | not used | `9090` |
| `DRASI_DEMO_GRAFANA_PORT` | not used | not used | `3000` |
| `DRASI_DEMO_PG_*` | not used | `live-postgres` only; see [PostgreSQL](#a-postgresql-cdc-source-live-postgres) | not used |
| `DRASI_DEMO_INSTANCE` | not used | set by the launcher | set by the launcher |

The showcase ports must all differ. The dashboard's own link to the Jaeger UI
assumes the default port; the links the launcher prints always use the actual
ports.

OTLP export interval and timeout are in seconds, from 1 to 3,600 and 1 to 30.
The older root-level `metrics` section of a Server configuration uses HTTP/JSON
and milliseconds instead; a configuration cannot use both.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `live` or `showcase` exits asking for `DRASI_DEMO_TRUST_LOCAL_PLUGINS=1` | Set it to confirm that you trust the plugins you built locally. |
| Start refuses the build receipt | Server or Core source, or the Server binary, changed since the last `build-plugins`; another `cargo build` or `cargo test` can replace the binary. Rerun `bash examples/observability/demo.sh build-plugins`. |
| The build fails on a missing dependency | It runs offline. Fetch dependencies with network access first, for example with `cargo fetch` in this repository and in `../drasi-core`. |
| `showcase requires a working podman` | Start podman; on macOS, `podman machine start`. |
| `Ports already in use: …` | Another program or an earlier run holds the port. Stop it, or choose other ports with the `DRASI_DEMO_*_PORT` variables. |
| `Container … already exists` or `Podman network … already exists` | An earlier run was killed before cleanup. Check that it is a leftover, then remove it yourself, for example `podman rm -f drasi-server-observability-showcase-jaeger` and `podman network rm drasi-server-observability-showcase`. |
| `send` reports that no demo answered | Start the showcase or a `live` mode first. If you changed the port, set `DRASI_DEMO_PORT` for `send` too. |
| Prometheus's endpoint link does not open | Expected: `host.containers.internal` exists only inside the containers. Open `http://127.0.0.1:18081/metrics`. |
| Grafana's Jaeger data source reports 404 | Expected; see the [expected quirks](#grafana-the-dashboard). Trace lookup by ID still works. |
| A panel stays empty | Give it a scrape or two (5 seconds each). If it is still empty, check `target/observability-example/runs/<instance>/showcase/server.log`. |

## Security notes

- Everything binds to 127.0.0.1. `/metrics` and the inspection API have no
  authentication; anyone who can reach the management port can read them.
  CORS is not authentication. In production, put scraping behind a trusted
  proxy or a restricted network.
- Grafana allows anonymous read-only access and has no admin user. No
  configuration file in this example contains credentials.
- `DRASI_DEMO_TRUST_LOCAL_PLUGINS=1` skips signature verification for the three
  plugins you built. Never use it for plugins from anywhere else.
- Plain-HTTP OTLP is for local development. Use HTTPS, with headers from
  environment variables, for any real receiver.
- The PostgreSQL password stays an environment reference and is never written
  to disk by the launcher.

## Further reading

- [Telemetry configuration](../../docs/telemetry-configuration.md): every
  setting, limit and metric bound, plus ownership and startup behavior.
- [Observability baseline](../../docs/observability-baseline.md): implementation
  status, what has been verified and what remains open.
