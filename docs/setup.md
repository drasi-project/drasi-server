# Drasi Server local source setup

This guide covers the **generic Drasi Server host in this checkout**: build it,
create a private configuration, and run a loopback-only server. It does not
install WorkGraph or create a GitHub Sandbox repository.

For that workflow, start with [WorkGraph setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/README.md),
then follow its [host guide](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/host.md)
and [Sandbox repository guide](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/sandbox.md).
WorkGraph owns its protocol, compiler, plugins, and repository kit. `drasi-core`
provides the generic library/SDK; neither Core nor Server is the WorkGraph host
configuration or Sandbox kit.

## 1. Check the repository layout

The path dependencies in [Cargo.toml](../Cargo.toml) require sibling checkouts:

```text
workspace/
  drasi-core/
    lib/
    core/
    components/
  drasi-server/
    Cargo.toml
    ui/
  drasi-workgraph/       # Only needed for WorkGraph, not an empty Server
```

For a new prototype workspace, clone the matching branch into a new directory
(do not clone over existing work):

```bash
git clone --branch workgraph-generic-recovery https://github.com/drasi-project/drasi-core.git
git clone --branch workgraph-generic-recovery https://github.com/drasi-project/drasi-server.git
cd drasi-server
```

If the checkouts already exist, use those instead. Keep Server, Core, and locally
built plugins on compatible revisions and toolchains. WorkGraph's guide adds
its own checkout/provenance requirements; these two clones do not replace them.

There is no separate `../drasi-lib` checkout or nested `drasi-server/drasi-core`
submodule to initialize. The commented patch examples in `.cargo/config.toml`
do not change the active sibling dependencies in `Cargo.toml`.

## 2. Install build prerequisites

| Requirement | Current checkout |
|-------------|------------------|
| Rust/Cargo via rustup | **1.95.0**, pinned by [rust-toolchain.toml](../rust-toolchain.toml); the old Rust 1.70 advice is obsolete |
| Node.js and npm | Use **Node 22**, matching the UI builder in [Dockerfile](../Dockerfile); needed for the Web UI, not an API-only Rust build |
| Native build tools | C/C++ compiler and linker, Make, pkg-config, CMake, Clang/libclang |
| Native libraries/tools | jq and oniguruma development libraries; Protocol Buffers compiler and headers; see the platform notes below |
| Git and network access | Fetch matching repositories and uncached Cargo/npm dependencies |
| Optional tools | curl for HTTP checks; Docker/PostgreSQL only for examples that require them |

The server enables jq middleware and includes RocksDB through its Core path
dependencies. `persistIndex: false` is a runtime setting, not a way to omit
native build dependencies.

On Debian/Ubuntu, the repository's Docker build uses this native dependency
baseline:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev libpq-dev \
  libjq-dev libonig-dev protobuf-compiler libprotobuf-dev cmake git clang libclang-dev
```

On systems where jq has no pkg-config entry, set `JQ_LIB_DIR` to the directory
containing the installed `libjq` library, as the Dockerfile does. Do not guess a
library path for another CPU architecture.

For a native macOS build, install Xcode Command Line Tools and Homebrew first.
The [macOS release recipe](../.github/workflows/release.yaml) uses these libraries
and jq/oniguruma overrides; CMake is also needed for native dependencies:

```bash
brew install protobuf pkg-config jq oniguruma autoconf automake libtool cmake
export JQ_LIB_DIR="$(brew --prefix jq)/lib"
export ONIG_LIB_DIR="$(brew --prefix oniguruma)/lib"
export LIBJQ_STATIC=1
export LIBONIG_STATIC=1
```

Use libraries built for the same architecture as Rust. Building an x86_64 binary
on Apple Silicon needs additional native-library work; it is not equivalent to
using the arm64 Homebrew libraries.

**Platform/network limits:** Rust, Cargo, npm, native packages, and registry
artifacts must actually be available. A missing pinned toolchain or uncached
dependency is a setup blocker, not a reason to silently change the toolchain or
lockfile. On Windows MSVC, [build.rs](../build.rs) downloads vendored native
libraries from GHCR when they are absent; this is not an offline build path.
The cross-build recipes also need their target libraries and access to the
sibling Core checkout. This guide uses a native build, not an unverified
cross-compilation or container workaround.

## 3. Build the server and, optionally, the UI

Run from `drasi-server/`, with the native-library environment set in this shell.
The shell examples use native macOS/Linux paths. For a UI-enabled release build,
the explicit sequence is:

```bash
(cd ui && npm ci && npm run build) &&
  cargo build --release --locked
