# Native PostgreSQL consumers

These examples use one native PostgreSQL row source per database stream, with
the actual query owners bound through `consumerProgress`. Each query initializes
through `drasi.postgres/shared-row-bootstrap`; streaming uses
`drasi.postgres/shared-changes`. Both run inside the Server instance's one
ComputationGraph.

The active configurations use **canonical `computation.definition`**, generated
from the loaded factories' real metadata. This form is required because the
examples bind secret configuration resources. Do not replace references with
credential literals or invent a readable-graph reference syntax.

## Build the matching local runtime

Use the current sibling Core checkout and the same target directory for the
Server and separately built libraries. Path dependencies do not rebuild plugins.
The native recipes require this matching local build, not registry fallback.

From `drasi-server`:

```bash
export CARGO_TARGET_DIR="$PWD/target"
cargo build --locked --bin drasi-server --example native-postgres
(
  cd ../drasi-core
  cargo build --locked --lib --features dynamic-plugin \
    -p drasi-computation-source-postgres \
    -p drasi-computation-bootstrap-postgres \
    -p drasi-computation-source-http \
    -p drasi-computation-bootstrap-http \
    -p drasi-reaction-log -p drasi-reaction-http -p drasi-reaction-sse
)
```

Use `--offline` when dependencies are already available. Follow the repository's
normal platform/toolchain setup, including its jq library settings on macOS.
Build UI assets with `make build-ui` if you also need the embedded Server UI.
The existing log, HTTP and SSE output adapters remain connected through the
native query-results outlet; these examples do not add a second execution engine.

## Generate and run a configuration

Files in `recipes/` are **generation inputs**, not alternative runtime
configurations. They retain the application's query text, language, source
subscriptions, synthetic joins and existing output configuration.

| Input recipe | Generated active configuration |
|---|---|
| `postgres-cdc-complete.yaml` | `../configs/02-sources/postgres-cdc-complete.yaml` |
| `order-exception-handling.yaml` | `../configs/06-real-world-scenarios/order-exception-handling.yaml` |
| `getting-started-server.yaml` | `../getting-started/server-config.yaml` |
| `getting-started-step-{3,4,5,6}.yaml` | `../getting-started/configs/getting-started-step-{3,4,5,6}.yaml` |

For example, from the Server root:

```bash
target/debug/examples/native-postgres generate target/debug \
  examples/postgres/recipes/getting-started-server.yaml \
  examples/getting-started/server-config.yaml

# Supply the real database credentials/connection overrides to the recipe.
export POSTGRES_PASSWORD='your-configured-password'
export DRASI_PG_POSTGRES_MESSAGES_SETTINGS="$(
  target/debug/examples/native-postgres settings \
    examples/postgres/recipes/getting-started-server.yaml postgres-messages
)"
target/debug/drasi-server \
  --config examples/getting-started/server-config.yaml \
  --plugins-dir target/debug --skip-verification
```

Use `--skip-verification` only for the deliberately matched local build.
Settings keys are `DRASI_PG_<UPPERCASE_SOURCE_ID_WITH_UNDERSCORES>_SETTINGS`.
The generator's `settings <recipe> <source-id>` command resolves the recipe's
existing environment settings and emits native settings JSON. Keep that JSON
in the process environment or a suitable secret store, not in committed YAML.
For the numbered steps, the source is `my-postgres` and the key is
`DRASI_PG_MY_POSTGRES_SETTINGS`. CDC-complete uses `app-database`; order exceptions
use the source ID declared in their recipe.

A canonical configuration reference looks like:

```json
{
  "Reference": {
    "resource": "postgres-configuration",
    "key": "env-json:DRASI_PG_POSTGRES_MESSAGES_SETTINGS",
    "secret": true
  }
}
```

The configured publication and logical slot must already exist and cover the
selected tables. Bootstrap opens a separately owned temporary exported snapshot;
it does not reset, recreate, drop or assume managed ownership of the configured
slot. Missing initialization, unavailable WAL, changed bindings and incomplete
consumer membership are errors, not permission to start a consumer at the head.

## Mixed PostgreSQL/HTTP initialization

Trading keeps its queries in `../trading/app/src/services/queries.ts`. Its export
script combines those declarations with `server/trading-native-recipe.json` and
the actual PG/HTTP factory metadata:

