# Observability: configuration and an explicit live metrics pipeline

The default remains **configuration-only**: Off, no instances, no plugins.
The optional `live` mode uses a real local **mock source → native query → log
reaction** with exactly one backend, Prometheus or OTLP/HTTP protobuf. It does
not enable tracing, process collectors, inspection or handled-branch latency.
No dashboard, Docker or collector service is installed by this example.

These are reproducible instructions, **not a record of successful OBS-04 live
qualification**. See the [baseline](../../docs/observability-baseline.md) for
recorded results and outstanding qualification. Health alone proves no metrics.

## Prerequisites and unchanged default

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
Prometheus and metrics-only OTLP are now available:

```sh
DRASI_DEMO_PROFILE=basic DRASI_DEMO_METRICS_BACKEND=prometheus \
  bash examples/observability/demo.sh validate
DRASI_DEMO_PROFILE=persistence DRASI_DEMO_METRICS_BACKEND=otlp \
  bash examples/observability/demo.sh validate
```

Replacing `validate` with `start` enables the chosen backend, but still starts
**no pipeline**. Settings for the other backend do not activate it. Effective
tracing, process collection and inspection still reject startup explicitly:

```sh
DRASI_DEMO_PROFILE=debug \
  DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
  bash examples/observability/demo.sh start
DRASI_DEMO_INSPECTION=true bash examples/observability/demo.sh start
```

Invalid profiles, sample ratio `1.5`, interval `0`, timeout `31` and shutdown
budget `301` fail validation even if a destination is retained but inactive.
The wizard writes ignored `wizard-config.yaml`, refusing overwrite. Skip
telemetry to omit it, choose Off/inspection-disabled for logs only, or choose
one available metrics backend without tracing/process/inspection. The launcher
does not silently switch to wizard output.

## 1. Explicitly build the three matching local libraries

Stop other instances of this demo before rebuilding. This is the manual,
opt-in preparation command; neither ordinary startup nor live startup downloads
plugins or adopts an arbitrary existing executable:

```sh
python3 scripts/plugin_origin.py mode       # must print local
bash examples/observability/demo.sh build-plugins
```

The second command builds only Server plus `drasi-source-mock`,
`drasi-computation-transformer-query` and `drasi-reaction-log` with their
`dynamic-plugin` features. It uses the currently selected Core sources,
including local guard edits. Cargo runs offline; missing cached dependencies
are an explicit failure, not a registry fallback. It runs **no tests**.
Core and Server lockfile graphs use separate target directories:

- Server: `target/observability-server`
- Core libraries: `target/observability-plugins`
- Published local files: `target/debug/plugins/libdrasi_source_mock.dylib`,
  `libdrasi_computation_transformer_query.dylib`, `libdrasi_reaction_log.dylib`
  on macOS (`.so` on Linux).

`target/observability-example/local-build.json` records source fingerprints
(including dirty/untracked source), compiler identity and binary hashes **after
that build**. Live startup rejects a missing receipt, a missing/replaced binary
or changed Core/Server source; rerun the explicit build command. Matching names,
ABI versions or an old successful build are insufficient provenance. This is
a developer-trusted local receipt, not a cryptographic supply-chain attestation.
The host still performs its normal metadata/ABI/capability checks when loading.
No library is opened by the preparation helper. The local libraries are unsigned:
live mode requires `DRASI_DEMO_TRUST_LOCAL_PLUGINS=1` and then passes
`--skip-verification` for only the three receipt-checked local files. This is
an explicit development trust decision, not signature verification. Normal
configuration-only startup retains verification. Never use this opt-in for
untrusted or registry-downloaded libraries.
Another Cargo build/test can replace the Server executable with a different
feature set; rerun `build-plugins` if the receipt then rejects it.

## 2. Run and scrape the real pipeline

```sh
DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 bash examples/observability/demo.sh live prometheus
```

The launcher prints its unique instance and owned run directory. In another
terminal, after several log reaction results have appeared:

```sh
curl --noproxy '*' --fail http://127.0.0.1:18081/api/v1/instances
# Substitute the printed instance; list endpoints return assigned UUIDs + names.
INSTANCE=run-REPLACE_WITH_PRINTED_SUFFIX
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

Scrape twice a few seconds apart to establish growth. Allow asynchronous
in-flight differences; continuous mock input is not a one-event exact-count
fixture. Identity labels are `instance_id`, `graph_id`, `component_id` and the
specific source/query/reaction/queue labels, not guessed names or fabricated
zeros. Queue IDs name physical bindings; they need not be component UUIDs.
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
still grow. `drasi_query_progress_sequence`, `drasi_pipeline_end_to_end_duration`
and CPU/memory collector metrics must be absent in every Prometheus variant.
Progress is deliberately **OTLP-only**. The default exclusion is `drasi.plugin.`;
`DRASI_DEMO_EXCLUDE` replaces that one literal substring (not a glob).

## 3. Use an existing OTLP receiver instead

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
ID. Run the Basic/Debug/Persistence and exclusion variants above with `otlp`.
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

## State, ownership and environment

Each live start creates `target/observability-example/runs/run-<random>/` and
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
| `DRASI_DEMO_INSPECTION` / `TRACE_ENDPOINT` / `SAMPLE_RATIO` | `false` / empty / `0.1` | not used; disabled |
| `DRASI_DEMO_EXCLUDE` | not used | `drasi.plugin.` |
| `DRASI_DEMO_INSTANCE` | not used | launcher-owned unique run name |

The last combined row abbreviates `DRASI_DEMO_TRACE_ENDPOINT` and
`DRASI_DEMO_SAMPLE_RATIO`. These are ordinary `ConfigValue` references, not a
second environment system. Existing environment values win over defaults;
unset overrides to restore baseline behavior. The CLI loads optional `.env`
without overriding existing values; live mode runs in a new directory without
one. New OTLP interval/timeout ranges are 1..=3,600 / 1..=30 **seconds**; legacy
root `metrics` remains HTTP/JSON and **milliseconds**. Never combine both.

Loopback is intentional. There is no new authentication system; production
scraping needs a trusted proxy/restricted network, and CORS is not authentication.
See [telemetry configuration](../../docs/telemetry-configuration.md) for bounds,
scrape overload, ownership and startup rollback.