```

The existing shortcut is `make build-release` (`make build` for debug). It runs
the UI build before Cargo, but `make build-ui` uses `npm install`, skips the UI
when npm is missing, and can mask a failed npm command with its final success
message. The explicit sequence above propagates failures and uses the npm
lockfile. Do not assume a Make success message proves that UI assets exist.

For an API-only release build, omit the npm step:

```bash
cargo build --release --locked
```

Then run with `--disable-ui` or `enableUi: false`. Cargo does not run npm.
`build.rs` creates an empty `ui/dist` if needed; an empty directory is not a UI.

The release binary is `target/release/drasi-server` (`.exe` on Windows).
[UI assets](../src/ui_assets.rs) are embedded at release compile time, so build
the UI **before** Cargo when moving the binary away from this checkout. At
runtime, [the server](../src/server.rs) prefers `ui/dist/index.html` under its
working directory, then falls back to embedded assets. `/ui/` returns 404 when
neither exists. Building the UI afterward only helps a binary running with
those filesystem assets; rebuild Rust to distribute the newly embedded UI.

Record these paths for the remaining shell examples:

```bash
SERVER_REPO="$PWD"
SERVER_BIN="$SERVER_REPO/target/release/drasi-server"
```

## 4. Create an isolated, loopback-only configuration

Use a new private working directory, not a tracked example or an existing live
host directory. The following location is an example; choose another if it
already contains a server or data you need to preserve:

```bash
mkdir -m 700 "$HOME/drasi-server-local" &&
  mkdir "$HOME/drasi-server-local/plugins" &&
  cd "$HOME/drasi-server-local"
```

Create `server.yaml` there with:

```yaml
apiVersion: drasi.io/v1
id: local-server
host: 127.0.0.1
port: 8080
logLevel: info
enableUi: true
persistConfig: false
persistIndex: false
autoInstallPlugins: false
verifyPlugins: true
corsAllowedOrigins:
  - http://127.0.0.1:8080
sources: []
queries: []
reactions: []
```

This is an empty host with no external plugins, not a WorkGraph deployment. Use
`enableUi: false` if you chose the API-only build.

Important configuration behavior:

- **Bind address:** The default is `0.0.0.0:8080`, not loopback. Set `host` in
  YAML; there is no `--host` CLI option. `--port 18080` overrides only the API
  port. If changing the port, update the allowed browser origin too.
- **Local safety:** This checkout's management API has no built-in inbound
  authentication or TLS. CORS is not authentication. Keep it loopback-only for
  development; remote access needs a separately secured boundary. Source and
  reaction listeners have their own bind/port settings, independent of the
  Server API port.
- **Environment:** `run` and `validate` load `.env` beside the selected config
  file and resolve `${VAR}` / `${VAR:-default}` references. Keep secrets private
  and out of version control. `SERVER_HOST`, `SERVER_PORT`, and `LOG_LEVEL` only
  affect YAML fields that explicitly reference them; `RUST_LOG` overrides
  logging directly. There is no `--log-level` CLI option.
- **Paths and data:** Relative config arguments, plugin overrides, state-store
  paths, and runtime files use the process working directory, not the config
  file's directory. Every instance has an always-on redb WAL under
  `./data/<instance-key>/wal/`, with a hex-encoded instance key and files created
  as events arrive. `persistIndex` adds RocksDB indexes under the same instance
  key; `stateStore` is separate plugin-state persistence. `persistConfig: false`
  does not disable the WAL or make the host stateless.
- **Configuration writes:** `persistConfig: false` allows API mutations but
  does not save them to YAML. With the default `true`, API changes can rewrite
  the config; keep both the file and its directory writable for atomic saves.
  A non-writable config triggers read-only management behavior, not network
  authentication.

Do not rely on a missing config as a dry run: invoking the server without a
subcommand creates a default file at `--config` (default `config/server.yaml`)
and **starts the server**, with the defaults above.

## 5. Validate, run, and check the plain host

From the private working directory:

```bash
"$SERVER_BIN" validate --config ./server.yaml --plugins-dir ./plugins
```

Review the output with the validation limits below, then start the host:

```bash
"$SERVER_BIN" --config ./server.yaml --plugins-dir ./plugins
```

The run command stays in the foreground. Use Ctrl-C in that terminal to stop
your server. In another terminal:

```bash
curl --fail http://127.0.0.1:8080/health
curl --fail http://127.0.0.1:8080/api/v1/instances
curl --fail http://127.0.0.1:8080/api/v1/plugins/kinds
```

The instance list should include `local-server`. The empty config does not
create data pipelines; a no-lockfile warning is expected because this directory
contains no external plugins. Open `http://127.0.0.1:8080/ui/` for the optional UI,
`http://127.0.0.1:8080/api/v1/docs/` for Swagger, or
`http://127.0.0.1:8080/api/v1/openapi.json` for the API schema.

### What validation does and does not prove

`validate` parses config, resolves settings/environment references, and loads
local native plugin libraries to inspect their descriptors and schemas. It does
not start the Server listener, instantiate configured sources/reactions or
secret-store providers, start queries, or auto-install plugins.

However, loading a native library calls plugin initialization code. Only point
it at **trusted** plugins: it is not a sandbox or a guarantee of no plugin-side
network/filesystem effects. The host validation path does not perform OCI
downloads or contact configured secret providers, and it does not verify that
credentials or external services work. It reads the adjacent `.env` if present;
do not run it over a private config without permission to read that material.

Validation does **not** apply startup signature verification, so
`--skip-verification` is neither needed nor a validation control. Missing
plugins or plugin-load failures may produce warnings without a nonzero exit.
Inspect the output for missing kinds and schema errors; exit zero alone is not
proof that a configured pipeline can start.

