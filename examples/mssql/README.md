# Native SQL Server consumer

**Implementation available; real SQL Server qualification remains blocked on the
current ARM64 Podman host. Do not treat this example as production-qualified.**

The generator uses the actual source, bootstrap and stored-procedure factory
metadata. It builds one CDC producer feeding two independent continuous queries
and an awaited stored-procedure sink. It does not call a legacy source/reaction
adapter or create a private graph.

Build all three native SQL Server libraries from the matching `drasi-core`
checkout with `dynamic-plugin`; use `make build-native-mssql-plugins` there.
Then run:

```sh
cargo run --example native-mssql -- target/debug examples/mssql/generated.yaml
```

The generated configuration contains secret references, not credentials:

* `DRASI_MSSQL_SETTINGS`: JSON matching the native source's `settings`:
  `connection` (host, port, database, user, password, tables, encryption,
  trust_server_certificate, poll_interval_ms), plus `barrier_table`.
* `DRASI_MSSQL_SINK_SETTINGS`: JSON containing the same `connection`,
  `producers: {"all": "all/out"}`, and `default_template` or `routes`. For example an added
  template is `EXEC dbo.ApplyEffect {{param operation}},{{param after.id}},{{param after.value}}`.

Use a database with CDC enabled and SQL Server Agent running. The example selects
`dbo.Items(id int primary key, value int)` and labels it `Items`. Its administrator
must also provision a separate CDC-enabled `dbo.DrasiBarrier(token uniqueidentifier
primary key)` table and configure `barrier_table: "dbo.DrasiBarrier"`.
Application credentials need selected-table and CDC SELECT/catalog visibility,
INSERT/DELETE on the barrier table, and EXECUTE on configured procedures. Normal
operation does **not** require sysadmin or permission to kill sessions.

The bootstrap holds table read locks, inserts a uniquely identified marker through
a separate connection, waits for CDC to capture it, reads the locked tables, then
commits. A sampled CDC maximum alone is not a consistent snapshot boundary. The
barrier blocks table writers for the bounded snapshot duration.

This example deliberately uses memory indexes: restart reconstructs both queries
from SQL Server. It does not imply persistent query state, once-only external
effects, automatic expired-CDC recovery, or cancellation of a running procedure.
Timeout/failed stop retains SQL work for cleanup; it is not a successful stop.
An interrupted procedure may already have committed effects and is not retried.

The real Host fixture is
`drasi-core/components/host-sdk/tests/native_mssql_components.rs`; it fails rather
than skipping when a real SQL Server cannot start. See the three native crate
READMEs for capability and qualification limits.

For an explicitly approved external SQL Server, the
[qualification fixture instructions](../../../drasi-core/components/host-sdk/tests/fixtures/native_mssql/README.md)
list every required connection/credential reference and DBA permission.
External mode requires an isolated pre-provisioned `drasi_native_...` database,
an ownership marker, and fresh owned schema namespaces. It never creates or drops
an external database. The newly authored extended scenarios are not qualified
until executed against that approved real environment.
