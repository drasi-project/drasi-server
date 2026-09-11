# Drasi Server

Drasi Server is a standalone server for real-time data change processing. It monitors your data sources, runs continuous queries, and triggers automated reactions when results change—all through a simple YAML configuration or visual Web UI.

**Setting up this checkout?** Follow the [local Server setup guide](docs/setup.md)
for the sibling Core dependency, native prerequisites, UI build, plugins, and a
loopback-only configuration. For WorkGraph, use the separate
[WorkGraph setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/README.md),
[host](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/host.md),
and [Sandbox repository](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/sandbox.md)
guides. Server is a generic host, not the WorkGraph protocol/compiler or repository kit.

**Key Features:**
- **Visual Web UI** for managing data pipelines
- **REST API** for programmatic control
- **YAML-based configuration** with environment variable support
- **Multiple data sources** (PostgreSQL CDC, HTTP webhooks, gRPC)
- **Continuous queries** using Cypher query language
- **Automated reactions** (HTTP webhooks, SSE, gRPC, logging)
- **Solution Templates** for quick deployment of pre-configured pipelines
- **VS Code Extension** for integrated development

## Table of Contents

- [What is Drasi?](#what-is-drasi)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Running Drasi Server](#running-drasi-server)
- [Web UI Guide](#web-ui-guide)
- [Instances](#instances)
- [Solution Templates](#solution-templates)
- [Configuration Reference](#configuration-reference)
- [REST API](#rest-api)
- [VS Code Extension](#vs-code-extension)
- [Development Utilities](#development-utilities)
- [Docker Deployment](#docker-deployment)
- [Use Cases](#use-cases)
- [Complete Configuration Examples](#complete-configuration-examples)
- [Troubleshooting](#troubleshooting)
- [Building from Source](#building-from-source)
- [Related Projects](#related-projects)

---

## What is Drasi?

Drasi is an open-source Data Change Processing platform that simplifies building change-driven solutions. Instead of polling databases, parsing event streams, or maintaining external state, you declaratively specify what changes matter through **continuous queries**.

### Core Concepts

| Concept | Description |
|---------|-------------|
| **Sources** | Data ingestion points that connect to your systems and stream changes (PostgreSQL, HTTP, gRPC) |
| **Queries** | Cypher queries that run continuously, maintaining current results and generating change notifications |
| **Reactions** | Automated responses triggered when query results change (webhooks, SSE streams, logging) |
| **Instances** | Isolated processing environments with their own sources, queries, and reactions |
| **Solution Templates** | Pre-configured component sets that can be deployed with a single click |

### How It Works

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Sources   │ ──► │   Queries   │ ──► │  Reactions  │
│             │     │             │     │             │
│ PostgreSQL  │     │  Continuous │     │  Webhooks   │
│ HTTP/gRPC   │     │   Cypher    │     │  SSE/gRPC   │
│ Mock        │     │   Queries   │     │  Logging    │
└─────────────┘     └─────────────┘     └─────────────┘
```

---

## Quick Start

Start with the [local setup guide](docs/setup.md). It walks through building this
checkout, running an empty host, and adding the locally built mock/log plugins
for a first continuous query. A Cargo build alone does not include those plugins.

After setup, use the [Web UI guide](#web-ui-guide), the
[progressive configuration examples](examples/configs/README.md), or the
[PostgreSQL getting-started tutorial](examples/getting-started/README.md).
These generic Server examples are not the WorkGraph Sandbox.

---

## Installation

This branch requires sibling `../drasi-core` source dependencies, Rust **1.95.0**,
and native build libraries. The optional UI also needs Node.js/npm (Node 22 is
used by the Docker UI builder). See [setup prerequisites and commands](docs/setup.md)
rather than the former single-repository installation recipe.

Published images are a different installation path: the Compose files pull an
image, not this checkout. Read [Docker scope and limitations](DOCKER.md) before
using them. `init` creates a configuration interactively; it does not install
Core, native dependencies, or WorkGraph.

---

## Running Drasi Server

Use an existing private config with an explicit loopback `host` as shown in
[local setup](docs/setup.md#4-create-an-isolated-loopback-only-configuration).
Bare startup otherwise creates a writable config and binds `0.0.0.0:8080`.
The management API does not provide inbound authentication or TLS.

### Basic Usage

```bash
# Run with default config (config/server.yaml)
drasi-server

# Run with specific config
drasi-server --config path/to/config.yaml

# Run on different port
drasi-server --port 9090

# With cargo
cargo run -- --config config/server.yaml
```

### Command Line Reference

```
drasi-server [OPTIONS] [COMMAND]
```

**Global Options:**

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--config <PATH>` | `-c` | `config/server.yaml` | Path to the configuration file |
| `--port <PORT>` | `-p` | (from config) | Override the server port |
| `--plugins-dir <PATH>` | | `<binary-dir>/plugins` | Directory for runtime plugin libraries |
| `--skip-verification` | | `false` | Disable startup signature verification for trusted local development plugins |
| `--enable-ui` | | | Enable Web UI (overrides config) |
| `--disable-ui` | | | Disable Web UI (overrides config) |
| `--help` | `-h` | | Print help information |
| `--version` | `-V` | | Print version information |

**Commands:**

| Command | Description |
|---------|-------------|
| `run` | Run the server (default if no command specified) |
| `init` | Create a new configuration file interactively |
| `validate` | Validate a configuration file without starting |
| `doctor` | Check system dependencies |
| `plugin` | Install, list, search, upgrade, and remove plugins |

`host` is configured in YAML; there is no `--host` flag. Use `logLevel` in YAML
or `RUST_LOG` for logging, not `--log-level`. See the
[validation limits and legacy helper warnings](docs/setup.md#what-validation-does-and-does-not-prove)
before using `validate`, `doctor`, or their Make wrappers as readiness checks.

**Examples:**

```bash
# Run with Web UI enabled
drasi-server --enable-ui

# Run with Web UI disabled
drasi-server --disable-ui

# Create new config interactively
drasi-server init --output config/my-config.yaml

# Validate config file
drasi-server validate --config config/server.yaml
drasi-server validate --config config/server.yaml --show-resolved

# Legacy diagnostic: includes an obsolete submodule check
drasi-server doctor
drasi-server doctor --all  # Include optional deps
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Override log level (e.g., `debug`, `trace`, `drasi_server=debug`) |

Drasi Server automatically loads `.env` files from the same directory as your config file.

### Configuration File Auto-Creation

If no config file exists at the specified path, Drasi Server creates a default one automatically:

```bash
# Creates config/server.yaml if it doesn't exist
drasi-server --config config/server.yaml
```

This also starts the server. It is not a configuration-only command.

---

## Web UI Guide

Drasi Server includes a visual Web UI for managing data pipelines without writing configuration files or API calls.

### Prerequisites (source builds only)

The Web UI is a separate Vite/React app under `ui/` that compiles to static
assets in `ui/dist`, served at `/ui/`. Build the UI before the release Rust
binary so it can embed those assets:

```bash
(cd ui && npm ci && npm run build) &&
  cargo build --release --locked
```

`make build-release` is the existing shortcut; its UI step can skip or mask npm
failures, so follow the [build guidance](docs/setup.md#3-build-the-server-and-optionally-the-ui).
At runtime the server prefers `ui/dist` under the working directory, then
embedded assets. If you build the UI after Rust, rebuild Rust before moving the
binary away from those filesystem assets.

If neither filesystem nor embedded UI assets exist, `/ui/` returns 404.
Use `--disable-ui` (or `enableUi: false`) for an intentional API-only build.

### Accessing the Web UI

For the local setup configuration, open **http://127.0.0.1:8080/ui/** in your browser.

The UI is enabled by default. To disable it:
```bash
# Via command line
drasi-server --disable-ui

# Via config file
enableUi: false
```

### Flow Canvas

The main canvas displays your data pipeline as an interactive graph:

- **Green nodes** = Sources (data inputs)
- **Blue nodes** = Queries (data processing)
- **Purple nodes** = Reactions (outputs/actions)
- **Animated edges** show data flow direction

**Node Status Colors:**
| Color | Status |
|-------|--------|
| Green border | Running |
| Gray border | Stopped |
| Red border | Failed/Error |
| Pulsing animation | Starting/Stopping |

**Canvas Interactions:**
- **Click** a node to open its inspector panel
- **Drag** nodes to rearrange the layout
- **Scroll** to zoom in/out
- **Click empty space** to close the inspector

### Adding Components

1. Click **+ Add** in the top toolbar
2. Choose a component type:
   - **Source** → Select source kind (PostgreSQL, HTTP, gRPC, Mock, Platform)
   - **Query** → Configure continuous Cypher query
   - **Reaction** → Select reaction kind (Log, HTTP, SSE, gRPC, etc.)
   - **Solutions** → Deploy pre-configured templates
3. Fill in the configuration form
4. Click **Save** to create the component

### Inspector Panels

Click any node to open its inspector panel on the right side. The panel shows:

**For Sources:**
- Status and configuration details
- Connected queries
- Start/Stop/Delete actions
- Configuration properties

**For Queries:**
- Query text and language
- Connected sources and reactions
- Current result count
- Start/Stop/Delete actions

**For Reactions:**
- Configuration and connection details
- Subscribed queries
- Start/Stop/Delete actions

### Activity Feed

Click the **Activity** button (bell icon) in the toolbar to open the activity panel. This shows:
- Real-time component events (started, stopped, created, deleted)
- Error messages and warnings
- Timestamped event log

### Theme Toggle

Click the sun/moon icon in the toolbar to switch between light and dark themes.

### URL-Based Instance Selection

You can link directly to a specific instance:
```
http://localhost:8080/ui?instance=my-instance-id
```

---

## Instances

Instances are isolated processing environments, each with its own sources, queries, and reactions. They're useful for:

- **Multi-tenant deployments** - Separate data pipelines per customer
- **Environment isolation** - Dev/staging/production in one server
- **Modular architectures** - Group related components together

### Default Behavior

When you start Drasi Server, it creates one default instance. The Web UI and convenience API routes operate on this instance automatically.

### Creating Instances

**Via Web UI:**
1. Click the instance selector dropdown in the top-left
2. Click **+ New Instance**
3. Enter an instance ID
4. Click **Create**

**Via API:**
```bash
curl -X POST http://localhost:8080/api/v1/instances \
  -H "Content-Type: application/json" \
  -d '{"id": "my-new-instance"}'
```

**Via Config File:**
```yaml
apiVersion: drasi.io/v1
host: 0.0.0.0
port: 8080

instances:
  - id: production
    sources:
      - kind: postgres
        id: main-db
        # ...
    queries: []
    reactions: []
    
  - id: staging
    sources:
      - kind: mock
        id: test-data
    queries: []
    reactions: []
```

### Switching Instances

**In Web UI:**
Click the instance selector dropdown and choose an instance.

**Via URL:**
```
http://localhost:8080/ui?instance=production
```

### Cloning Instances

The Web UI allows cloning an instance with all its components:
1. Select the source instance
2. Click the instance selector → **Clone Instance**
3. Enter a new instance ID
4. Components are copied to the new instance

### Instance-Specific API Routes

All component routes support instance-specific access:
```
/api/v1/instances/{instanceId}/sources
/api/v1/instances/{instanceId}/queries
/api/v1/instances/{instanceId}/reactions
/api/v1/instances/{instanceId}/snapshot   # GET - configuration snapshot
/api/v1/instances/{instanceId}/clone      # POST - clone from another instance
```

The convenience routes (`/api/v1/sources`, etc.) operate on the first/default instance.

---

## Solution Templates

Solution Templates are pre-configured sets of sources, queries, and reactions that can be deployed with a single action. They're useful for:

- Quickly setting up common patterns
- Sharing configurations across teams
- Creating reusable pipeline blueprints

### Built-in Templates

Drasi Server includes templates in the `solutions/` directory:

| Template | Description |
|----------|-------------|
| `simple-log-pipeline.yaml` | Basic source → query → log setup |
| `iot-temperature-monitor.yaml` | IoT sensor monitoring with alerts |

### Deploying Templates

**Via Web UI:**
1. Click **+ Add** → **Solutions**
2. Browse the Solution Gallery
3. Click a template to select it
4. Configure any required variables
5. Choose the target instance
6. Click **Deploy Solution**

**Via API:**
```bash
curl -X POST http://localhost:8080/api/v1/instances/default/solutions \
  -H "Content-Type: application/json" \
  -d '{
    "templateId": "iot-temperature-monitor",
    "variables": {
      "TEMP_THRESHOLD": "80"
    }
  }'
```

### Template Variables

Templates can include variables using `${VAR_NAME:-default}` syntax:

```yaml
# Template with variables
name: IoT Monitor
queries:
  - id: high-temp
    query: "MATCH (s:Sensor) WHERE s.temp > ${TEMP_THRESHOLD:-75} RETURN s"
```

When deploying, you can override these values.

### Creating Templates from Instances

Save your current instance configuration as a reusable template:

**Via Web UI:**
1. Select the instance in the dropdown
2. Click **Create Template**
3. Fill in template metadata (name, description, version)
4. Click **Save**

The template is saved to the `solutions/` directory.

### Template YAML Format

```yaml
name: My Template
description: A description of what this template does
version: "1.0.0"
author: Your Name
license: MIT
defaultInstanceId: my-instance

sources:
  - kind: mock
    id: data-source
    autoStart: true

queries:
  - id: my-query
    query: "MATCH (n) WHERE n.value > ${THRESHOLD:-10} RETURN n"
    sources:
      - sourceId: data-source
    autoStart: true

reactions:
  - kind: log
    id: logger
    queries:
      - my-query
    autoStart: true
```

### Uploading Templates

You can upload custom template YAML files directly in the Web UI:
1. Click **+ Add** → **Solutions**
2. Click **Upload Template**
3. Select your YAML file
4. Configure variables and deploy

---

#### `plugin`

Manage dynamic plugins — install, upgrade, list, search, and remove plugin shared libraries.

Plugin management is part of the normal Server build. The old `builtin-plugins`
and `dynamic-plugins` feature switches no longer exist. For self-built plugins,
follow the [local plugin setup](docs/setup.md#6-add-generic-plugins-for-examples).

##### `plugin install`

Install a plugin from an OCI registry, local file, or HTTP URL.

```bash
# From OCI registry (default)
drasi-server plugin install source/postgres:0.1.8
drasi-server plugin install source/postgres              # latest compatible version
drasi-server plugin install ghcr.io/acme/custom-source:1.0.0

# From OCI registry using wildcard patterns (quote to prevent shell expansion)
drasi-server plugin install "source/*"
drasi-server plugin install "*/postgres"

# From local file
drasi-server plugin install file:///opt/drasi/libdrasi_source_custom.so

# From HTTP URL
drasi-server plugin install https://releases.example.com/libdrasi_source_custom.so

# Install all plugins declared in the config file
drasi-server plugin install --from-config

# Install using exact versions from lockfile
drasi-server plugin install --from-config --locked
```

**Options:**
- `--from-config`: Install all plugins declared in the config file's `plugins` section
- `--registry <URL-or-PATH>`: Override OCI registry or local directory source (default: from config or `ghcr.io/drasi-project`)
- `--platform <PLATFORM>`: Accepted by the CLI, but currently unused by installation dispatch; do not rely on it to select a different platform
- `--locked`: Use exact versions from `plugins.lock` (fails if lockfile is missing or outdated)

> **Tip:** Wildcard patterns apply to OCI references only. File/HTTP installs must use exact URIs.

##### `plugin upgrade`

Upgrade installed plugins to newer compatible versions from the OCI registry.

> **Note:** This is an offline, package-manager-style command. It only
> updates plugin binaries on disk and the `plugins.lock` file — it does
> **not** reload or replace plugins in a running server. Restart the server
> for upgraded plugins to take effect.

```bash
# Upgrade a specific plugin
drasi-server plugin upgrade source/postgres

# Upgrade to a specific version
drasi-server plugin upgrade source/postgres:0.2.0

# Upgrade all installed plugins
drasi-server plugin upgrade --all

# Preview what would change without downloading
drasi-server plugin upgrade --all --dry-run
```

**Options:**
- `--all`: Upgrade all installed plugins
- `--registry <URL>`: Override OCI registry
- `--dry-run`: Show what would change without actually upgrading

**Output:**
```
Checking for upgrades...
  source/postgres — upgrading 0.1.8 → 0.2.0
  reaction/log — up to date (0.1.7)
  file:///opt/custom.so — skipped (non-OCI source)

Upgrade complete: 1 upgraded, 1 up to date, 0 failed
```

##### `plugin list`

List installed plugins in the plugins directory.

```bash
drasi-server plugin list
```

##### `plugin search`

Search for available versions of a plugin in the registry.

```bash
drasi-server plugin search source/postgres
drasi-server plugin search reaction/sse --registry ghcr.io/my-org
```

##### `plugin remove`

Remove one or more installed plugins.

```bash
drasi-server plugin remove source/postgres
drasi-server plugin remove libdrasi_source_postgres.so

# Remove with wildcard patterns (quote to prevent shell expansion)
drasi-server plugin remove "source/*"
drasi-server plugin remove "*/postgres"
```

##### `plugin install-all`

Install all available plugins from the registry's plugin directory.

```bash
drasi-server plugin install-all
drasi-server plugin install-all --registry ghcr.io/my-org
```

## Configuration Reference

Drasi Server uses YAML configuration files. All configuration values support environment variable interpolation using `${VAR}` or `${VAR:-default}` syntax.

### Server Settings

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | string | (auto-generated UUID) | Unique server identifier |
| `host` | string | `0.0.0.0` | Server bind address |
| `port` | integer | `8080` | Server port |
| `logLevel` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `persistConfig` | boolean | `true` | Enable saving API changes to config file |
| `persistIndex` | boolean | `false` | When `true`, registers a RocksDB index provider named `rocksdb` as the default index backend for all queries in the instance (data stored under `./data/<instance-key>/index`). When `false`, queries use in-memory indexes. Individual queries can override the backend via `storageBackend`. |
| `enableUi` | boolean | `true` | Serve the Web UI when filesystem or embedded assets are available |
| `stateStore` | object | (none) | State store provider for plugin state persistence |
| `defaultPriorityQueueCapacity` | integer | `10000` | Default capacity for query/reaction event queues |
| `defaultDispatchBufferCapacity` | integer | `1000` | Default buffer capacity for event dispatching |
| `pluginRegistry` | string | `ghcr.io/drasi-project` | Default OCI registry or local directory source for plugin installation |
| `autoInstallPlugins` | boolean | `false` | Install explicit `plugins` dependencies on startup when enabled |
| `verifyPlugins` | boolean | `true` | Enable cosign signature verification for downloaded plugins (Sigstore keyless: Fulcio + Rekor) |
| `trustedIdentities` | array | `[]` | Custom trusted signer identities for plugin verification (e.g., email, URI) |
| `plugins` | array | `[]` | Dependencies for `plugin install --from-config`, or startup when `autoInstallPlugins: true` (see [Plugins](#plugins-configuration)) |
| `corsAllowedOrigins` | array | `[]` | Allowed browser origins; empty permits all origins, and does not provide authentication |

> **Note**: `<instance-key>` is a hex-encoded form of the instance ID, shared by
> the index and WAL directories. Paths are relative to the process working directory.

**Example:**

```yaml
apiVersion: drasi.io/v1
id: my-server
host: 0.0.0.0
port: 8080
logLevel: info
persistConfig: true
persistIndex: false
pluginRegistry: ghcr.io/drasi-project
verifyPlugins: true  # optional: verify plugin signatures via Sigstore (Fulcio + Rekor)
# trustedIdentities:  # optional: restrict to specific signers
#   - issuer: "https://accounts.google.com"
#     subjectPattern: "release@example.com"

stateStore:
  kind: redb
  path: ./data/state.redb

plugins:
  - ref: source/postgres:0.1.8
  - ref: reaction/sse

sources: []
queries: []
reactions: []
```

> **Write-ahead log (always on):** Every instance maintains a durable
> write-ahead log (WAL) for source events, backed by redb. It is written to
> `./data/<instance-key>/wal/` (where `<instance-key>` is a hex-encoded form of
> the instance ID), relative to the server's working directory. The WAL is
> always enabled and there is currently no configuration option to disable it,
> so account for this directory when planning disk usage and, in containerized
> deployments, mount a volume for `./data/` to persist it across restarts. This
> is separate from the optional `persistIndex` (query indexes) and `stateStore`
> (plugin state) storage.

### Plugins Configuration

The `plugins` section declares plugin dependencies that can be installed with `drasi-server plugin install --from-config`. Each entry specifies a plugin reference that supports three URI formats:

| Format | Example | Description |
|--------|---------|-------------|
| OCI reference | `source/postgres:0.1.8` | Pull from OCI registry (default) |
| File URI | `file:///opt/drasi/libdrasi_source_custom.so` | Copy from local filesystem |
| HTTP URL | `https://releases.example.com/plugin.so` | Download via HTTP |

**Example:**

```yaml
apiVersion: drasi.io/v1
pluginRegistry: ghcr.io/drasi-project

plugins:
  # OCI registry plugins (resolved from pluginRegistry)
  - ref: source/postgres:0.1.8
  - ref: reaction/sse

  # Local file
  - ref: file:///opt/drasi/libdrasi_source_custom.so

  # HTTP download
  - ref: https://releases.example.com/libdrasi_reaction_custom.so

sources: []
queries: []
reactions: []
```

Install all declared plugins:

```bash
drasi-server plugin install --from-config
```

A `plugins.lock` file is created in the plugins directory after installation, pinning exact versions and digests for reproducible builds. Use `--locked` to enforce lockfile versions:

```bash
drasi-server plugin install --from-config --locked
```

### State Store Configuration

State stores allow plugins (Sources, Bootstrap Providers, Reactions) to persist runtime state that survives server restarts. If not configured, an in-memory state store is used (state is lost on restart).

#### REDB State Store

File-based persistent storage using the REDB embedded database.

```yaml
stateStore:
  kind: redb
  path: ./data/state.redb  # Supports ${ENV_VAR:-default}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `kind` | string | Yes | Must be `redb` |
| `path` | string | Yes | Path to the database file |

---

### Sources

Sources connect to data systems and stream changes to queries. Each source type has specific configuration fields.

#### Common Source Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `kind` | string | (required) | Source type: `postgres`, `http`, `grpc`, `mock`, `platform` |
| `id` | string | (required) | Unique source identifier |
| `autoStart` | boolean | `true` | Start source automatically on server startup |
| `bootstrapProvider` | object | (none) | Bootstrap provider configuration |

#### PostgreSQL Source (`postgres`)

Streams changes from PostgreSQL using logical replication (WAL).

```yaml
sources:
  - kind: postgres
    id: my-postgres
    autoStart: true
    host: localhost
    port: 5432
    database: mydb
    user: postgres
    password: ${DB_PASSWORD}
    tables: [orders, customers]
    slotName: drasi_slot
    publicationName: drasi_publication
    sslMode: prefer
    tableKeys:
      - table: orders
        keyColumns: [id]
    bootstrapProvider:
      kind: postgres
      host: localhost
      port: 5432
      database: mydb
      user: postgres
      password: ${DB_PASSWORD}
      tables: [orders, customers]
      slotName: drasi_slot
      publicationName: drasi_pub
      sslMode: prefer
      tableKeys:
        - table: orders
          keyColumns: [id]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `localhost` | Database host |
| `port` | integer | `5432` | Database port |
| `database` | string | (required) | Database name |
| `user` | string | (required) | Database user |
| `password` | string | `""` | Database password |
| `tables` | array | `[]` | Tables to monitor |
| `slotName` | string | `drasi_slot` | Replication slot name |
| `publicationName` | string | `drasi_publication` | Publication name |
| `sslMode` | string | `prefer` | SSL mode: `disable`, `prefer`, `require` |
| `tableKeys` | array | `[]` | Primary key definitions for tables |

#### HTTP Source (`http`)

Receives events via HTTP endpoints. Supports two modes:
- **Standard Mode**: Uses the built-in `HttpSourceChange` format
- **Webhook Mode**: Custom routes with configurable payload mappings for third-party webhooks

**Basic Configuration (Standard Mode):**

```yaml
sources:
  - kind: http
    id: my-http
    autoStart: true
    host: 0.0.0.0
    port: 9000
    timeoutMs: 10000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | (required) | Listen address |
| `port` | integer | (required) | Listen port |
| `endpoint` | string | (auto) | Custom endpoint path |
| `timeoutMs` | integer | `10000` | Request timeout in milliseconds |
| `webhooks` | object | (none) | Webhook configuration (enables webhook mode) |

##### Webhook Mode

Webhook mode enables receiving events from third-party services (GitHub, Stripe, etc.) by mapping their payloads to Drasi source change events.

**GitHub Webhook Example:**

```yaml
sources:
  - kind: http
    id: github-webhook
    autoStart: true
    host: 0.0.0.0
    port: 9000
    webhooks:
      errorBehavior: reject
      cors:
        allowOrigins: ["*"]
      routes:
        - path: /github/events
          methods: [POST]
          auth:
            signature:
              type: hmac-sha256
              secretEnv: GITHUB_WEBHOOK_SECRET
              header: X-Hub-Signature-256
              prefix: "sha256="
          mappings:
            - when:
                header: X-GitHub-Event
                equals: push
              elementType: node
              operation: insert
              template:
                id: "commit-{{payload.head_commit.id}}"
                labels: ["Commit"]
                properties:
                  message: "{{payload.head_commit.message}}"
                  author: "{{payload.head_commit.author.name}}"
            - when:
                header: X-GitHub-Event
                equals: pull_request
              elementType: node
              operationFrom: "$.action"
              operationMap:
                opened: insert
                closed: delete
                synchronize: update
              template:
                id: "pr-{{payload.pull_request.id}}"
                labels: ["PullRequest"]
                properties:
                  title: "{{payload.pull_request.title}}"
```

##### Webhook Configuration Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `errorBehavior` | string | `accept_and_log` | Error handling: `accept_and_log`, `accept_and_skip`, `reject` |
| `cors` | object | (none) | CORS configuration |
| `routes` | array | (required) | List of webhook route configurations |

##### CORS Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable CORS |
| `allowOrigins` | array | `["*"]` | Allowed origins |
| `allowMethods` | array | `["GET", "POST", ...]` | Allowed HTTP methods |
| `allowHeaders` | array | `["Content-Type", ...]` | Allowed headers |
| `exposeHeaders` | array | `[]` | Headers to expose |
| `allowCredentials` | boolean | `false` | Allow credentials |
| `maxAge` | integer | `3600` | Preflight cache time (seconds) |

##### Webhook Route Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | (required) | Route path (supports `:param` for path parameters) |
| `methods` | array | `[POST]` | Allowed HTTP methods |
| `auth` | object | (none) | Authentication configuration |
| `errorBehavior` | string | (global) | Override error behavior for this route |
| `mappings` | array | (required) | Payload to event mappings |

##### Authentication Configuration

**HMAC Signature Verification:**

```yaml
auth:
  signature:
    type: hmac-sha256    # or hmac-sha1
    secretEnv: WEBHOOK_SECRET
    header: X-Signature
    prefix: "sha256="    # Optional prefix to strip
    encoding: hex        # or base64
```

**Bearer Token Verification:**

```yaml
auth:
  bearer:
    tokenEnv: API_TOKEN
```

##### Webhook Mapping Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `when` | object | (none) | Condition for when this mapping applies |
| `operation` | string | (none) | Static operation: `insert`, `update`, `delete` |
| `operationFrom` | string | (none) | JSONPath to extract operation from payload |
| `operationMap` | object | (none) | Map payload values to operations |
| `elementType` | string | (required) | Element type: `node` or `relation` |
| `effectiveFrom` | string/object | (none) | Timestamp configuration |
| `template` | object | (required) | Element creation template |

##### Mapping Conditions

```yaml
when:
  header: X-Event-Type    # Check a header value
  field: "$.event.type"   # Or check a payload field (JSONPath)
  equals: "push"          # Must equal this value
  contains: "event"       # Or must contain this substring
  regex: "^(push|pull)"   # Or must match this regex
```

##### Element Templates

Templates use Handlebars syntax with access to `{{payload.*}}`, `{{headers.*}}`, and `{{path.*}}` variables.

**Node Template:**

```yaml
template:
  id: "{{payload.id}}"
  labels: ["Event", "{{payload.type}}"]
  properties:
    name: "{{payload.name}}"
    timestamp: "{{payload.created_at}}"
```

**Relation Template:**

```yaml
template:
  id: "{{payload.relation_id}}"
  labels: ["CONNECTS_TO"]
  from: "{{payload.source_id}}"
  to: "{{payload.target_id}}"
```

##### Effective From Configuration

Control the timestamp used for the `effective_from` field:

```yaml
# Simple: auto-detect format
effectiveFrom: "{{payload.timestamp}}"
```

```yaml
# Explicit format
effectiveFrom:
  value: "{{payload.created_at}}"
  format: iso8601  # or unix_seconds, unix_millis, unix_nanos
```

#### gRPC Source (`grpc`)

Receives events via gRPC streaming.

```yaml
sources:
  - kind: grpc
    id: my-grpc
    autoStart: true
    host: 0.0.0.0
    port: 50051
    timeoutMs: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `0.0.0.0` | Listen address |
| `port` | integer | `50051` | Listen port |
| `timeoutMs` | integer | `5000` | Connection timeout in milliseconds |

#### Mock Source (`mock`)

Generates synthetic test data for development and demonstrations. Supports three data types with configurable generation intervals.

**Configuration format:**
```yaml
sources:
  - kind: mock
    id: test-source
    autoStart: true
    dataType:
      type: generic    # or "counter", "sensorReading"
    intervalMs: 2000
```

**Sensor reading with custom sensor count:**
```yaml
sources:
  - kind: mock
    id: sensor-source
    autoStart: true
    dataType:
      type: sensorReading
      sensorCount: 10          # Simulate 10 unique sensors
    intervalMs: 2000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `dataType` | object | `{ type: generic }` | Type of mock data (see below) |
| `intervalMs` | integer | `5000` | Data generation interval in milliseconds |

**Data Types:**

| Type | Value | Generated Nodes | Properties |
|------|-------|-----------------|------------|
| Counter | `{ type: counter }` | `Counter` | `value` (sequential int), `timestamp` |
| Sensor Reading | `{ type: sensorReading, sensorCount: N }` | `SensorReading` | `sensor_id`, `temperature` (20-30°C), `humidity` (40-60%), `timestamp` |
| Generic | `{ type: generic }` | `Generic` | `value` (random int), `message`, `timestamp` |

**Sensor Reading Behavior:**
- First reading for each sensor generates an **INSERT** event
- Subsequent readings for the same sensor generate **UPDATE** events
- `sensorCount` controls how many unique sensors are simulated (default: 5)
- Sensor IDs: `sensor_0` through `sensor_{sensorCount-1}`

#### Platform Source (`platform`)

Consumes events from Redis Streams for Drasi Platform integration.

```yaml
sources:
  - kind: platform
    id: platform-source
    autoStart: true
    redisUrl: redis://localhost:6379
    streamKey: my-stream
    consumerGroup: drasi-core
    batchSize: 100
    blockMs: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `redisUrl` | string | (required) | Redis connection URL |
| `streamKey` | string | (required) | Redis stream key to consume |
| `consumerGroup` | string | `drasi-core` | Consumer group name |
| `consumerName` | string | (auto) | Consumer name within group |
| `batchSize` | integer | `100` | Events to read per batch |
| `blockMs` | integer | `5000` | Block timeout in milliseconds |

---

### Bootstrap Providers

Bootstrap providers deliver initial data to queries before streaming begins. Any source can use any bootstrap provider.

#### PostgreSQL Bootstrap (`postgres`)

Loads initial data from PostgreSQL using the COPY protocol.

```yaml
bootstrapProvider:
  kind: postgres
  host: localhost
  port: 5432
  database: mydb
  user: postgres
  password: ${DB_PASSWORD}
  tables: [orders, customers]
  slotName: drasi_slot
  publicationName: drasi_pub
  sslMode: prefer
  tableKeys:
    - table: orders
      keyColumns: [id]
```

#### Script File Bootstrap (`scriptfile`)

Loads initial data from JSONL files.

```yaml
bootstrapProvider:
  kind: scriptfile
  filePaths:
    - /data/initial_nodes.jsonl
    - /data/initial_relations.jsonl
```

#### Platform Bootstrap (`platform`)

Loads initial data from a remote Drasi Query API.

```yaml
bootstrapProvider:
  kind: platform
  queryApiUrl: http://remote-drasi:8080
  timeoutSeconds: 300
```

#### No-Op Bootstrap (`noop`)

Returns no initial data.

```yaml
bootstrapProvider:
  kind: noop
```

---

### Queries

Continuous queries process data changes and maintain materialized results.

```yaml
queries:
  - id: active-orders
    query: |
      MATCH (o:Order)
      WHERE o.status = 'active'
      RETURN o.id, o.customer_id, o.total
    queryLanguage: Cypher
    sources:
      - sourceId: orders-db
    autoStart: true
    enableBootstrap: true
    bootstrapBufferSize: 10000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | string | (required) | Unique query identifier |
| `query` | string | (required) | Query string (Cypher or GQL) |
| `queryLanguage` | string | `GQL` | Query language: `Cypher` or `GQL` |
| `sources` | array | (required) | Source subscriptions |
| `autoStart` | boolean | `false` | Start query automatically |
| `enableBootstrap` | boolean | `true` | Process initial data from sources |
| `bootstrapBufferSize` | integer | `10000` | Event buffer size during bootstrap |
| `priorityQueueCapacity` | integer | (global) | Override queue capacity for this query |
| `dispatchBufferCapacity` | integer | (global) | Override buffer capacity for this query |
| `storageBackend` | string or object | (instance default) | Index backend for this query. See [Per-Query Index Backend](#per-query-index-backend). |
| `joins` | array | (none) | Synthetic join definitions |

**Important Limitation**: `ORDER BY`, `TOP`, and `LIMIT` clauses are not supported in continuous queries.

#### Per-Query Index Backend

By default, every query uses the instance's index backend: in-memory when `persistIndex` is `false`, or the persistent `rocksdb` provider when `persistIndex` is `true`. The optional `storageBackend` field lets an individual query override that default.

It accepts either a **named provider** (a string) or an **inline specification** (an object):

```yaml
queries:
  # Reference the instance's persistent provider by name.
  # Requires `persistIndex: true` so the `rocksdb` provider is registered.
  - id: persistent-query
    query: "MATCH (n) RETURN n"
    storageBackend: rocksdb

  # Force in-memory indexes for this query, even when persistIndex is true.
  - id: volatile-query
    query: "MATCH (n) RETURN n"
    storageBackend:
      kind: memory
      enableArchive: true
```

> **Note**: `rocksdb` is the only persistent provider compiled into drasi-server, and it is only registered when `persistIndex: true`. Referencing a named backend that has not been registered will fail query startup.

#### Source Subscriptions

```yaml
sources:
  - sourceId: orders-db
    nodes: [Order, Customer]      # Optional: filter node labels
    relations: [PLACED_BY]        # Optional: filter relation labels
    pipeline: [decoder, mapper]   # Optional: middleware pipeline
```

#### Synthetic Joins

Create virtual relationships between nodes from different sources:

```yaml
queries:
  - id: order-customer-join
    query: |
      MATCH (o:Order)-[:CUSTOMER]->(c:Customer)
      RETURN o.id, c.name
    sources:
      - sourceId: orders-db
      - sourceId: customers-db
    joins:
      - id: CUSTOMER
        keys:
          - label: Order
            property: customer_id
          - label: Customer
            property: id
```

---

### Reactions

Reactions respond to query result changes.

#### Common Reaction Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `kind` | string | (required) | Reaction type |
| `id` | string | (required) | Unique reaction identifier |
| `queries` | array | (required) | Query IDs to subscribe to |
| `autoStart` | boolean | `true` | Start reaction automatically |

#### Log Reaction (`log`)

Writes query results to console output.

```yaml
reactions:
  - kind: log
    id: log-output
    queries: [my-query]
    autoStart: true
    defaultTemplate:
      added:
        template: "Added: {{json this}}"
      updated:
        template: "Updated: {{json this}}"
      deleted:
        template: "Deleted: {{json this}}"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `routes` | object | `{}` | Query-specific template configurations |
| `defaultTemplate` | object | (none) | Default template for all queries |

#### HTTP Reaction (`http`)

Sends query results to HTTP endpoints.

```yaml
reactions:
  - kind: http
    id: webhook
    queries: [my-query]
    baseUrl: https://api.example.com
    timeoutMs: 5000
    token: ${API_TOKEN}
    routes:
      my-query:
        added:
          url: /events
          method: POST
          headers:
            Content-Type: application/json
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `baseUrl` | string | `http://localhost` | Base URL for requests |
| `timeoutMs` | integer | `5000` | Request timeout in milliseconds |
| `token` | string | (none) | Bearer token for authorization |
| `routes` | object | `{}` | Query-specific endpoint configurations |

#### HTTP Adaptive Reaction (`http-adaptive`)

HTTP reaction with adaptive batching and retry logic.

```yaml
reactions:
  - kind: http-adaptive
    id: adaptive-webhook
    queries: [my-query]
    baseUrl: https://api.example.com
    timeoutMs: 5000
    adaptiveMinBatchSize: 1
    adaptiveMaxBatchSize: 1000
    adaptiveWindowSize: 100
    adaptiveBatchTimeoutMs: 1000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `adaptiveMinBatchSize` | integer | `1` | Minimum batch size |
| `adaptiveMaxBatchSize` | integer | `1000` | Maximum batch size |
| `adaptiveWindowSize` | integer | `100` | Window size for adaptive calculations |
| `adaptiveBatchTimeoutMs` | integer | `1000` | Batch timeout in milliseconds |

#### gRPC Reaction (`grpc`)

Streams query results via gRPC.

```yaml
reactions:
  - kind: grpc
    id: grpc-stream
    queries: [my-query]
    endpoint: grpc://localhost:50052
    timeoutMs: 5000
    batchSize: 100
    maxRetries: 3
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `endpoint` | string | `grpc://localhost:50052` | gRPC endpoint URL |
| `timeoutMs` | integer | `5000` | Connection timeout in milliseconds |
| `batchSize` | integer | `100` | Events per batch |
| `batchFlushTimeoutMs` | integer | `1000` | Batch flush timeout |
| `maxRetries` | integer | `3` | Maximum retry attempts |
| `connectionRetryAttempts` | integer | `5` | Connection retry attempts |
| `initialConnectionTimeoutMs` | integer | `10000` | Initial connection timeout |
| `metadata` | object | `{}` | Custom gRPC metadata key-value pairs |

#### gRPC Adaptive Reaction (`grpc-adaptive`)

gRPC reaction with adaptive batching.

```yaml
reactions:
  - kind: grpc-adaptive
    id: adaptive-grpc
    queries: [my-query]
    endpoint: grpc://localhost:50052
    adaptiveMinBatchSize: 1
    adaptiveMaxBatchSize: 1000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `endpoint` | string | `grpc://localhost:50052` | gRPC endpoint URL |
| `timeoutMs` | integer | `5000` | Connection timeout in milliseconds |
| `maxRetries` | integer | `3` | Maximum retry attempts |
| `connectionRetryAttempts` | integer | `5` | Connection retry attempts |
| `initialConnectionTimeoutMs` | integer | `10000` | Initial connection timeout |
| `metadata` | object | `{}` | Custom gRPC metadata key-value pairs |
| `adaptiveMinBatchSize` | integer | `1` | Minimum batch size |
| `adaptiveMaxBatchSize` | integer | `1000` | Maximum batch size |
| `adaptiveWindowSize` | integer | `100` | Window size for adaptive calculations |
| `adaptiveBatchTimeoutMs` | integer | `1000` | Batch timeout in milliseconds |

#### SSE Reaction (`sse`)

Streams query results via Server-Sent Events.

```yaml
reactions:
  - kind: sse
    id: sse-stream
    queries: [my-query]
    host: 0.0.0.0
    port: 8081
    ssePath: /events
    heartbeatIntervalMs: 30000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `0.0.0.0` | Listen address |
| `port` | integer | `8080` | Listen port |
| `ssePath` | string | `/events` | SSE endpoint path |
| `heartbeatIntervalMs` | integer | `30000` | Heartbeat interval in milliseconds |

#### Platform Reaction (`platform`)

Publishes query results to Redis Streams in CloudEvent format.

```yaml
reactions:
  - kind: platform
    id: redis-publisher
    queries: [my-query]
    redisUrl: redis://localhost:6379
    emitControlEvents: false
    batchEnabled: true
    batchMaxSize: 100
    batchMaxWaitMs: 100
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `redisUrl` | string | (required) | Redis connection URL |
| `pubsubName` | string | (auto) | Pub/sub channel name |
| `sourceName` | string | (auto) | Source identifier in events |
| `maxStreamLength` | integer | (unlimited) | Maximum stream length |
| `emitControlEvents` | boolean | `false` | Emit control events |
| `batchEnabled` | boolean | `false` | Enable batching |
| `batchMaxSize` | integer | `100` | Maximum batch size |
| `batchMaxWaitMs` | integer | `100` | Maximum wait time for batch |

#### Profiler Reaction (`profiler`)

Collects performance metrics for queries.

```yaml
reactions:
  - kind: profiler
    id: query-profiler
    queries: [my-query]
    windowSize: 100
    reportIntervalSecs: 60
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `windowSize` | integer | `100` | Metrics window size |
| `reportIntervalSecs` | integer | `60` | Report interval in seconds |

---

### Multi-Instance Configuration

For advanced use cases requiring isolated processing environments, configure multiple DrasiLib instances:

```yaml
apiVersion: drasi.io/v1
host: 0.0.0.0
port: 8080
logLevel: info

instances:
  - id: analytics
    persistIndex: true
    stateStore:
      kind: redb
      path: ./data/analytics-state.redb
    sources:
      - kind: postgres
        id: analytics-db
        # ... source config
    queries:
      - id: high-value-orders
        query: "MATCH (o:Order) WHERE o.total > 1000 RETURN o"
        sources:
          - sourceId: analytics-db
    reactions:
      - kind: log
        id: analytics-log
        queries: [high-value-orders]

  - id: monitoring
    persistIndex: false
    sources:
      - kind: http
        id: metrics-api
        host: 0.0.0.0
        port: 9001
    queries:
      - id: alert-threshold
        query: "MATCH (m:Metric) WHERE m.value > m.threshold RETURN m"
        sources:
          - sourceId: metrics-api
    reactions:
      - kind: sse
        id: alert-stream
        queries: [alert-threshold]
        port: 8082
```

Each instance has:
- Its own isolated namespace for sources, queries, and reactions
- Optional separate state store and index persistence settings
- API access via `/api/v1/instances/{instanceId}/...`

---

### Environment Variable Interpolation

All configuration values support environment variable substitution:

```yaml
apiVersion: drasi.io/v1
host: ${SERVER_HOST:-0.0.0.0}
port: ${SERVER_PORT:-8080}

sources:
  - kind: postgres
    id: production-db
    host: ${DB_HOST}
    password: ${DB_PASSWORD}  # Required - fails if not set
```

**Syntax:**
- `${VAR}` - Required variable, fails if not set
- `${VAR:-default}` - Optional variable with default value

## REST API

The server exposes a REST API at `http://localhost:8080` (default). For complete API documentation with all request/response schemas, see the interactive Swagger UI at `/api/v1/docs/`.

#### Request Body Formats (JSON and YAML)

Every endpoint that accepts a request body accepts **both JSON and YAML** payloads
interchangeably. The format is selected from the request's `Content-Type` header:
send `application/json` (the default) for JSON, or one of `application/yaml`,
`application/x-yaml`, `text/yaml`, `text/x-yaml`, or `text/vnd.yaml` for YAML.
When no `Content-Type` is provided, the body is parsed as JSON.

```bash
# Create a query using a YAML body
curl -X POST http://localhost:8080/api/v1/queries \
  -H "Content-Type: application/yaml" \
  --data-binary $'id: high-values\nquery: "MATCH (n) WHERE n.value > 75 RETURN n"\nautoStart: true'
```

### API Documentation

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check (returns `{"status": "ok"}`) |
| `GET /api/versions` | List available API versions |
| `GET /api/v1/docs/` | Interactive Swagger UI |
| `GET /api/v1/openapi.json` | OpenAPI 3.0 specification |

### Instances API

```bash
# List all instances
curl http://localhost:8080/api/v1/instances

# Create new instance
curl -X POST http://localhost:8080/api/v1/instances \
  -H "Content-Type: application/json" \
  -d '{"id": "my-instance", "persistIndex": false}'
```

### Sources API

```bash
# List all sources
curl http://localhost:8080/api/v1/sources

# Create a mock source
curl -X POST http://localhost:8080/api/v1/sources \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "mock",
    "id": "test-source",
    "autoStart": true,
    "dataType": {"type": "sensorReading", "sensorCount": 5},
    "intervalMs": 2000
  }'

# Get source details
curl http://localhost:8080/api/v1/sources/test-source

# Start/stop source
curl -X POST http://localhost:8080/api/v1/sources/test-source/start
curl -X POST http://localhost:8080/api/v1/sources/test-source/stop

# Delete source
curl -X DELETE http://localhost:8080/api/v1/sources/test-source

# Push data to HTTP source (via proxy to avoid CORS)
curl -X POST http://localhost:8080/api/v1/sources/http-source/push \
  -H "Content-Type: application/json" \
  -d '{"nodes": [{"id": "n1", "labels": ["Item"], "properties": {"name": "test"}}]}'
```

### Queries API

```bash
# List all queries
curl http://localhost:8080/api/v1/queries

# Create a query
curl -X POST http://localhost:8080/api/v1/queries \
  -H "Content-Type: application/json" \
  -d '{
    "id": "high-values",
    "query": "MATCH (n:Item) WHERE n.value > 100 RETURN n",
    "queryLanguage": "Cypher",
    "sources": [{"sourceId": "test-source"}],
    "autoStart": true
  }'

# Get query details
curl http://localhost:8080/api/v1/queries/high-values

# Get current query results
curl http://localhost:8080/api/v1/queries/high-values/results

# Example response:
# {
#   "results": [
#     {"n.id": "item-1", "n.value": 150},
#     {"n.id": "item-2", "n.value": 200}
#   ]
# }

# Start/stop query
curl -X POST http://localhost:8080/api/v1/queries/high-values/start
curl -X POST http://localhost:8080/api/v1/queries/high-values/stop

# Delete query
curl -X DELETE http://localhost:8080/api/v1/queries/high-values
```

### Reactions API

```bash
# List all reactions
curl http://localhost:8080/api/v1/reactions

# Create a log reaction
curl -X POST http://localhost:8080/api/v1/reactions \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "log",
    "id": "my-logger",
    "queries": ["high-values"],
    "autoStart": true
  }'

# Create an HTTP webhook reaction
curl -X POST http://localhost:8080/api/v1/reactions \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "http",
    "id": "my-webhook",
    "queries": ["high-values"],
    "autoStart": true,
    "baseUrl": "https://api.example.com",
    "routes": {
      "high-values": {
        "added": {
          "url": "/events",
          "method": "POST"
        }
      }
    }
  }'

# Get reaction details
curl http://localhost:8080/api/v1/reactions/my-logger

# Start/stop reaction
curl -X POST http://localhost:8080/api/v1/reactions/my-logger/start
curl -X POST http://localhost:8080/api/v1/reactions/my-logger/stop

# Delete reaction
curl -X DELETE http://localhost:8080/api/v1/reactions/my-logger
```

### SSE Events Stream

Subscribe to real-time component events:

```bash
# Stream all component events (sources, queries, reactions)
curl -N http://localhost:8080/api/v1/events

# Example events:
# data: {"type":"SourceStatusChanged","sourceId":"test-source","status":"Running"}
# data: {"type":"QueryResultAdded","queryId":"high-values","result":{...}}
```

### Per-Component Logs and Events

```bash
# Get source events (paginated)
curl http://localhost:8080/api/v1/sources/test-source/events

# Stream source events (SSE)
curl -N http://localhost:8080/api/v1/sources/test-source/events/stream

# Get source logs
curl http://localhost:8080/api/v1/sources/test-source/logs

# Stream source logs (SSE)
curl -N http://localhost:8080/api/v1/sources/test-source/logs/stream

# Same endpoints available for queries and reactions:
# /api/v1/queries/{id}/events
# /api/v1/queries/{id}/events/stream
# /api/v1/queries/{id}/logs
# /api/v1/queries/{id}/logs/stream
# /api/v1/reactions/{id}/events
# /api/v1/reactions/{id}/events/stream
# /api/v1/reactions/{id}/logs
# /api/v1/reactions/{id}/logs/stream
```

### Solution Templates API

```bash
# List available solution templates
curl http://localhost:8080/api/v1/catalog/solutions

# Get solution template details
curl http://localhost:8080/api/v1/catalog/solutions/iot-temperature-monitor

# Deploy a solution template
curl -X POST http://localhost:8080/api/v1/instances/default/solutions \
  -H "Content-Type: application/json" \
  -d '{
    "templateId": "iot-temperature-monitor",
    "variables": {"TEMP_THRESHOLD": "80"}
  }'

# Create solution template from current instance
curl -X POST http://localhost:8080/api/v1/instances/default/catalog/solutions \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Template",
    "description": "Custom pipeline configuration",
    "version": "1.0.0"
  }'
```

### Instance-Specific Routes

All component routes support instance-specific access:

```bash
# Operations on specific instance
curl http://localhost:8080/api/v1/instances/production/sources
curl http://localhost:8080/api/v1/instances/production/queries
curl http://localhost:8080/api/v1/instances/production/reactions
curl -N http://localhost:8080/api/v1/instances/production/events

# Convenience routes operate on the first/default instance
curl http://localhost:8080/api/v1/sources  # Same as /instances/{first}/sources
```

### Response Format

Successful responses:
```json
{
  "success": true,
  "data": { ... }
}
```

Error responses:
```json
{
  "success": false,
  "error": "Error message describing what went wrong"
}
```

### Common HTTP Status Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 201 | Created |
| 400 | Bad request (invalid JSON or missing fields) |
| 404 | Resource not found |
| 409 | Conflict (resource already exists) |
| 500 | Internal server error |

---

## VS Code Extension

The Drasi Server VS Code extension provides integrated development tools for managing Drasi resources.

### Installation

**From VS Code:**
1. Open VS Code
2. Go to Extensions (Ctrl+Shift+X / Cmd+Shift+X)
3. Search for "Drasi Server"
4. Click Install

**From VSIX:**
```bash
cd dev-tools/vscode/drasi-server
npm install
npm run compile
npm run package
# Install the generated .vsix file
```

### Features

| Feature | Description |
|---------|-------------|
| **Workspace Explorer** | Browse YAML files containing Drasi resources |
| **Drasi Explorer** | View and interact with live resources on the server |
| **CodeLens Actions** | Apply resources or launch server from YAML files |
| **Launch Server** | Start drasi-server from a config file |
| **Query Debugger** | Debug queries with real-time results |
| **Event Streaming** | Stream events and logs in real time |
| **YAML IntelliSense** | Auto-completion and validation from OpenAPI schema |
| **YAML Generators** | Scaffold Source, Query, Reaction YAML via prompts |

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `drasiServer.url` | `http://localhost:8080` | Server API URL |
| `drasiServer.instanceId` | (empty) | Instance ID (uses first if empty) |
| `drasiServer.binaryPath` | (empty) | Path to drasi-server binary |
| `drasiServer.connections` | `[]` | Saved server connections |

### Usage

**Managing Servers:**
1. Open the **Drasi** view in the Activity Bar
2. Right-click to add/edit server connections
3. Click the plug icon to switch active server

**Launching the Server:**
1. Open a Drasi YAML config file
2. Click the **▶ Launch Server** CodeLens at the top
3. Select the drasi-server binary (first time only)
4. Confirm the port number

**Applying Resources:**
- Individual sources, queries, and reactions show an **Apply** CodeLens
- Click to upsert the resource to the connected server

**Creating Resources:**
- Right-click in a YAML editor
- Select **Create Source YAML**, **Create Query YAML**, or **Create Reaction YAML**
- Follow the prompts to scaffold a new resource

**Debugging Queries:**
- Right-click a query in the Drasi Explorer
- Select **Watch** to open the query debugger
- See real-time results as data flows through

**Streaming Events/Logs:**
- Right-click any component (source, query, reaction)
- Select **Stream Events** or **Stream Logs**
- Watch real-time output in a VS Code panel

---

## Development Utilities

Drasi Server includes a Makefile with common development commands.

### Available Commands

```bash
make help           # Show all available commands
```

**Getting Started:**
```bash
make run            # Build (debug) and run the server
make run-release    # Build (release) and run the server
make demo           # Run the getting-started example
```

For a first setup, use [the explicit local setup path](docs/setup.md).
`make setup` can start a server, and both `doctor` variants still check an
obsolete submodule location. They are not authoritative prerequisite checks
for this sibling-checkout branch.

**Development:**
```bash
make build          # Build UI, then debug binary
make build-release  # Build UI, then release binary
make dev-build      # Format, lint, and test
make clean-dev-build # Clean, format, lint, and test
make test           # Run all tests
make clippy         # Run linter
make fmt            # Format code
make fmt-check      # Check formatting
```

**Utilities:**
```bash
make clean          # Clean build artifacts
make demo-cleanup   # Stop demo containers; can also remove demo volumes
make vscode-test    # Run VS Code extension tests
```

Use the built binary's `validate --config <path> --plugins-dir <path>` directly:
`make validate` hides stderr and masks errors. See [validation behavior](docs/setup.md#what-validation-does-and-does-not-prove).

---

## Docker Deployment

See [DOCKER.md](DOCKER.md) for the tracked Compose filenames, published-image
scope, port/mount safety, and persistence requirements. The current Dockerfile
does not copy sibling `drasi-core` path dependencies into its build context;
`docker build .` / `make docker-build` are not a working source-build recipe for
this branch. Use [native local setup](docs/setup.md) for this checkout.

---

## Use Cases

### Real-Time Inventory Alerts

```yaml
queries:
  - id: low-stock-alert
    query: |
      MATCH (p:Product)
      WHERE p.quantity <= p.reorder_point
      RETURN p.sku, p.name, p.quantity, p.reorder_point
    sources:
      - sourceId: inventory-db

reactions:
  - kind: http
    id: reorder-webhook
    queries: [low-stock-alert]
    baseUrl: https://purchasing.example.com
    routes:
      low-stock-alert:
        added:
          url: /reorder
          method: POST
```

### Fraud Detection

```yaml
queries:
  - id: suspicious-transactions
    query: |
      MATCH (t:Transaction)
      WHERE t.amount > 10000
        AND t.country <> t.account_country
      RETURN t.id, t.account_id, t.amount, t.country
    sources:
      - sourceId: transactions-db

reactions:
  - kind: sse
    id: fraud-alerts
    queries: [suspicious-transactions]
    port: 8081
```

---

## Complete Configuration Examples

### Example 1: PostgreSQL CDC with Webhook

A production-ready setup monitoring PostgreSQL changes and sending webhooks.

```yaml
apiVersion: drasi.io/v1
id: production-pipeline
host: 0.0.0.0
port: 8080
logLevel: info
persistConfig: true
enableUi: true

# Persist state across restarts
stateStore:
  kind: redb
  path: ${DATA_DIR:-./data}/state.redb

sources:
  - kind: postgres
    id: orders-db
    autoStart: true
    host: ${DB_HOST}
    port: ${DB_PORT:-5432}
    database: ${DB_NAME}
    user: ${DB_USER}
    password: ${DB_PASSWORD}
    tables: [orders, customers, products]
    slotName: drasi_orders_slot
    publicationName: drasi_orders_pub
    sslMode: ${DB_SSL_MODE:-prefer}
    tableKeys:
      - table: orders
        keyColumns: [id]
      - table: customers
        keyColumns: [customer_id]
    bootstrapProvider:
      kind: postgres
      host: ${DB_HOST}
      port: ${DB_PORT:-5432}
      database: ${DB_NAME}
      user: ${DB_USER}
      password: ${DB_PASSWORD}
      tables: [orders, customers, products]

queries:
  - id: high-value-orders
    query: |
      MATCH (o:orders)
      WHERE o.total > ${ORDER_THRESHOLD:-1000}
      RETURN o.id, o.customer_id, o.total, o.status
    queryLanguage: Cypher
    sources:
      - sourceId: orders-db
    autoStart: true
    enableBootstrap: true

  - id: new-customers
    query: |
      MATCH (c:customers)
      WHERE c.created_at > datetime() - duration('P7D')
      RETURN c.customer_id, c.email, c.created_at
    queryLanguage: Cypher
    sources:
      - sourceId: orders-db
    autoStart: true

reactions:
  - kind: http
    id: order-webhook
    queries:
      - high-value-orders
    autoStart: true
    baseUrl: ${WEBHOOK_BASE_URL}
    token: ${WEBHOOK_TOKEN}
    timeoutMs: 10000
    routes:
      high-value-orders:
        added:
          url: /api/orders/high-value
          method: POST
          headers:
            Content-Type: application/json
          body: '{"event": "high_value_order", "order_id": "{{after.id}}", "total": {{after.total}}}'
        updated:
          url: /api/orders/updated
          method: POST
        deleted:
          url: /api/orders/cancelled
          method: POST

  - kind: log
    id: debug-logger
    queries:
      - high-value-orders
      - new-customers
    autoStart: true
```

### Example 2: Multi-Instance Setup

Isolated environments for different use cases.

```yaml
apiVersion: drasi.io/v1
host: 0.0.0.0
port: 8080
logLevel: info
enableUi: true

instances:
  # Production analytics instance
  - id: analytics
    persistIndex: true
    stateStore:
      kind: redb
      path: ./data/analytics-state.redb
    sources:
      - kind: postgres
        id: analytics-db
        autoStart: true
        host: ${ANALYTICS_DB_HOST}
        port: 5432
        database: analytics
        user: ${DB_USER}
        password: ${DB_PASSWORD}
        tables: [events, metrics, users]
        bootstrapProvider:
          kind: postgres
    queries:
      - id: active-users
        query: "MATCH (u:users) WHERE u.last_active > datetime() - duration('PT1H') RETURN count(u) as active_count"
        sources:
          - sourceId: analytics-db
        autoStart: true
    reactions:
      - kind: sse
        id: metrics-stream
        queries: [active-users]
        host: 0.0.0.0
        port: 8082
        autoStart: true

  # Development/testing instance
  - id: development
    persistIndex: false
    sources:
      - kind: mock
        id: test-data
        autoStart: true
        dataType:
          type: sensorReading
          sensorCount: 10
        intervalMs: 1000
    queries:
      - id: test-query
        query: "MATCH (s:SensorReading) WHERE s.temperature > 28 RETURN s"
        sources:
          - sourceId: test-data
        autoStart: true
    reactions:
      - kind: log
        id: test-logger
        queries: [test-query]
        autoStart: true
```

### Example 3: Webhook Gateway with Authentication

Receiving events from external services like GitHub or Stripe.

```yaml
apiVersion: drasi.io/v1
host: 0.0.0.0
port: 8080
logLevel: info
enableUi: true

sources:
  # GitHub webhook source
  - kind: http
    id: github-events
    autoStart: true
    host: 0.0.0.0
    port: 9000
    webhooks:
      errorBehavior: reject
      cors:
        allowOrigins: ["*"]
      routes:
        - path: /github/webhook
          methods: [POST]
          auth:
            signature:
              type: hmac-sha256
              secretEnv: GITHUB_WEBHOOK_SECRET
              header: X-Hub-Signature-256
              prefix: "sha256="
          mappings:
            # Push events
            - when:
                header: X-GitHub-Event
                equals: push
              elementType: node
              operation: insert
              template:
                id: "commit-{{payload.head_commit.id}}"
                labels: ["Commit", "GitHubEvent"]
                properties:
                  sha: "{{payload.head_commit.id}}"
                  message: "{{payload.head_commit.message}}"
                  author: "{{payload.head_commit.author.name}}"
                  repo: "{{payload.repository.full_name}}"
                  branch: "{{payload.ref}}"
            # Pull request events
            - when:
                header: X-GitHub-Event
                equals: pull_request
              elementType: node
              operationFrom: "$.action"
              operationMap:
                opened: insert
                closed: delete
                synchronize: update
              template:
                id: "pr-{{payload.pull_request.id}}"
                labels: ["PullRequest", "GitHubEvent"]
                properties:
                  number: "{{payload.pull_request.number}}"
                  title: "{{payload.pull_request.title}}"
                  author: "{{payload.pull_request.user.login}}"
                  state: "{{payload.pull_request.state}}"

  # Stripe webhook source
  - kind: http
    id: stripe-events
    autoStart: true
    host: 0.0.0.0
    port: 9001
    webhooks:
      routes:
        - path: /stripe/webhook
          methods: [POST]
          auth:
            signature:
              type: hmac-sha256
              secretEnv: STRIPE_WEBHOOK_SECRET
              header: Stripe-Signature
          mappings:
            - when:
                field: "$.type"
                contains: "payment_intent"
              elementType: node
              operationFrom: "$.type"
              operationMap:
                payment_intent.created: insert
                payment_intent.succeeded: update
                payment_intent.canceled: delete
              template:
                id: "payment-{{payload.data.object.id}}"
                labels: ["Payment", "StripeEvent"]
                properties:
                  amount: "{{payload.data.object.amount}}"
                  currency: "{{payload.data.object.currency}}"
                  status: "{{payload.data.object.status}}"

queries:
  - id: new-commits
    query: "MATCH (c:Commit) RETURN c ORDER BY c.timestamp DESC"
    sources:
      - sourceId: github-events
    autoStart: true

  - id: large-payments
    query: "MATCH (p:Payment) WHERE p.amount > 10000 RETURN p"
    sources:
      - sourceId: stripe-events
    autoStart: true

reactions:
  - kind: log
    id: event-logger
    queries:
      - new-commits
      - large-payments
    autoStart: true
```

### Example 4: Environment-Variable-Heavy Configuration

Demonstrating extensive use of environment variables for different environments.

```yaml
# config/server.yaml
# Run with: source .env && cargo run -- --config config/server.yaml

apiVersion: drasi.io/v1
id: "${SERVER_ID:-drasi-${ENVIRONMENT:-dev}}"
host: "${SERVER_HOST:-0.0.0.0}"
port: "${SERVER_PORT:-8080}"
logLevel: "${LOG_LEVEL:-info}"
persistConfig: true   # Boolean fields don't support env var substitution
persistIndex: false
enableUi: true

# Capacity tuning via env vars
defaultPriorityQueueCapacity: "${QUEUE_CAPACITY:-10000}"
defaultDispatchBufferCapacity: "${BUFFER_CAPACITY:-1000}"

stateStore:
  kind: redb
  path: "${STATE_STORE_PATH:-./data/state.redb}"

sources:
  - kind: postgres
    id: main-db
    autoStart: true  # Boolean fields don't support env var substitution
    host: "${DB_HOST}"
    port: "${DB_PORT:-5432}"
    database: "${DB_NAME}"
    user: "${DB_USER}"
    password: "${DB_PASSWORD}"
    sslMode: "${DB_SSL_MODE:-prefer}"
    tables: ["${DB_TABLES:-orders,customers}"]
    slotName: "${DB_SLOT_NAME:-drasi_slot}"
    publicationName: "${DB_PUB_NAME:-drasi_publication}"
    bootstrapProvider:
      kind: postgres

queries:
  - id: monitored-changes
    query: "${MAIN_QUERY:-MATCH (n) RETURN n}"
    queryLanguage: Cypher
    sources:
      - sourceId: main-db
    autoStart: true
    enableBootstrap: true

reactions:
  - kind: log  # Enum fields don't support env var substitution
    id: main-reaction
    queries:
      - monitored-changes
    autoStart: true
```

**Corresponding .env file:**
```bash
# .env
ENVIRONMENT=production
SERVER_ID=prod-drasi-01
SERVER_PORT=8080
LOG_LEVEL=info

# Database
DB_HOST=db.example.com
DB_PORT=5432
DB_NAME=production
DB_USER=drasi_app
DB_PASSWORD=your-secure-password-here
DB_SSL_MODE=require

# Features
PERSIST_CONFIG=true
PERSIST_INDEX=true
ENABLE_UI=false
AUTO_START_SOURCES=true
AUTO_START_QUERIES=true
AUTO_START_REACTIONS=true

# Tuning
QUEUE_CAPACITY=50000
BUFFER_CAPACITY=5000
```

---

## Troubleshooting

### Web UI Not Appearing

**Symptom:** Navigating to `/ui` returns 404 or blank page.

**Solutions:**
1. Check if UI is enabled in config:
   ```yaml
   enableUi: true
   ```
2. Use command line flag: `drasi-server --enable-ui`
3. Verify the server logs show "Web UI: enabled" on startup

### Configuration Changes Not Persisting

**Symptom:** Changes made via API are lost on restart.

**Solutions:**
1. Ensure `persistConfig: true` in config (default)
2. Check the config file is writable
3. If running in Docker, ensure config directory is mounted as a volume

```yaml
# Config file setting
persistConfig: true  # Default - saves API changes to config file
```

### Query Not Receiving Data

**Symptom:** Query created but results are always empty.

**Checklist:**
1. **Check source status:**
   ```bash
   curl http://localhost:8080/api/v1/sources/my-source
   # Should show "status": "Running"
   ```

2. **Verify query is started:**
   ```bash
   curl http://localhost:8080/api/v1/queries/my-query
   # Should show "status": "Running"
   ```

3. **Check query subscription references correct source:**
   ```bash
   curl http://localhost:8080/api/v1/queries/my-query
   # Check "sources" array contains correct sourceId
   ```

4. **Enable debug logging:**
   ```bash
   RUST_LOG=debug cargo run
   ```

5. **Check for label matching issues:**
   - Query must match node labels generated by the source
   - For mock sources: `SensorReading`, `Counter`, or `Generic`

### Bootstrap Not Working

**Symptom:** Query starts but doesn't have initial data from source.

**Solutions:**
1. Ensure `enableBootstrap: true` on the query (default):
   ```yaml
   queries:
     - id: my-query
       enableBootstrap: true  # Default
   ```

2. Verify the source has a bootstrap provider configured:
   ```yaml
   sources:
     - kind: postgres
       id: my-db
       bootstrapProvider:
         kind: postgres
         # ... config
   ```

3. Check bootstrap provider has matching table/label configuration

### PostgreSQL Connection Failing

**Common issues:**

1. **Replication slot issues:**
   ```bash
   # Check existing slots
   psql -c "SELECT * FROM pg_replication_slots;"
   
   # Drop stale slot if needed
   psql -c "SELECT pg_drop_replication_slot('drasi_slot');"
   ```

2. **WAL level not set:**
   ```sql
   -- Must be 'logical' for CDC
   SHOW wal_level;
   
   -- Set in postgresql.conf:
   -- wal_level = logical
   ```

3. **Publication not created:**
   ```sql
   -- Create publication for all tables
   CREATE PUBLICATION drasi_publication FOR ALL TABLES;
   
   -- Or for specific tables
   CREATE PUBLICATION drasi_publication FOR TABLE orders, customers;
   ```

### Port Already in Use

```bash
# Use a different port
cargo run -- --port 9090

# Or in config
port: 9090

# Find what's using the port
lsof -i :8080
```

### Docker Container Won't Start

```bash
# Check logs
docker compose logs drasi-server

# Common fixes:
# - Config file syntax error: validate with `drasi-server validate`
# - Permission issues: check config mount ownership for the container user (see DOCKER.md)
# - Database not ready: wait for postgres health check
```

### Debug Logging

```bash
# Basic debug logging
RUST_LOG=debug cargo run

# Detailed Drasi logging
RUST_LOG=drasi_server=trace cargo run

# Specific component logging
RUST_LOG=drasi_server::api=debug cargo run

# Multiple log levels
RUST_LOG=info,drasi_server=debug,drasi_lib=trace cargo run
```

### Component Stuck in Starting/Stopping State

**Symptom:** Component shows "Starting" or "Stopping" indefinitely.

**Solutions:**
1. Check server logs for errors
2. Delete and recreate the component
3. Restart the server

```bash
# Force delete
curl -X DELETE http://localhost:8080/api/v1/sources/stuck-source

# Restart server
docker compose restart drasi-server
```

---

## Building from Source

Use [docs/setup.md](docs/setup.md) for the canonical build instructions.
It covers the pinned Rust toolchain, sibling Core checkout, native libraries,
UI-before-Rust build order, and separate plugin builds. The former static/dynamic
feature tables and `make build-dynamic*` instructions described removed
interfaces. Server's `cargo xtask` now manages vendored native libraries, not
plugin builds; generic plugin building belongs to Core.

### Cross-Compilation

The [Makefile](Makefile) retains `build-cross` and `build-cross-release`;
[Cross.toml](Cross.toml) defines Linux GNU/musl containers for x86_64 and aarch64.
These advanced recipes require target-native dependencies and access to
`../drasi-core` inside the build container. They do not build the UI or plugins,
and the patch-path mount helper does not discover the active sibling paths in
`Cargo.toml`. Start with the native build unless you have supplied that cross
environment; do not substitute the removed `build-dynamic-cross` targets.

## License

Apache License 2.0. See [LICENSE](LICENSE) for details.

## Related Projects

- [DrasiLib](https://github.com/drasi-project/drasi-core/tree/workgraph-generic-recovery/lib) - Generic event processing library in the sibling Core checkout
- [WorkGraph setup](https://github.com/drasi-project/drasi-workgraph/blob/workgraph-generic-recovery/docs/setup/README.md) - WorkGraph host and Sandbox repository setup, owned by `drasi-workgraph`
- [Drasi](https://github.com/drasi-project) - Main Drasi project
- [Drasi Documentation](https://drasi.io/) - Complete documentation

## Support

- **Issues**: [GitHub Issues](https://github.com/drasi-project/drasi-server/issues)