Use the actual binary's `validate` command, not `make validate`: that Make
wrapper suppresses stderr and masks failures. `--show-resolved` displays
resolved server settings, not a fully validated or redacted runtime deployment.

## 6. Add generic plugins for examples

Sources such as `mock`, `postgres`, `http`, and `grpc`, and reactions such as
`log` and `sse`, are runtime shared libraries. Cargo builds the host, not all
those plugins. A small application/no-op core is statically registered; there
are no `builtin-plugins` or `dynamic-plugins` Server feature switches.

For a local source build, build generic plugins from the **same sibling Core
checkout**. From `drasi-server/`:

| Command | Output directory |
|---------|------------------|
| `make build-local-plugins` | `target/release/plugins/` (all generic plugins, release) |
| `make build-local-plugins-debug` | `target/debug/plugins/` (all generic plugins, debug) |
| `make build-local-test-plugins` | `target/debug/plugins/` (mock source, log/HTTP reactions, scriptfile bootstrap) |

For example, after stopping the plain host:

```bash
cd "$SERVER_REPO"
make build-local-plugins
```

These targets delegate builds to Core and copy libraries to Server. Check that
the expected `.so` (Linux), `.dylib` (macOS), or `.dll` (Windows) files were
actually produced; the copy recipe can print a warning without failing Make.
They do not build WorkGraph-specific plugins.

For a first pipeline, copy the `sources`, `queries`, and `reactions` blocks from
[hello-world.yaml](../examples/configs/01-fundamentals/hello-world.yaml) into your
private `server.yaml`, retaining the loopback host, CORS restriction,
`persistConfig: false`, and `autoInstallPlugins: false` above. Do not run the
tracked file unchanged: it enables registry installation and binds all
interfaces. See the [configuration examples](../examples/configs/README.md)
for other pipelines.

Then use the explicit local plugin directory:

```bash
cd "$HOME/drasi-server-local"
"$SERVER_BIN" validate --config ./server.yaml \
  --plugins-dir "$SERVER_REPO/target/release/plugins"
```

Resolve errors and confirm all referenced plugin kinds are available before
starting; missing plugins can be warnings even with exit zero. Then run:

```bash
"$SERVER_BIN" --config ./server.yaml \
  --plugins-dir "$SERVER_REPO/target/release/plugins" --skip-verification
```

**Development-only exception:** Self-built plugins are unsigned. Startup
defaults to `verifyPlugins: true` and re-verifies lockfile entries against the
registry; without a verified lockfile, local plugin files are skipped.
`--skip-verification` (or private `verifyPlugins: false`) allows trusted,
self-built libraries to load. Do not use it for untrusted downloads or as a
workaround for SDK/target incompatibility. Rebuild matching host and plugins
instead of mixing registry binaries with local Core changes.

### Plugin directory versus registry

`--plugins-dir` selects the directory to load/install libraries. Its default is
`plugins/` **beside the executable**, such as `target/release/plugins/`, not
necessarily `./plugins` in the working directory.

`pluginRegistry` in YAML and `--registry` on plugin commands select where
installations are sourced. They accept OCI registries and local directory paths;
they do not change the runtime scan directory.

`autoInstallPlugins` defaults to `false`. When enabled, startup installs the
explicit `plugins: [{ref: ...}]` dependencies. Declaring component `kind` fields
alone does not install their plugins. Keep automatic installation off for the
local-build recipe.

For an intentionally registry-backed setup with compatible artifacts, declare
the required `plugins` in a private config, then install explicitly:

```bash
"$SERVER_BIN" --config ./server.yaml --plugins-dir ./plugins plugin install --from-config
"$SERVER_BIN" --config ./server.yaml --plugins-dir ./plugins plugin list
```

Installation writes libraries and `plugins.lock`; `--from-config --locked`
requires an existing matching lockfile. Registry access, signatures, SDK
compatibility, and an artifact for your actual platform are prerequisites, not
guaranteed by this checkout. A local directory registry does not sign libraries.
CLI upgrades change disk files, not a running host; restart deliberately.

## Legacy helpers and further reading

- `drasi-server doctor` and `make doctor` still check the obsolete
  `drasi-core/lib` submodule path. Their missing-submodule warning does not
  diagnose this sibling layout. `make setup` can invoke the server while
  creating a config; it is not a non-starting prerequisite check.
- `drasi-server init --output <new-path>` is an interactive configuration
  wizard, not an installer for Core or WorkGraph. Review its output and plugin
  choices before starting it.
- [Docker notes](../DOCKER.md) explain why published-image Compose examples
  and the current root-only Docker build are not a way to build this branch.
- [README configuration/API reference](../README.md#configuration-reference)
  and [plugin architecture notes](plugin-architecture.md) cover the generic host.
- [WorkGraph host setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/host.md)
  owns the actual WorkGraph configuration and build provenance.
  [WorkGraph Sandbox setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/sandbox.md)
  owns the disposable repository used to test it; the Server playground and
  GitHub-webhook examples are not that Sandbox.
