# Native MySQL consumer

This example runs one native MySQL source, two real continuous queries, their
query-owned initial snapshots, and a native stored-procedure sink inside one
Server instance and ComputationGraph. The queries have different filters; the
source binds both actual progress owners rather than creating one producer per
query or synthesizing an aggregate checkpoint.

`server.yaml` is generated from `recipe.json` by `examples/native-mysql.rs`.
The generator loads the matching native libraries and obtains their factory
identities, configuration versions and port contracts. Do not edit the generated
definition by hand. This example uses the canonical `computation.definition`
form because its source, bootstrappers and sink bind secret configuration
resources; it does not introduce a readable-graph reference syntax.

## Build and generate

Use the sibling Core checkout matching this Server and its SDK. Independently
built libraries are required; rebuilding Server alone does not update them.
From `drasi-server`, with the shared `CARGO_TARGET_DIR` pointing at its `target`:

```sh
cargo build --offline --locked --manifest-path ../drasi-core/Cargo.toml \
  -p drasi-computation-source-mysql \
  -p drasi-computation-bootstrap-mysql \
  -p drasi-computation-sink-storedproc-mysql --features dynamic-plugin
cargo build --offline --locked --example native-mysql --bin drasi-server
target/debug/examples/native-mysql generate target/debug \
  examples/mysql/recipe.json examples/mysql/server.yaml
```

The native libraries reject literal secret settings in a canonical definition.
The generated configuration uses `env-json:DRASI_MYSQL_SOURCE_SETTINGS` and
`env-json:DRASI_MYSQL_SINK_SETTINGS` through the real configuration resource.
Both environment variables contain JSON objects, resolved at construction, not
credentials checked into the graph.

The source settings are the native `MySqlConfig`: explicitly selected `items`,
ordinary `changes`, `replay: server`, and an opt-in coordinated
`start: {mode: snapshot, lock_timeout_ms: 5000}`. Each query has its own
`sourceProgress` and `nativeBootstrap` resource; the source has the corresponding
fixed `consumerProgress` set. The sink settings bind the `all-items` and
`selected-items` producers to their output streams and use parameter-only templates such as:

```text
CALL effect({{param output.alias}},{{param operation}},{{param after.id}},{{param after.value}})
```

See the [source](../../../drasi-core/components/computation-plugins/sources/mysql/README.md),
[bootstrap](../../../drasi-core/components/computation-plugins/bootstrappers/mysql/README.md)
and [sink](../../../drasi-core/components/computation-plugins/sinks/storedproc-mysql/README.md)
family guides for full connection, retention, locking and delivery contracts.

## Isolated end-to-end qualification

The wrapper provisions only a new, uniquely named Podman `mysql:8.4` container.
It creates its own database, tables and procedure; uses loopback-only ephemeral
ports, a random test password, a new working directory and copied matching
libraries; and removes only its recorded container ID and owned anonymous
database volumes. It records exact mount/volume ownership in
`resource-ownership.json` before removal and verifies that those volumes are gone.
It does not read demo
environment files, reuse installed databases, reset binlogs, or alter retention.
An existing evidence directory is rejected.

```sh
python3 examples/mysql/qualify.py \
  --server target/debug/drasi-server \
  --generator target/debug/examples/native-mysql \
  --plugins target/debug \
  --evidence /tmp/drasi-mysql-qualification-unique
```

The wrapper checks reproducible generation against the checked-in definition,
both initial query results, multi-statement update/insert/delete fan-out,
parameterized procedure effects, rollback exclusion with a committed barrier,
offline writes and fresh memory initialization after a Server restart, resumed
delivery, and server-observed connection cleanup. Evidence includes library
hashes, non-secret generated configurations, Server logs and observations.
The Core Host test `native_mysql_components` separately exercises static and
rebuilt dynamic libraries on current-thread and multithreaded Tokio runtimes.

The generator's integration tests also run the real Server configuration
resolver and HTTP API on both runtime flavors, with all three dynamically loaded
roles. They check catalog inspection, exact query and procedure results,
rollback exclusion, configuration persistence without resolved secrets, offline
writes, reconstruction and server-observed session cleanup:

```sh
cargo test --offline --locked --example native-mysql -- \
  --include-ignored --test-threads=1
```

These cases require the freshly rebuilt libraries and Podman; the command
explicitly includes them rather than treating skipped integration cases as proof.
The paired identity cases add the canonical `nativeIdentity` resource and a
configured secret reference. They change the actual MySQL password, restart the
same sink, verify fresh secret resolution without constructor reads, preserve
the original configuration, and reconstruct the Server with the rotated secret.

The Host matrix also checks repeated fixture runs and failed database startup
without leaking owned volume count or data. The CLI failure-path regression is:

```sh
python3 -B -m unittest discover -s examples/mysql -p test_qualify.py -v
```

Set `DRASI_MYSQL_FIXTURE_RECORDS` to retain fixture ownership records in a chosen
evidence directory. These checks never prune unrelated volumes or images.

## Boundaries

These are ordinary in-memory queries, not whole-transaction or durable input-pipe
qualification. Each row may be observed separately. Source-specific row positions
support safe partial replay while required binlog history remains; no retention
setting is changed. Coordinated initialization requires InnoDB and explicit
permission for the brief server-wide read lock.

Initial snapshots populate the real query results, not a replay of historical
procedure calls. The native sink reports `Handled` only after its submitted
commands finish, but has no atomic destination cursor or exactly-once promise.
A failed procedure may already have effects; a submitted call is not blindly
retried. Memory restart reconstructs query state, not external destination state.

The native source exposes its actual captured catalog through DrasiLib's shared
source-schema reader, independently of pending replication reads. Factory
configuration schema endpoints are not database schema discovery. Native cursor
formats and the older legacy row tokens are intentionally distinct. This example
does not claim GTID failover, MariaDB support, or cold persistent-input-pipe restart.
The checked-in recipe uses constructor-resolved username/password settings.
Provider-based consumers instead bind the optional native `identity` dependency;
the integration cases above qualify that per-start refresh path separately.
