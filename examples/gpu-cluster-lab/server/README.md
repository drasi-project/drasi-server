# GPU Cluster Lab: Drasi Server hosting

**Implementation in progress; not yet runnable.** This directory is for running the application
as a `gpu-demo` instance owned by the stock `drasi-server` executable, using
configuration and plugins rather than a custom embedded runtime.

Release packaging builds the actual `drasi-server` binary, PostgreSQL source and
bootstrap plugins, SSE reaction, and shared native GPU plugin from one exported
source manifest. It does not build or launch `gpu-runtime`. The control service
and React assets use the existing shared application build.
The sibling Core workspace's `Cargo.lock` is an explicit, fingerprinted build
input even though Core ignores it in Git. If it does not exist yet, generate it
with `cargo generate-lockfile --manifest-path ../../../../drasi-core/Cargo.toml`.

```sh
./demo stage
./demo build
```

`./demo build --staged` reuses the exported snapshot without reading newer
Core/Server changes into it.

Build finishes with an isolated package check, also available as
`./demo check-package`. It starts the stock executable with no host ports and no
external network, verifies the real native-plugin metadata endpoint lists all
six GPU factories, checks the standard PostgreSQL/bootstrap/SSE registrations
and image fingerprints, then requires graceful exit 0. Results are retained in
`.build/package-check.json`. This is a packaging gate, **not** GPU graph or live
scenario acceptance.

Staging and build artifacts belong to `server/.build/`. Images use a separate
`gpu-cluster-lab-server-<workspace-id>` prefix; these commands do not touch the
embedded installation's database, images, credentials or source export.
For the already implemented hosting path, see the
[embedded implementation](../embedded/README.md), which remains the behavior reference.
Package checks do not establish the health of an existing embedded deployment.

The lifecycle helper's process owner launches only the configured stock Server
child. Stop sends SIGINT to that owned PID, waits for exit 0, and reports a
nonzero exit or forced termination as an error. It has no DrasiLib dependency
and does not contain a second graph. Its focused tests run on macOS/Linux:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The Server-hosted version should reuse [shared/](../shared/README.md), including
the native plugin, domain libraries, policies, queries, migrations, fixtures and
React UI, rather than copy those implementations. Deployment configuration and
Server-specific lifecycle integration belong here.

The remaining graph integration is not just a Server configuration-validation
change. Ordinary Server queries have a portless query-host node in the instance
graph; their evaluator's input/output ports belong to an owned nested graph.
The embedded implementation instead uses the public computation pipeline builder
to create portful query nodes directly in the instance graph. Declaring an edge
to an ordinary query's nonexistent `in` or `out` port cannot connect these paths.
A supported bridge or explicitly selected pipeline construction needs a design
decision, including lifecycle and configuration save/reload/clone behavior.

Existing reaction-plugin `SnapshotFetcher` and `SnapshotStream.next_keyed()`
already preserve exact row identities, sequence watermarks and empty snapshots.
The standard SSE HTTP snapshot endpoint returns only row values; neither it nor
the default SSE event payload supplies the full keyed bootstrap/watermark
contract. The example's native lifecycle handoff and scenario-reset integration
are still unimplemented. Do not infer application readiness from process health,
or PostgreSQL complete-transaction coherence from successful row-stream delivery.
