# Observability configuration example

This is a runnable **configuration-first** example for OBS-02
([#235](https://github.com/drasi-project/drasi-server/issues/235)). It demonstrates
policy validation, environment overrides, the init wizard, and startup errors
for capabilities that are not implemented yet.

It is **not an end-to-end observability demo**. There are no instances, sources,
queries, reactions, collectors or dashboards. New telemetry runtime/exporter,
process-metric and inspection support will be integrated in later work. A health
response proves that the Server is responding, not that telemetry was exported.

## Prerequisites

- The Server's supported Rust toolchain and native build dependencies.
- The matching sibling `../drasi-core` checkout selected by Server's Cargo
  dependencies. See the [OBS-01 baseline](../../docs/observability-baseline.md).
- Bash. The optional HTTP checks below use curl.

No Docker, database, registry download, plugin build or external collector is
needed. The launcher builds the current Server with `--locked` into
`target/observability-server/` and uses an example-specific plugins directory
under `target/observability-example/`, initially empty. Signature verification
remains enabled; auto-installation and configuration persistence are disabled.
The launcher never changes branches or fetches a different Core checkout.

## 1. Start the Server

```bash
cd examples/observability
./demo.sh
```

By default, the Server runs in the foreground on **127.0.0.1:18081**, with
telemetry Off and UI disabled. In another terminal:

```bash
curl --noproxy '*' --fail http://127.0.0.1:18081/health
curl --noproxy '*' --fail http://127.0.0.1:18081/api/v1/instances
```

Expect HTTP 200 and an empty instance list. New telemetry routes are absent:

```bash
curl --noproxy '*' -i http://127.0.0.1:18081/metrics
curl --noproxy '*' -i http://127.0.0.1:18081/api/v1/telemetry
```

Both return HTTP 404. Stop this Server with **Ctrl+C** in its terminal.
No background processes or containers are started by the launcher.

To avoid another local listener:

```bash
DRASI_DEMO_PORT=18082 ./demo.sh
```

## 2. Validate configuration without starting a Server

These commands can run while the Server above remains running; validation does
not bind a port or contact a collector.

```bash
./demo.sh validate
```

The default policy is valid and requests no unavailable runtime capabilities.
Try Basic with no destinations:

```bash
DRASI_DEMO_PROFILE=basic ./demo.sh validate
```

This is also valid: a profile alone does not choose an exporter or backend.
It can start normally, but collects/exports no new telemetry.

## 3. Distinguish valid policy from runtime availability

```bash
DRASI_DEMO_PROFILE=basic DRASI_DEMO_METRICS_BACKEND=prometheus \
  ./demo.sh validate
```

Validation exits successfully and warns that Prometheus runtime support is
unavailable. The equivalent startup **fails explicitly** before binding a port:

```bash
DRASI_DEMO_PROFILE=basic DRASI_DEMO_METRICS_BACKEND=prometheus \
  ./demo.sh start
```

Other valid policies that currently produce capability warnings:

```bash
# Metrics-only OTLP; does not enable tracing.
DRASI_DEMO_PROFILE=persistence DRASI_DEMO_METRICS_BACKEND=otlp \
  ./demo.sh validate

# Tracing-only HTTP/protobuf; does not enable metrics.
DRASI_DEMO_PROFILE=debug \
  DRASI_DEMO_TRACE_ENDPOINT=http://127.0.0.1:4318/v1/traces \
  ./demo.sh validate

# Inspection remains independent, even when the profile is Off.
DRASI_DEMO_INSPECTION=true ./demo.sh validate
```

No collector is required: these commands validate settings only. Replacing
`validate` with `start` rejects these unavailable capabilities; it does not
silently ignore them. Settings on an unselected metrics backend never start a
second exporter.

## 4. Try invalid inputs

Each command below must exit nonzero with validation guidance:

```bash
DRASI_DEMO_SAMPLE_RATIO=1.5 ./demo.sh validate
DRASI_DEMO_PROFILE=verbose ./demo.sh validate
DRASI_DEMO_EXPORT_INTERVAL=0 ./demo.sh validate
DRASI_DEMO_SHUTDOWN_TIMEOUT=301 ./demo.sh validate
```

The interval is validated even while its OTLP backend is inactive. Valid ranges:
sampling ratio finite 0..=1; OTLP interval 1..=3,600 seconds; shared telemetry
shutdown budget 1..=300 seconds. New intervals are seconds, not the legacy root
`metrics` section's milliseconds.

## 5. Try the init wizard

```bash
./demo.sh init
```

The existing wizard writes ignored `wizard-config.yaml` and refuses to overwrite
an existing file. Select no sources/reactions or plugin installation for this
configuration-only walkthrough.

- Skip telemetry: the generated configuration omits its section.
- Choose Off: only inspection is offered; exporter/process questions are skipped.
- Choose another profile: select tracing and/or one metrics backend.
- Generated policy is validated before writing. Unavailable runtime choices
  are reported explicitly.

For runnable output now, choose **Off with inspection disabled**, or a non-Off
profile with no tracing endpoint, metrics backend None and inspection disabled.
The launcher always uses `server-config.yaml`; it does not silently switch to
wizard output. To validate the generated file from the repository root:

```bash
target/observability-server/debug/drasi-server validate \
  --config examples/observability/wizard-config.yaml \
  --plugins-dir target/observability-example/empty-plugins
```

## Environment and safety

`server-config.yaml` uses ordinary `ConfigValue` references, not a second
environment-configuration system. Overrides affect the process you launch;
they do not reconfigure a running Server. Restart to apply changes.

| Variable | Default |
|---|---|
| `DRASI_DEMO_PORT` | `18081` |
| `DRASI_DEMO_PROFILE` | `off` |
| `DRASI_DEMO_INSPECTION` | `false` |
| `DRASI_DEMO_TRACE_ENDPOINT` | empty |
| `DRASI_DEMO_SAMPLE_RATIO` | `0.1` |
| `DRASI_DEMO_METRICS_BACKEND` | `none` |
| `DRASI_DEMO_METRICS_ENDPOINT` | `http://127.0.0.1:4318/v1/metrics` |
| `DRASI_DEMO_EXPORT_INTERVAL` | `30` |
| `DRASI_DEMO_SHUTDOWN_TIMEOUT` | `5` |

Existing environment values override defaults, so unset any overrides when
returning to the baseline. The CLI also loads an optional local `.env`, without
overriding existing environment values.

Do not add real credentials to YAML or plaintext defaults. This example's
credential-free HTTP destinations are for loopback development only.
Authenticated OTLP requires HTTPS and environment-backed headers; custom
CA/mTLS and early plugin-backed Secret resolution are deferred. Do not combine
root `metrics` with any `telemetry` section.

The example binds loopback and implements no new authentication system. Do not
expose scrape/inspection endpoints to untrusted networks when later support is
added; protect them with a trusted proxy or restricted network.

See [telemetry configuration](../../docs/telemetry-configuration.md) for the full
policy contract. This example can grow into a real source/query/reaction and
collector demonstration as the later runtime/exporter issues land.
