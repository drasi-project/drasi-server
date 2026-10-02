# Obstacle Impact

A small **geometry computation graph running inside the stock `drasi-server`
executable**. Drag a maintenance barrier across a planned cart
path; a native geometry transformer creates an `Obstruction`; a continuous
query joins the affected cart, task and destination. Move it away and the
results retract.

The display name is **Obstacle Impact**; the example directory and graph instance
ID remain `move-a-wall`.

This is a change-processing example, not a scheduler, autonomous robot demo,
route planner or safety system. The carts never move. There is no prediction
clock, physics loop, rerouting, or robot stop/resume command.

## Getting started

**Run setup before start, including after cleaning the example.** `./demo start`
does not install dependencies or rebuild missing binaries. Do not run `npm ci`
in the example's `ui/` first: its local `@drasi/react` package is created by
`./demo setup`.

### 1. Check the source checkouts and tools

This is a source-built ComputationGraph example, not a standalone published
package. Use the existing sibling checkouts on
`agentofreality-parallel-computation-graph`:

```text
drasi-computation-graph/
  drasi-core/
  drasi-server/
    examples/move-a-wall/
```

The commands below use this workspace's absolute path. If your checkouts live
elsewhere, replace that prefix while keeping the sibling layout. Do not switch
branches or create another checkout just to run this example. See the
[compatible revisions](#compatibility-and-verified-evidence); an older checkout
or a published Server binary may not support its native graph resources.

Requirements:

- macOS or Linux, Git, Bash, and make.
- Rust/rustup. `rust-toolchain.toml` pins **1.97.1**, including rustfmt and Clippy,
  for the host and plugins. If it is not installed:
  `rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy`.
- **Node.js 22 or 24** and npm, available in the same terminal as the build.
- Native build tools and libraries, as below.
- Internet access for initial Cargo/npm downloads. Allow at least **15 GB of
  free disk space** for the example's native build and dependencies.

No Docker, database, registry, broker, or GPU is needed.

On **macOS**, install Xcode Command Line Tools if `cc` is unavailable
(`xcode-select --install`), then install the native dependencies with Homebrew:

```sh
brew install jq protobuf pkg-config cmake
export JQ_LIB_DIR="$(brew --prefix jq)/lib"
```

On **Debian/Ubuntu Linux**, the corresponding prerequisites are:

```sh
sudo apt-get update
sudo apt-get install -y build-essential clang libclang-dev cmake pkg-config \
  jq libjq-dev libonig-dev protobuf-compiler libssl-dev
export JQ_LIB_DIR="$(pkg-config --variable=libdir libjq)"
```

Keep `JQ_LIB_DIR` set in the terminal used for setup and Rust tests. Other Linux
distributions need equivalent packages.

### 2. Prepare the Server UI and React SDK source

Start from the **Server repository root**, not the coordinator or example:

```sh
cd /Users/alljones/dev/drasi-computation-graph/drasi-server

# Confirm that the sibling source tree and supported Node version are available.
test -f ../drasi-core/Cargo.toml
node --version
npm --version

# This is the stock Server administration UI, not the example's React UI.
# Reuse an existing bundle; build it if it is missing.
if [ ! -f ui/dist/index.html ]; then
  make build-ui
fi
test -f ui/dist/index.html

# Confirm that the pinned, real @drasi/react source is in this Git object store.
git cat-file -e 2a36f857526baa08304a698131854f222b40b108:dev-tools/react/package.json
```

Coordinate with any owner of the shared Server UI before rebuilding its assets.
If the Server UI source has changed since its last build, run `make build-ui`
even if the bundle already exists. If the final Git check reports a missing
object, obtain it in the same Server checkout, then repeat the check:

```sh
git fetch origin 2a36f857526baa08304a698131854f222b40b108
git cat-file -e 2a36f857526baa08304a698131854f222b40b108:dev-tools/react/package.json
```

The SDK is not in this branch's working tree. Setup exports just that package
from the pinned commit, preserving its license, into this example's ignored
staging directory and builds it. It does not switch branches, create a checkout,
or copy GPU Lab's dependencies.

### 3. Build the example, then run it

In the same terminal:

```sh
cd /Users/alljones/dev/drasi-computation-graph/drasi-server/examples/move-a-wall
./demo setup
```

Wait for **`Ready: ./demo start`**. Setup installs and builds the example's UI,
builds both plugins and the stock Server, generates the configuration, and
validates it. If any step fails, fix the reported error and rerun setup; do not
proceed to start with an incomplete build.

The validator may summarize this as zero sources and zero queries: those counts
refer to the legacy configuration lists, while this example declares its native
components under `computation.definition`. Confirm the running graph in step 4.

```sh
./demo start
```

Leave this terminal running. Use a **second terminal** for the checks below.

Open **http://127.0.0.1:5421**. Use that exact hostname, not `localhost`, because
the source's development command endpoint checks the browser origin.

The first build compiles the stock Server and native dependencies and can take
many minutes, especially while compiling `librocksdb-sys`. A clean example build
took about 23 minutes on the validation Mac; incremental builds are much faster.
`setup` uses one Cargo job and this example's own `target/`. The native scene and
geometry plugin and the standard SSE plugin are built from the same local code
and toolchain as the Server. Symbols are not stripped. `wall-config` generates
`.build/server.yaml` from the actual native factory descriptors and built-in
continuous-query factory contracts; it does not run an embedded runtime.

`start` checks that its ports are free, launches the **actual**
`target/debug/drasi-server --config .build/server.yaml --plugins-dir
.build/plugins --skip-verification --enable-ui`, then launches the thin Vite
UI/proxy. It waits for a real `geometry-status` query result before opening the
UI service. Server logs are in `.build/server.log`.

The launcher copies the existing stock Server UI bundle into its own ignored
`.build/ui/dist` directory; it does not rebuild or modify the shared UI.
`--enable-ui` overrides the generated configuration's API-only default.
The footer's **Drasi Server Web UI** link opens **http://127.0.0.1:8421/ui/**,
served by the same stock Server instance that runs this geometry graph.

### 4. Confirm startup

The browser should show **Live query results** and two unobstructed planned
journeys. From a second terminal:

```sh
curl --fail --silent --show-error \
  http://127.0.0.1:8421/api/v1/instances/move-a-wall/queries/geometry-status/results
```

Before any edits, the successful response's `data` array contains a row with
`id: "current"`, `revision: 1`, `objects: 9`, and `obstructions: 0`.
The example UI is on **5421**; the separate stock Server administration UI is on
**8421/ui/**. Neither is the Server's usual default port 8080.

| Loopback port | Owner |
| --- | --- |
| 5421 | Vite: React assets and same-origin proxy only |
| 8421 | Stock Drasi Server REST API and Server Web UI |
| 8422 | Standard `drasi-reaction-sse`, inside Server |
| 8423 | Example native scene source's input command endpoint, inside Server |

### Stop, rebuild, or clean up

**Stop:** Ctrl-C in the launch terminal. The launcher signals only the child
processes it started and waits for them. It never discovers or kills other
listeners. All data is intentionally volatile; a full stop/start reconstructs
the fixture. Do not individually restart a native component: reconstruct the
whole graph to get a fresh source bootstrap and query caches.

**Rebuild:** stop the launcher, rerun `./demo setup`, then `./demo start`. Do this
after changing the native source, queries, or sibling Server/Core code; do not
replace a plugin library while the Server has it loaded.

**After cleanup:** `.build/`, `target/`, `artifacts/`, `ui/dist/`,
`ui/node_modules/`, and `ui/vendor/` are disposable, ignored example outputs.
Removing them does not remove the source, but you must repeat setup before
starting or running tests. The shared Server `ui/dist/` is a separate prerequisite.

This is a local development example. Plugin signature verification is disabled
only for these locally built libraries. Do not expose its command or Server
management ports to a network.

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
SSE reaction have separate cards; the catalog is explained as a shared resource,
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
    +----------> real SSE reaction + real query snapshots --+
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
| `wall-ui` | Actual result subscriptions to all five CQs | Standard SSE on `/events`; `@drasi/react` loads real snapshots and applies real deltas |

The standard SSE reaction explicitly configures `heartbeatIntervalMs: 5000`,
including it in the full reaction metadata consumed by `@drasi/react`. This is
only a connection heartbeat; it never triggers geometry computation.

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

## Inspect and validate

Complete setup first. For the Rust/UI checks, run this with the demo stopped:

```sh
cd /Users/alljones/dev/drasi-computation-graph/drasi-server/examples/move-a-wall
./demo test                  # geometry/real-Cypher tests, Rust fmt/Clippy, UI tests/build
```

For live acceptance, run `./demo start` in one terminal. In a **second terminal**:

```sh
cd /Users/alljones/dev/drasi-computation-graph/drasi-server/examples/move-a-wall
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
to ignored `artifacts/`. Example-specific data, plugin binaries, packages, logs,
and build outputs stay under this example and are ignored. Cargo/npm download
caches, the Playwright browser installation, and the stock Server UI build are
outside that directory and may be shared with other projects.

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
| `Build the stock Server Web UI first` or `/ui/` is unavailable | Run `make build-ui` from `drasi-server`, then rerun setup. Do not confuse `drasi-server/ui/dist` with this example's `ui/dist`. |
| `Pinned @drasi/react commit ... is absent` | Fetch the pinned SDK commit into the Server checkout using step 2. |
| npm cannot find `vendor/drasi-react.tgz` | Run `./demo setup` from the example root; it creates the local package before installing UI dependencies. |
| `protoc`, a C/C++ compiler, CMake, libclang, or libjq is missing | Install the platform prerequisites above. For libjq discovery/link errors, set `JQ_LIB_DIR` in this terminal before rerunning setup or tests. |
| `Run ./demo setup first`, missing plugin, or missing Server executable | Setup did not finish, or its generated files were cleaned. Rerun setup and wait for its `Ready` message. |
| `EADDRINUSE` | Stop the launcher that owns the conflicting port, or resolve the other service's ownership first. Do not kill unrelated processes. The fixed ports are listed above. |
| Commands fail origin checks | Open `http://127.0.0.1:5421`, not `localhost`, a file URL, or a different port. |
| Chromium executable is missing | Run `(cd ui && npx playwright install chromium)` after setup. |
| Native resource/ABI error or no initial `geometry-status` result | Check the compatible source revisions and `.build/server.log`; stop and rebuild the host and both plugins together with setup. Do not mix downloaded binaries with these local plugins. |

If startup fails, inspect `.build/server.log` rather than substituting a local
result model. Port conflicts fail without touching another service. If a native
component fails, inspect the computation endpoint and restart the whole example
after addressing the input/build issue. The default input/query/native state is
volatile; process restart intentionally starts fresh.

## Compatibility and verified evidence

The current example and native query-catalog integration are committed, not
an extra working-tree patch. The latest validated source revisions are:

| Repository | Commit |
| --- | --- |
| `drasi-server` | `a70064fb2decfdc8496f5385d5140f9025ff6417` |
| `drasi-core` | `5f48406bfce641d83d7f881877a4a8cac663eeac` |
| `@drasi/react` source in the Server Git object store | `2a36f857526baa08304a698131854f222b40b108` |

Server support constructs a graph-scoped `QueryResultsCatalog` and validates
query/outlet catalog agreement. It does not replace query evaluation, the Server
API, or SSE, and requires no example-specific Core modification. An older stock
binary may reject the resource recipe; build from the compatible checkouts with
`./demo setup`. This does not require an uncommitted production-code edit.

The generator declares this resource alongside `memoryIndexes`:

```yaml
# Under computation.definition:
resources:
  - id: wall-query-catalog
    role: QueryCatalog
    ownership: Graph
    binding: wall-results
resource_configurations:
  wall-query-catalog:
    kind: queryCatalog
```

Each CQ has `dependencies.catalog: [wall-query-catalog]` as well as its index
dependency. The shared stock outlet has the same catalog dependency and handled
completion; all five query output ports connect to it. Descriptors and schemas
come from the actual factory implementations, not hand-written equivalents.

Verified locally on macOS (Apple Silicon); the Linux prerequisites above are
provided for Linux source builds, not a claim of a Linux validation run:

- Ten Rust geometry/transformer tests using actual Cypher queries, including
  active filtering, scene-only records and task/destination enrichment; five UI
  record tests; formatting, Clippy, TypeScript and production UI build.
- Actual stock Server bootstrap: `geometry-status` returns revision 1,
  nine objects and zero obstructions.
- Real source commands through the native geometry transformer into query
  snapshots and standard SSE, including exact enriched `ADD`, same-key rename
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
