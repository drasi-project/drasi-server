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

## Run

Use the existing sibling `drasi-server` and `drasi-core` checkouts. Do not use a
published Server binary with plugins built against unrelated sources.

Prerequisites:

- macOS or Linux; Rust/rustup (the example pins 1.97.1 for both host and plugins).
- Node.js 22 or 24 and npm.
- A built stock Server Web UI in `drasi-server/ui/dist`. Run `make build-ui`
  from `drasi-server` if it is missing; coordinate with the UI owner before
  rebuilding shared assets in this workspace.
- Stock Server support for the `queryCatalog` computation resource recipe.
  Use the current workspace's Server integration changes, not an older binary;
  see [compatibility](#compatibility-and-verified-evidence).
- The normal Server/Core native build prerequisites: C/C++ compiler, CMake,
  protobuf compiler and system libjq if required by your platform. See the
  repositories' build instructions.
- The `drasi-server` Git object
  `2a36f857526baa08304a698131854f222b40b108`, which contains the real
  `@drasi/react` package. It is not in this branch's current working tree.
  `setup` exports just that package into this example's ignored staging
  directory, preserving its license, and builds it. It does not switch branches,
  create a checkout, or copy GPU Lab's generated dependencies.
- Internet access for initial Cargo/npm dependency downloads. No Docker,
  database, registry, broker, or GPU is needed.

```sh
cd /Users/alljones/dev/drasi-computation-graph/drasi-server/examples/move-a-wall
./demo setup
./demo start
```

Open **http://127.0.0.1:5421**. Use that exact hostname, not `localhost`, because
the source's development command endpoint checks the browser origin.

The first build compiles the stock Server and can take several minutes.
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

| Loopback port | Owner |
| --- | --- |
| 5421 | Vite: React assets and same-origin proxy only |
| 8421 | Stock Drasi Server REST API and Server Web UI |
| 8422 | Standard `drasi-reaction-sse`, inside Server |
| 8423 | Example native scene source's input command endpoint, inside Server |

**Stop:** Ctrl-C in the launch terminal. The launcher signals only the child
processes it started and waits for them. It never discovers or kills other
listeners. All data is intentionally volatile; a full stop/start reconstructs
the fixture. Do not individually restart a native component: reconstruct the
whole graph to get a fresh source bootstrap and query caches.

This is a local development example. Plugin signature verification is disabled
only for these locally built libraries. Do not expose its command or Server
management ports to a network.

## A 90-second presentation

1. **Start clear.** Point to Ada's path to Packing and Grace's bent path to
   Assembly. The shaded path is each cart's circular footprint plus clearance,
   not just a zero-width line. These are static plans, not moving carts.
2. **Move one input.** Drag an obstacle across a path; try **Movable wall**
   upward across Ada's path. A dashed
   ghost is explicitly tentative. On release the command goes to the source.
   The path/cart turn orange and the impact panel names **Ada / Deliver
   packaging / Packing station / Movable wall** from the real impact query.
3. **Explain the computation.** In **Follow the change**, directly below the
   floorplan, click the flow's
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
obstacle/destination and use arrow keys to move it 0.25 m. Every shape has a
keyboard-accessible coordinate editor; carts have radius/clearance controls.
The **Edit scene** panel keeps the selected object's properties beside the floor:
click an object or path on the floor, or use **Selected object** (including inactive
objects and carts without a journey). Use **Apply input change** to save or
**Remove object** to delete it. The **Add** buttons create a draft cart, journey,
convex polygon obstacle or destination; **Add object** submits it and **Cancel**
discards it. Names and active state are editable. The UI presents one floor;
new objects always use the internal `ground` floor ID. There is no floor picker
or floor-creation control. Same-floor checks remain in the native geometry.
Stable IDs do not change on update. Unsupported polygons or out-of-range values
produce visible errors.

## What actually runs

```text
React input command (no geometry)
    |
    v
native scene source / authoritative in-memory store
    | graph ChangeEnvelope: FloorObject inserts/updates/sparse deletes
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

- Coordinates, cart radii, clearances and distances are **metres**.
- The floor is a 24 by 16 m rectangle. Inputs must be finite and within it.
- Carts are circular, with radius 0.05-2 m and clearance 0-2 m. A journey sweeps
  that circle along every segment of its polyline (2-32 points).
- Obstacles are **simple, strictly convex polygons**, 3-16 vertices, without a
  repeated closing vertex. Rectangles and convex angled shapes are supported.
  Concave, self-crossing, duplicate/collinear or degenerate rings are rejected,
  not treated as clear. No holes, curves or unsupported shape variants.
- `geo` computes the Euclidean distance between the whole polyline and the
  closed polygon. A crossing or segment inside the polygon has distance zero.
  An obstruction exists when the distance is at most radius + clearance +
  **1e-7 m**. Tangency/contact therefore counts as obstruction. Polygon validity
  also rejects edge lengths at/below 1e-7 m and cross products at/below 1e-7 m².
- Only active carts, journeys and obstacles on the **same floor** interact.
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

```sh
./demo test                  # geometry/real-Cypher tests, Rust fmt/Clippy, UI tests/build
./demo start                 # leave running in another terminal
./demo check                 # real stock-server/API/SSE acceptance; restores fixture
cd ui
npx playwright install chromium   # once, if the browser is not installed
cd ..
./demo browser               # actual drag, reload, offline/reconnect, errors, reset
```

The real query tests cover crossing, finite footprint/clearance, near misses,
contact, polyline turns, overlapping causes, invalid shapes and separate floors;
obstacle-only/path-only changes, stable insert/update/delete diffs, no-op commands,
cart/path/obstacle deletion, empty/bootstrap/reset, stale commands/envelopes,
upstream active filtering and downstream destination/task enrichment.
These tests use the real Cypher evaluator, not mocked query output. They are
test-only, not a substitute for the stock-server live check.

`check` is an acceptance script for native topology, exact enriched rows, active
filtering, multiple causes, retractions, reset and real SSE delivery. It requires
actual stock SSE `ADD`, stable-key `UPDATE` and `DELETE` rows and saves them in
ignored `artifacts/sse-changes.json`. `browser` writes screenshots
to ignored `artifacts/`. All generated data, plugin binaries, packages, logs,
and build outputs stay under this example and are ignored.

Useful actual Server endpoints:

```sh
curl -s http://127.0.0.1:8421/api/v1/instances/move-a-wall/computation
curl -s http://127.0.0.1:8421/api/v1/instances/move-a-wall/queries/affected-journeys/results
curl -N http://127.0.0.1:8422/events
```

If startup fails, inspect `.build/server.log` rather than substituting a local
result model. Port conflicts fail without touching another service. If a native
component fails, inspect the computation endpoint and restart the whole example
after addressing the input/build issue. The default input/query/native state is
volatile; process restart intentionally starts fresh.

## Compatibility and verified evidence

Validated with these checkout base revisions and the Server's separately
authorized, working-tree `queryCatalog` integration change:

| Repository | Commit |
| --- | --- |
| `drasi-server` | `9c5689af071aacad0fb8f797793430a90a96dc20` |
| `drasi-core` | `5f48406bfce641d83d7f881877a4a8cac663eeac` |

The original Server base revision did not expose a native query catalog recipe.
The supported Server change constructs a graph-scoped `QueryResultsCatalog` and
validates query/outlet catalog agreement. It does not replace query evaluation,
the Server API, or SSE, and requires no Core modification. An older stock binary
will reject the recipe; rebuild with `./demo setup`.

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

Verified locally:

- Eight Rust geometry/transformer tests using actual Cypher queries, including
  active filtering and task/destination enrichment; four UI record tests;
  formatting, Clippy, TypeScript and production UI build.
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
remain visible, commands are disabled, and no scene or "clear floor" result is
fabricated. It does not validate geometry/SSE integration. Stop Vite with Ctrl-C.