```bash
cd examples/trading/app
node --import tsx scripts/export-native-recipe.ts \
  ../../../target/debug/examples/native-postgres ../../../target/debug
```

This writes `app/public/native-computation.json`. The app installs the complete
fixed-member source/query group with one existing
`POST /api/v1/instances/{instanceId}/computation/components` request, then creates
its SSE output. It does not create one PostgreSQL source per query or silently
repair a partial group. Existing definitions, references and consumer bindings
must match; membership changes require an explicit group replacement.

Queries with both PostgreSQL and HTTP inputs use this resource recipe:

```json
{
  "kind": "compositeBootstrap",
  "component": "portfolio-query",
  "providers": [
    "portfolio-query-postgres-stocks-bootstrap",
    "portfolio-query-price-feed-bootstrap"
  ]
}
```

Declare **exactly** those ordered, distinct providers as Bootstrap resource
dependencies. Bind the composite to the query's single `bootstrap` slot.
Every child must belong to that query. Recovery-bearing children must share
its exact actual progress reader, which the composite returns unchanged.

Composition supports 1–32 children and at most 256 distinct watermarks. A bounded,
version-1 handover record gives each resource ID its own opaque state namespace;
the total serialized record remains within the query's 65,536-byte bound.
Retained membership/order cannot be reinterpreted. Snapshots stream in declared
order without collecting all rows. Child completion metadata and watermarks
commit together through the existing query transaction. No persistence is
fabricated for a memory query.

`ResetRequired` dominates `RefreshVolatile`, which dominates `Ready`. A non-Ready
aggregate stops/rearms all children, including already-Ready native siblings,
before reset/reprepare. Cancellation or partial failure fences reuse until owned
cleanup succeeds. Retirement holds the actual child lifecycles; rejected partial
acquisitions resume them.

Trading's real API exposes the original 50-price JSONL seed at
`/api/initial-prices`. Native HTTP bootstrap reads it before live prices begin.
The price generator waits for all real queries to be Running. HTTP snapshots
have no atomic replay cursor: this application ordering is not a durable or
gap-free HTTP recovery protocol.

## Isolated end-to-end qualification

These commands require Podman and the built binaries/libraries above. They create
fresh databases, unique configured slots, loopback ports and new instance state,
then stop their processes and remove only their own resources. Evidence
directories must not already exist. No installed demo `.env`, reset script,
database, slot or state directory is used.

```bash
python3 examples/postgres/qualify.py \
  --server target/debug/drasi-server \
  --generator target/debug/examples/native-postgres --plugins target/debug \
  --evidence /tmp/native-postgres-proof

python3 examples/postgres/qualify-trading.py \
  --server target/debug/drasi-server \
  --generator target/debug/examples/native-postgres --plugins target/debug \
  --evidence /tmp/native-trading-proof
```

The first covers all seven generated PG-only configurations: snapshots, shared
live CDC, timed inactivity queries and order-exception HTTP output. The second
invokes the actual Trading app setup code, real Trading API, all eleven native
queries, live PostgreSQL/price joins, aggregates, rollback exclusion and real SSE.
It also verifies idempotent app setup, stopped readers and surviving configured
slots before deleting its isolated database container. The Trading API's Python
requirements and app dependencies must be installed; `--python` can select a
prepared virtual environment.

Core's focused static/dynamic composition proof is
`drasi-host-sdk --test native_postgres_http_bootstrap -- --ignored --test-threads=1`.
It runs on both Tokio flavors, including a held HTTP snapshot with concurrent
PostgreSQL writes, feedback fencing, live changes, memory reinitialization and
actual child retirement hold/resume. Build the four PG/HTTP native libraries first
and set `CARGO_TARGET_DIR` or `DRASI_NATIVE_PLUGIN_DIR`.

GPU Lab has its separate fresh native transaction/managed-slot proof:
[`embedded/ops/qualify-native-postgres.py`](../gpu-cluster-lab/embedded/ops/qualify-native-postgres.py).
These row examples do not substitute for whole-transaction delivery. Native
schema-discovery integration remains separate work, and cold reconstruction with
undrained persistent input pipes is not qualified by these memory/reconciliation
tests.
