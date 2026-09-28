# Standalone project limitations and reuse gaps

`drasi-server forge` generates a source project, not a reduced Server executable.
Generation uses current Server DTOs, config validation, query mappings, and
bootstrap-reference resolution. The generated program calls component descriptors
directly, without depending on Server or its dynamic plugin host.

## Compatibility catalog

The catalog was aligned with drasi-core `32fc9052` and published versions on
2026-09-28: `drasi-lib =0.9.3`, `drasi-plugin-sdk =0.11.3`, and exact component
versions in `catalog.json`. It is data, not a dynamically discovered registry.
The local mode references the selected crates in a Core checkout; it requires
a checkout compatible with these APIs. Arbitrary older/newer checkouts are not
guaranteed to compile.

The here-traffic source/bootstrap and open511 bootstrap are not published.
The published open511 source and aws-sqs reaction target older, incompatible
Lib/SDK releases. The snapshot-test reaction is unpublished. These entries are
explicitly local-only: `--crates` fails before writing a project. Other unknown
kinds also fail rather than falling back to dynamic plugins. Application sources
have no descriptor and are not supported; generated projects have no Server
application API to drive them.

Server's OCI references, plugin version pins, hot reload, management API, Web UI,
and config persistence do not transfer to the generated runtime. A warning is
printed when explicit plugin references are replaced by catalog crate versions.
Crates.io mode has been checked with the representative mock/noop/log/file-secret/
password/redb/RocksDB pipeline, not every possible plugin combination.

**Upstream opportunity:** publish a release compatibility manifest containing
crate versions, descriptors, kind strings, and offline config schemas. This
would replace the hand-maintained catalog and improve per-field validation at
generation time. Currently plugin-specific validation happens in descriptors at
runtime, and native prerequisites vary by selected component.

## Config normalization and query fidelity

Exactly one instance is supported (root layout or one entry in `instances`).
Multiple instances are rejected. Top-level bootstrap definitions are resolved
using Server's helper, then separately instantiated for each referencing source.
Inline bootstrap configs retain the same source-config inheritance semantics.

Every mapped QueryConfig is serialized in full, including storage selection,
capacities, dispatch, bootstrap options, joins, middleware, and subscription
filters. This avoids losing non-default settings through a simplified builder,
but couples generated projects to a compatible Lib schema.

RocksDB is registered under Server's provider name `rocksdb` as the default when
`persistIndex` is enabled, retaining archive and memory-budget settings. Queries
can explicitly select that name or inline memory storage. Other named providers
and inline plugin backends are rejected because there is no compatible configured
provider (Lib does not support inline plugin construction). Sources get a redb
WAL just as in Server. WAL/index directories use Server's filesystem-safe instance
id encoding; state-store paths are independent.

**Upstream opportunity:** a lightweight shared configuration/factory crate could
replace this generated wiring. Importing Server at runtime would also import
its API, UI and dynamic loading dependencies, so it is deliberately avoided.

## Runtime overrides and secrets

The small override loader implements CLI > environment > file > embedded defaults.
Only instance id, logging and whole source/reaction config objects are overridable.
Topology, query text, bootstrap, identity and storage definitions remain compiled
in. Invalid flags, file fields and component ids fail explicitly.

Sealed output omits the loader, its direct serde/serde_yaml dependencies and the
sample override file. It is not hermetic: component environment/secret references,
secret files, data directories and external services are still runtime inputs.
Do not bake literal production credentials into generated source or executables.

Scalar environment/secret resolution reuses the SDK's async DtoMapper. Password
identities still need direct construction because they have no descriptor.
Secret stores are installed both on the Lib builder and on the SDK's global
resolver before constructing components. A small ValueResolver adapter is emitted
to avoid importing the heavyweight host SDK. Missing stores/secrets fail at
resolution rather than producing empty credentials.

**Upstream opportunity:** move the secret-store adapter and password descriptor
to a lightweight shared crate, and expose a reusable runtime override loader.

## Packaging

Generation does not require compiling or downloading plugins. `--build` explicitly
invokes `cargo build --release`; native dependencies must be installed separately.
Docker and CI files require portable dependencies or vendored local sources.
They include common native build prerequisites, not a guarantee that every
database/client library is available. The workflow uploads Linux/macOS artifacts;
it does not create a GitHub release or publish an image. Commit the generated
Cargo.lock after the first build and use `--locked` for reproducible builds.

Output directories must be empty to avoid overwriting user edits or leaving
nonsealed configuration files behind when generating sealed output.
