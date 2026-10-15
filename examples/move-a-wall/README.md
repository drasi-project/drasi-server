# Obstacle Impact

A small **geometry computation graph running inside the stock `drasi-server`
executable**. Drag a maintenance barrier across a planned cart
path; a native geometry transformer creates an `Obstruction`; a continuous
query joins the affected cart, task and destination. Move it away and the
results retract.

The display name is **Obstacle Impact**; the example directory and graph instance
ID remain `move-a-wall`.

The solution is declared in the checked-in [`server.yaml`](server.yaml):
components, continuous queries, joins, resources and connections. There is no
configuration generator or generated YAML prerequisite.

This is a change-processing example, not a scheduler, autonomous robot demo,
route planner or safety system. The carts never move. There is no prediction
clock, physics loop, rerouting, or robot stop/resume command.

## Quick Start

Use this ComputationGraph source checkout with the matching sibling `drasi-core`
checkout and the [build prerequisites](#build-prerequisites) installed. No Docker,
database, broker, or GPU is needed.

From the **`drasi-server` repository root**:

```sh
cd examples/move-a-wall
./demo setup && ./demo start
```

Open **http://127.0.0.1:5421**, then drag the movable wall across Ada's path.
The impact appears in the live query results; move the wall away to retract it.
Use `127.0.0.1`, not `localhost`, because the command endpoint checks the origin.

**Ctrl-C stops the example.** On subsequent runs, use `./demo start`.

Setup builds the current in-tree React SDK, both UIs, the standard Server and the
two native plugins. There is no separate UI preparation, historical Git fetch,
vendored SDK, or generated configuration step. The first native build can take
many minutes; later builds reuse the Server's Cargo cache. See
[Detailed setup and development](#detailed-setup-and-development) for prerequisites,
configuration edits, build outputs and troubleshooting.

## A 90-second presentation

Click the circled **i** beside **Obstacle Impact** for a full-page architecture
overlay. It fades in with the title in the same position as the demo; reduced
motion preferences disable the fade. The demo stays mounted underneath: opening and closing the overlay
does not navigate, reconnect the queries or discard unsaved editor changes.
Hovering or keyboard-focusing a component or arrow only highlights it; it never
opens details. Click a component to see its responsibility, implementation,
inputs and outputs. The details stay open while the pointer moves; the next
click anywhere (including the popup or another item) or Escape dismisses them.
Keyboard users can open details with Enter or Space. Click an arrow for a
readable schema of its payload, including native graph/query changes, source
commands, HTTP snapshots and SSE result differences. Arrow details support the
same click/dismiss interactions. These are documented contracts, not live data;
catalog-access arrows are explicitly identified as resource access rather than
graph pipes. On small screens, expand **Connection schemas** for the same details.
Close the overlay with the **X** at the top
right (or Escape when no details are open); focus returns to the info button.
The overview distinguishes browser commands, graph changes, query rows and
result delivery. Its expanded introduction explains the Euclidean-distance
calculation and why cart radius plus clearance matters even when a path misses
an obstacle. The three inspection queries are grouped for readability.
The configured `query-results` sink, host REST API and configured `wall-ui`
native SSE sink have separate cards; the catalog is explained as a shared resource,
not an extra component. This is not live graph inspection. On small screens
it becomes a component list with input connections.

The overview itself has no Drasi provider, API calls, SSE connection or backend
requirement. It is also available directly at
[http://127.0.0.1:5421/about.html](http://127.0.0.1:5421/about.html) for standalone
presentations; its X returns to the demo when opened that way. `npm run build`
includes the demo and standalone overview in `ui/dist`.
From `ui/`, run `npm run check:about` to build and check the overview, arrow schemas, hover,
keyboard/touch interactions, responsive layouts and navigation in Playwright.
It serves only the built assets on an ephemeral loopback port, requires no
running backend, and does not change the live scene.

1. **Start clear.** Point to Ada's path to Packing and Grace's upward-bowing arc to
   Assembly. The shaded path is each cart's circular footprint plus clearance,
   not just a zero-width line. These are static plans, not moving carts.
   Grace starts near the bottom of the scene. Her arc is 25 fixed points in the
   existing polyline format, not a new curve type or a browser-side calculation.
2. **Move one input.** Drag an obstacle across a path; try **Movable wall**
   upward across Ada's path. A dashed
   ghost is explicitly tentative. On release the command goes to the source.
   The path/cart turn orange and the impact panel names **Ada / Deliver
   packaging / Packing station / Movable wall** from the real impact query.
3. **Explain the computation.** In **Follow the change**, directly below the
   scene, click the flow's
   **Source**, **Context query**, **Geometry transformer**, and **Impact query**
   stages. The selected stage is highlighted and its query ID is shown.
   The records directly below show actual input revisions, the filtered context, the stable
   `journey-ada/wall` obstruction, and the enriched result. This transformer is
   geometry; the same event/query plumbing is not inherently about scheduling.
   **SSE / UI** shows the geometry-status query result, not a transport event log.
4. **Retract.** Drag the wall back to its original area, or remove it. The
   obstruction and impact disappear without a cart event. Grace is unaffected.
5. **Show flexibility.** Reset. Choose **Deliver
   packaging** in **Edit scene → Selected object**, then edit its polyline to
   `[[2,6.5],[21,6.5]]`; the path-only edit also creates an obstruction. Toggle
   the wall inactive to show upstream query filtering. Reset again.

Pointer dragging is available for obstacles, paths and destinations. Select an
obstacle/destination and use arrow keys to move it in 0.25 steps. Every shape has a
keyboard-accessible coordinate editor; carts have radius/clearance controls.
The **Edit scene** panel keeps the selected object's properties beside the scene:
click an object or path in the scene, or use **Selected object** (including inactive
objects and carts without a journey). Use **Apply input change** to save or
**Remove object** to delete it. The **Add** buttons create a draft cart, journey,
convex polygon obstacle or destination; **Add object** submits it and **Cancel**
discards it. Names and active state are editable. All objects share one scene.
Stable IDs do not change on update. Unsupported polygons or out-of-range values
produce visible errors.

## What actually runs

The scene and geometry plugin uses the native SDK and shared message contracts;
it does not include the query engine or `drasi-lib`. Drasi Server owns the graph
and queries. The example's Rust tests select the runtime as a test dependency.
The Quick Start and checked-in configuration are unchanged.

```text
React input command (no geometry)
    |
    v
native scene source / authoritative in-memory store
    | graph ChangeEnvelope: SceneObject inserts/updates/sparse deletes
    +------> scene-inputs CQ ------------------------------+
    |                                                     |
    v                                                     |
geometry-context CQ (WHERE active = true, collect payloads) |
    | query ChangeEnvelope                                |
    v                                                     |
native Geometry Transformer                               |
    | graph ChangeEnvelope: Obstruction +/- and context    |
    +------> obstructions CQ ------------------------------+
    +------> geometry-status CQ ---------------------------+
    v                                                     |
affected-journeys CQ (cart + journey + destination + cause) |
    |                                                     |
    +----------> native SSE sink + real query snapshots ----+
                                  |
                                  v
                     @drasi/react headless hooks / React
```

All sources, CQs, the transformer and SSE execute **inside `drasi-server`**.
There is no runnable custom host, alternate query API, synthetic SSE producer,
browser geometry calculation, or derived-state polling service. The native
plugin uses the real computation-plugin SDK ABI, factory metadata, schema
validators, lifecycle and `GraphChangeCodec` / `QueryChangeCodec`.
The Server uses its built-in `drasi/continuous-query` factories, declared
`memoryIndexes` and `queryCatalog` resources, and one stock
`drasi/query-results-outlet` sink. Every CQ and the outlet share the same catalog;
each query's `out` port also connects to the outlet's `in` port. The example's
`geo` dependency supplies the geometry calculation.

### Ports and query roles

| Component | Input | Output / role |
| --- | --- | --- |
| `scene` (`move-a-wall/scene`) | HTTP `POST /commands`, not a graph input port | `out`: graph-change schema; authoritative input changes only |
| `scene-inputs` | `in`: graph-change schema from `scene` | `out`: query-row schema; all editable records, including inactive objects, revision and changed IDs |
| `geometry-context` | `in`: graph-change schema from `scene` | `out`: query-row schema; only active objects plus explicit source manifest |
| `geometry` (`move-a-wall/geometry`) | `in`: query-row schema from `geometry-context` only | `out`: graph-change schema; diffed obstructions, projected metadata and revision status |
| `obstructions` | `in`: geometry graph changes | `out`: query rows; exact transformer results for inspector/path highlighting |
| `affected-journeys` | `in`: geometry graph changes | `out`: query rows; synthetic joins `CART_TASK`, `TO_DESTINATION`, `BLOCKS`, `CAUSED_BY` enrich each obstruction |
| `geometry-status` | `in`: geometry graph changes | `out`: query rows; last computed input revision and result count |
| `query-results` | `in`: query-change envelopes from all five CQs | Stock handled sink publishes into the shared `QueryResultsCatalog` for real snapshots/subscriptions |
| `wall-ui` | `in`: direct query-change envelopes from all five CQs | Native SSE on `/events`; `@drasi/react` loads real snapshots and applies real deltas |

The native SSE sink explicitly configures `heartbeatIntervalMs: 5000`. This is
only a connection heartbeat; it never triggers geometry computation.
The UI uses the current `dev-tools/react` package through a local dependency,
not an archived SDK or a modified copy. Its public `DrasiClientProvider` binds
an application-owned client that validates the real native graph, query lifecycle
and bound query-to-SSE edges instead of requiring a legacy Reaction DTO. It does
not fabricate a legacy resource, bypass readiness checks or replace the SDK's
snapshot/reconnection and result-reconciliation behavior.

The obstruction is a stable graph record representing the relationship
`(journey_id, obstacle_id)`, not a newly minted event ID on each drag. One journey
can have several obstruction causes. Removing one cause cannot erase another.
The geometry output also projects active cart/task/destination/obstacle metadata
so the impact query is the only place that assembles the human-readable effect.

The source accepts `put`, `delete`, `reset`, and `clear` commands with an
`expected_revision`. One mutex serializes mutations; stale revisions are rejected.
A bounded graph channel is reserved before committing an input mutation. Each
accepted command emits one real graph envelope with only changed entity records
and a revision/active-membership manifest. Identical commands are no-ops.

The context CQ really filters and aggregates those records. The transformer
caches query rows, current scene objects and its last emitted graph records. It
requires the CQ's entity versions to match the explicit source manifest before
publishing a new revision. This is **not** an inferred time/quiet-period boundary
or a claim about external database transactions. A full initial fixture or empty
scene has an explicit manifest too. Sparse input/query deletes use stable
identities. Duplicate/older query generations/sequences and obsolete input
revisions cannot revive removed obstruction records.

Only this transformer's event handler invokes `geo`; it recomputes the tiny
scene synchronously and diffs the result. Metadata-only changes do not invent
new obstruction IDs. Its pure constructor does not start background geometry
work. Source lifecycle owns and closes its command listener.

### Geometry contract

- Coordinates, cart radii, clearances and distances are **abstract scene values**,
  not real-world measurements. The existing `distance_m` and `required_m`
  property names are retained for query/API compatibility.
- The scene is a 24 by 16 rectangle. Inputs must be finite and within it.
- Carts are circular, with radius 0.05-2 and clearance 0-2. A journey sweeps
  that circle along every segment of its polyline (2-32 points).
- Obstacles are **simple, strictly convex polygons**, 3-16 vertices, without a
  repeated closing vertex. Rectangles and convex angled shapes are supported.
  Concave, self-crossing, duplicate/collinear or degenerate rings are rejected,
  not treated as clear. No holes, curves or unsupported shape variants.
- `geo` computes the Euclidean distance between the whole polyline and the
  closed polygon. A crossing or segment inside the polygon has distance zero.
  An obstruction exists when the distance is at most radius + clearance +
  **1e-7**. Tangency/contact therefore counts as obstruction. Polygon validity
  also rejects edge lengths and cross products at/below 1e-7.
- Only active carts, journeys and obstacles interact.
  Deleting a cart or journey removes its obstructions. Orphaned journeys can be
  edited or repaired; they do not fabricate a cart. Deleting destination
  metadata retracts the enriched impact, not a still-existing geometric cause.
  The obstruction inspector remains independent of enrichment.
- Maximum 64 scene entities. Whole-scene recomputation is intentional at this
  scale; there is no claim of a spatial index, simulation or production safety.

Query feeds are delivered independently, not as an atomic multi-query UI
snapshot. Source and geometry revisions remain visible; pending, stale,
disconnected and query-local errors are not labeled current/clear. A reload or
Reconnect resubscribes through the SDK and reads actual current query snapshots.
Explicit Reconnect discards pending previews without resubmitting commands,
including after a volatile Server restart. Command requests have a ten-second
timeout; errors remain visible and queries remain the source of truth if the
HTTP outcome is uncertain. The HTTP command response only confirms acceptance,
never an obstruction.

## Detailed setup and development

### Build prerequisites

Use the existing matching ComputationGraph checkouts, with this sibling layout:

```text
workspace/
  drasi-core/
  drasi-server/
    examples/move-a-wall/
```

This is a source-built example, not a standalone published package. An older
checkout or published Server binary may lack its native graph resources or
readable configuration support. No additional checkout or branch switch is
needed.

Requirements:

- macOS or Linux, Git, Bash, make, and Python 3.11 or later.
- Rust/rustup. The example inherits the Server's
  [`rust-toolchain.toml`](../../rust-toolchain.toml); it has no separate toolchain
  pin. The host and both native plugins use that same toolchain.
- **Node.js 22 or 24** and npm.
- Native build tools and libraries, as below.
- Internet access for initial Cargo/npm downloads and at least **15 GB of free
  disk space** for a clean native build.

On **macOS**, install Xcode Command Line Tools if `cc` is unavailable
(`xcode-select --install`), then install the native libraries:

```sh
brew install jq protobuf pkg-config cmake
```

On **Debian/Ubuntu Linux**:

```sh
sudo apt-get update
sudo apt-get install -y build-essential clang libclang-dev cmake pkg-config \
  jq libjq-dev libonig-dev protobuf-compiler libssl-dev
```

Other Linux distributions need equivalent packages. The launcher discovers
libjq with `pkg-config`; set `JQ_LIB_DIR` only if your installation needs an
explicit library directory.

### What setup builds

Run `./demo setup` with the example stopped. It:

1. Installs and builds the checked-in `dev-tools/react` package.
2. Installs the example UI's locked dependencies with local packages linked to
   their source, then type-checks and builds the UI.
3. Runs the Server's standard `make build`, including its administration UI.
4. Builds the scene/geometry plugin and Core's individual
   `drasi-computation-sink-sse` plugin, then stages those two libraries in
   `.build/plugins`. No other Core network component is built or loaded.
5. Validates the checked-in `server.yaml`, including its nine native components.

Wait for **`Ready: ./demo start`**. A failure stops setup; fix the reported error
and rerun it rather than starting a partial build. There is no SDK archive,
tarball generation, custom Server executable, or configuration generator.

The default Cargo output is the Server's shared `target/`, not an example-local
Server build. Standard `CARGO_TARGET_DIR` overrides are supported; use the same
value for setup, start and tests. `CARGO_BUILD_JOBS` defaults to one to limit
native-build memory use. Native debug information is disabled, but symbols are
not stripped. A clean build can take many minutes, particularly for RocksDB.

The launcher starts the stock Server binary, waits for the initial
`geometry-status` query result, checks the administration UI, then starts the
Vite frontend/proxy. It copies the standard administration UI bundle into the
isolated `.build/ui/dist` runtime directory. Server logs are in
`.build/server.log`. The footer's **Drasi Server Web UI** link opens
**http://127.0.0.1:8421/ui/** on the same Server instance.

| Loopback port | Owner |
| --- | --- |
| 5421 | Vite: React assets and same-origin proxy only |
| 8421 | Stock Drasi Server REST API and administration UI |
| 8422 | Native `drasi.network/sse-sink`, inside Server |
| 8423 | Native scene source's input command endpoint, inside Server |

### Editing the solution

Edit `server.yaml`, not a Rust configuration program or `.build/server.yaml`.
The same file is used by setup, startup and the Rust query tests. Query text and
join definitions live there once, rather than being copied into Rust helpers.

`components` names each component's ID, factory `kind`, version and settings.
`resources` declares the shared query indexes and result catalog. The catalog
and `query-results` outlet expose real results through the stock Server API;
`wall-ui` sends those same results over SSE. `connections` specifies where
records flow. Because these components each have one input/output port, a
connection needs only the two component names. The Server checks the actual
port schemas; the YAML does not copy them. Output streams default to
`component-id/port-id`, with `defaultPipeCapacity: 32`.

After a configuration edit:

```sh
./demo validate
```

Stop and restart `./demo start` to apply it; no rebuild is needed for YAML-only
changes. `persistConfig: false` keeps API operations from rewriting the file.
The initial scene remains the small fixture in `src/model.rs`, loaded by reset.

### Confirming startup

The browser should show **Live query results** and two unobstructed journeys.
For a direct snapshot, use another terminal:

```sh
curl --fail --silent --show-error \
  http://127.0.0.1:8421/api/v1/instances/move-a-wall/queries/geometry-status/results
```

Before edits, the response's `data` array contains `id: "current"`,
`revision: 1`, `objects: 9`, and `obstructions: 0`.

### Stopping, rebuilding and build outputs

**Stop:** Ctrl-C in the launch terminal. The launcher signals only its own child
processes and waits for them; it never kills another listener. All state is
intentionally volatile. Restart the whole example for a fresh fixture and query
caches rather than restarting an individual native component.

**Rebuild:** stop the launcher, run `./demo setup`, then `./demo start`. Rebuild
after changing native code, Server/Core or the SDK. Do not replace a plugin
library while the Server has it loaded.

Example-local `.build/`, `artifacts/`, `ui/dist/` and `ui/node_modules/` are
ignored outputs. The Server Cargo target, administration UI and React SDK builds
are shared repository outputs, not private copies. Repeat setup after cleaning
required outputs. Earlier builds may have left an example-local `target/`,
`.build/react-source/` or `ui/vendor/`; setup no longer uses those directories.

This is a loopback-only development example. Plugin signature verification is
disabled only for its locally built libraries. Do not expose its command or
Server management ports to a network.

## Inspect and validate

Complete setup first. For the Rust/UI checks, run this with the demo stopped:

```sh
# From examples/move-a-wall:
./demo test                  # geometry/real-Cypher tests, Rust fmt/Clippy, UI tests/build
```

For live acceptance, run `./demo start` in one terminal. In a **second terminal**:

```sh
# From examples/move-a-wall, in a second terminal:
./demo check                 # real stock-server/API/SSE acceptance
(cd ui && npx playwright install chromium)   # once, if Chromium is not installed
./demo browser               # actual drag, reload, offline/reconnect, errors, reset
```

On Linux, use `npx playwright install --with-deps chromium` from `ui/` if the
browser's system dependencies are missing. The architecture-only browser check
is `(cd ui && npm run check:about)`; it needs Chromium but no running backend.

**`check` and `browser` reset and edit the scene.** Run them only when no one is
using it. They finish at the default fixture on success, not at your custom scene.
Do not start a second launcher while the first one still owns the demo's ports.

The real query tests cover crossing, finite footprint/clearance, near misses,
contact, polyline turns, overlapping causes, invalid shapes and inactive objects;
obstacle-only/path-only changes, stable insert/update/delete diffs, no-op commands,
cart/path/obstacle deletion, empty/bootstrap/reset, stale commands/envelopes,
upstream active filtering and downstream destination/task enrichment.
These tests use the real Cypher evaluator, not mocked query output. They are
test-only, not a substitute for the stock-server live check.

`check` is an acceptance script for native topology, exact enriched rows, active
filtering, multiple causes, retractions, reset and real SSE delivery. It requires
actual stock SSE `ADD`, stable-key `UPDATE` and `DELETE` rows and saves them in
ignored `artifacts/sse-changes.json`. `browser` writes screenshots
to ignored `artifacts/`. Example-specific runtime files, staged plugins, logs and
UI outputs stay under this example and are ignored. The Cargo target, stock UI,
React SDK, download caches and Playwright browser installation may be shared.

Useful actual Server endpoints:

```sh
curl -s http://127.0.0.1:8421/api/v1/instances/move-a-wall/computation
curl -s http://127.0.0.1:8421/api/v1/instances/move-a-wall/queries/affected-journeys/results
curl -N http://127.0.0.1:8422/events
```

### Troubleshooting

| Symptom | Action |
| --- | --- |
| `./demo` is not found | Change to `drasi-server/examples/move-a-wall`, not the coordinator or Server root. |
| Stock Server UI assets are unavailable or `/ui/` returns 404 | Rerun `./demo setup`; its standard Server build prepares the administration UI automatically. |
| `@drasi/react` cannot be resolved or its `dist/` is missing | Run `./demo setup`; it builds the in-tree SDK before installing the UI's local dependencies. No Git fetch or vendor tarball is needed. |
| `protoc`, a C/C++ compiler, CMake, libclang, or libjq is missing | Install the [build prerequisites](#build-prerequisites). For a nonstandard libjq installation, set `JQ_LIB_DIR` before rerunning setup. |
| `Run ./demo setup first`, missing plugin, or missing Server executable | Setup did not finish, or its build artifacts were cleaned. Rerun setup and wait for its `Ready` message. |
| `EADDRINUSE` | Stop the launcher that owns the conflicting port, or resolve the other service's ownership first. Do not kill unrelated processes. The fixed ports are listed above. |
| Commands fail origin checks | Open `http://127.0.0.1:5421`, not `localhost`, a file URL, or a different port. |
| Chromium executable is missing | Run `(cd ui && npx playwright install chromium)` after setup. |
| Native resource/ABI error or no initial `geometry-status` result | Check the matching source checkouts and `.build/server.log`; stop and rebuild the host and both plugins together with setup. Do not mix downloaded binaries with these local plugins. |

If startup fails, inspect `.build/server.log` rather than substituting a local
result model. Port conflicts fail without touching another service. If a native
component fails, inspect the computation endpoint and restart the whole example
after addressing the input/build issue. The default input/query/native state is
volatile; process restart intentionally starts fresh.

## Compatibility and verified evidence

Setup uses the current Server, sibling Core and in-tree React SDK. The revisions
below record **historical qualification**, not dependency pins or instructions
to fetch old source. The example and native query-catalog integration were
originally qualified at these committed revisions:

| Repository | Commit |
| --- | --- |
| `drasi-server` | `a70064fb2decfdc8496f5385d5140f9025ff6417` |
| `drasi-core` | `5f48406bfce641d83d7f881877a4a8cac663eeac` |
| `@drasi/react` source in the Server Git object store | `2a36f857526baa08304a698131854f222b40b108` |

Historical pre-migration qualification on 2026-10-07 used Core
`e46f6130b29d9268603585a5912e92d6839fa01f` and Server
`a7b564a8fbbfaf141cd078994275e09f13a798d9`, **both including uncommitted changes**.
The example lockfile now includes their current dependencies. The native scene
plugin, standard SSE plugin and stock Server were rebuilt together; ten Rust
tests, five UI tests, live query/SSE checks and the full Chromium interaction
check passed. The fresh verification instance shut down cleanly.

Native SSE requalification on 2026-10-08 used Core
`953479729465b412a16764b29c9e56c55a1531e1` and Server
`de32b37a956563bda8266f57b7de3a2b5e976e81`, including the uncommitted example
migration. The native network sink, direct graph edges and shared browser
validator passed live checks and the full Chromium interaction/reconnect run.
The shared-package JSX regression is covered alongside the graph validator.

Server support constructs a graph-scoped `QueryResultsCatalog` and validates
query/outlet catalog agreement. It does not replace query evaluation, the Server
API, or SSE, and requires no example-specific Core modification. An older stock
binary may reject the resource recipe; build from the compatible checkouts with
`./demo setup`. Rebuild all three artifacts after a Core/SDK update; an older
plugin left in `target/` is not evidence that the current source works.

The static file declares this resource alongside `memoryIndexes`:

```yaml
# Under computation.graph:
resources:
  wall-query-catalog:
    kind: queryCatalog
```

Each CQ has `resources.catalog: wall-query-catalog` as well as its index
dependency. The shared stock outlet has the same catalog dependency and handled
completion; all five query output ports connect to it. Descriptors and schemas
come from the actual factory implementations, not hand-written equivalents.

The readable `computation.graph` format requires the shared loader and factory
description support in the current source. The committed revisions above
document earlier runtime checks, not this configuration change. Older binaries
do not understand the new format; rebuild the current host with `./demo setup`.

Verified locally on macOS (Apple Silicon); the Linux prerequisites above are
provided for Linux source builds, not a claim of a Linux validation run:

- Ten Rust geometry/transformer tests using actual Cypher queries, including
  active filtering, scene-only records and task/destination enrichment; ten UI
  record/native-topology tests; formatting, Clippy, TypeScript and production UI build.
- Actual stock Server bootstrap: `geometry-status` returns revision 1,
  nine objects and zero obstructions.
- Real source commands through the native geometry transformer into query
  snapshots and native SSE, including exact enriched `ADD`, same-key rename
  `UPDATE`, and `DELETE` records.
- Chromium drag-to-impact and retraction, path-only edits, CRUD for all four
  entity kinds, reload, offline/reconnect, keyboard movement, visible
  unsupported-polygon errors, and reset-to-clear; screenshots captured from
  the live query-backed UI.
- Graceful launcher shutdown and a full restart restoring the revision-1 fixture.

There is no runnable custom host, query facade, fake SSE or local result mode.

For a limited **disconnected UI check only**, run `npm run dev` from `ui/`
without a backend, then from the example root run
`node ops/check-browser.mjs --disconnected`. It checks that errors/staleness
remain visible, commands are disabled, and no scene or "clear scene" result is
fabricated. It does not validate geometry/SSE integration. Stop Vite with Ctrl-C.
