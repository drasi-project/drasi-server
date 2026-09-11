# Docker deployment notes

For **this sibling-Core source checkout**, use the [native Server setup guide](docs/setup.md).
The container assets below remain generic published-image deployment examples;
they are not a verified source-build path for this branch or a WorkGraph host.
WorkGraph's [host setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/host.md)
and [Sandbox setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/sandbox.md)
are maintained separately.

## Tracked assets and their scope

| Asset | What it does |
|-------|--------------|
| [docker-compose.yml](docker-compose.yml) | Pulls a Server image and starts PostgreSQL 14 with logical replication |
| [docker-compose-server-only.yml](docker-compose-server-only.yml) | Pulls only the Server image; the filename uses a hyphen, not `docker-compose.server-only.yml` |
| [Dockerfile](Dockerfile) | Builds the UI and Rust binary in stages, then runs as the `drasi` user |
| [config/server-docker.yaml](config/server-docker.yaml) | Image configuration template; a config bind mount can hide it |

Both Compose files default to `ghcr.io/drasi-project/drasi-server:latest`.
That floating published image does not prove compatibility with this branch's
Core SDK or WorkGraph plugins. Select a known-compatible image version/digest
and matching plugins before using a container deployment. No plugin artifacts
are built or copied by this Dockerfile.

Use a current Docker Compose v2 release that supports optional `env_file`
entries (`required: false`). The former blanket "v2.0+" prerequisite is not
sufficient for these manifests.

## Source-build limitation

[Cargo.toml](Cargo.toml) resolves `../drasi-core/lib`, `../drasi-core/core`, and
`../drasi-core/components/...`. The Dockerfile copies Server files into `/app`
but does not copy Core into the corresponding sibling path. Consequently
`docker build .` and `make docker-build` cannot build this checkout as written.
The Compose services contain `image`, not `build`, so `docker compose build`
does not build the checked-out Server either.

A container source build needs a deliberately adapted multi-repository context,
matching native libraries, and matching plugins. This documentation change does
not supply or claim such a build. Use [native setup](docs/setup.md) rather than
switching to an unrelated image to bypass a source-build failure.

## Before adapting the published-image examples

Use a private deployment directory and review the selected image's interface.
Do not mount an existing live configuration or database merely to try a demo.

- **Configuration:** Both Compose files mount `./config` read-write at
  `/app/config`; the image starts with `/app/config/server.yaml`. Prepare a
  private file explicitly. A directory mount hides the image's bundled config,
  and missing-file startup can generate a new default and start the server.
  Keep the file and directory writable by the container user if API changes
  should be saved. Do not apply recursive permissive permissions to secrets.
- **Ports:** Both publish `DRASI_API_PORT` (default 8080) and `DRASI_SSE_PORT`
  (default 8081) on all host interfaces by default. For local use, change
  mappings in the private deployment to `127.0.0.1:8080:8080` and publish only
  the plugin ports actually needed. The container API generally needs YAML
  `host: 0.0.0.0` to be reachable through a published port. That is distinct
  from the loopback host mapping. Port 8081 is not a built-in Server SSE service:
  a configured reaction must listen there.
- **Access control:** The management API has no built-in inbound authentication
  or TLS. CORS and read-only config are not substitutes. Any remote deployment
  needs a separately secured boundary.
- **Environment:** Compose maps `LOG_LEVEL` to `RUST_LOG`.
  `SERVER_HOST` / `SERVER_PORT` only affect configuration fields that reference
  those variables; they are not universal Server overrides. The full-stack file
  also supplies PostgreSQL connection variables and has demo password defaults
  that must not be used for a real deployment. Keep credentials private.
- **Data:** The full-stack file persists PostgreSQL in `drasi_postgres_data`,
  but neither file mounts `/app/data`. Add persistent storage, writable by the
  image's runtime user, for the always-on Server WAL and any configured indexes
  or plugin state. Config persistence alone does not preserve that state.
- **Plugins:** Provide compatible libraries in the executable-adjacent
  `plugins/` directory or pass `--plugins-dir` for a mounted directory. Generic
  registry installation requires available platform/SDK artifacts and signature
  verification; do not assume a Server image contains WorkGraph plugins.
- **Lifecycle:** Restart your own deployment after deliberate config changes.
  `docker compose down -v` removes database volumes; it is destructive cleanup,
  not a prerequisite or routine repair step.

The [Server setup guide](docs/setup.md#5-validate-run-and-check-the-plain-host)
documents the health/API endpoints and the limitations of config validation.
It is the canonical starting point instead of the former duplicated Docker
quick-start, submodule, and single-repository build instructions.
