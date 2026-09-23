# Independent React examples

One small **cold-storage monitoring** workspace, using the private, unpublished
[`@drasi/react`](../README.md) package. No Trading frontend,
Tailwind, chart framework, Storybook or package-source alias is involved.

| Page | What it demonstrates | What it does not prove |
| --- | --- | --- |
| `/` | Real live tables, validating projections, separate raw/render identity, typed columns and visible connection/query state. | An open socket is not a synchronized query. |
| `/query-table.html` | The same actual resources through the convenient `QueryTable`, with state/retry slots rather than another hook subscription. | Its labelled projection-error control is a client-side demonstration, not a backend outage. |
| `/hooks.html` | The same real data in custom semantic HTML, using only `/react` hooks. No package controls or CSS, including in the built import graph. | Not a separately implemented client or reducer. |
| `/showcase.html` | Explicitly **simulated** loading, empty, live changes, reconnect, resynchronization, stale rows, terminal error and retry; controlled sort, themes and generic dialogs. | No server or protocol behavior is simulated or verified by this page. |

The live pages connect to instance `cold-chain`, queries `north-room` and
`south-room`, and SSE reaction `cold-chain-events`. Both queries return the
same shape; `probeId`, not a stock symbol or serialized row, is their stable
raw key. The validated render model deliberately renames it to `key` and
`temperatureC` to `celsius`. The package extracts raw identity before
projection, including sparse deletes; the view's `rowKey` is separate.

This is the **single canonical repository workspace**, now beside the package.
The package tarball ships developer guides, not this runnable source or its
server setup. The clean-consumer runner below copies this workspace separately
and installs that tarball. `examples/react` retains only a migration link.

## Run only the simulated showcase

This **server-free** path needs Node and the frontend dependencies, not Rust,
Drasi Server, Python, plugins, a database or registry access for plugin setup.
The checkout commands below use the tested **Node 24.19.0 / npm 11.17.0**
copied-local install. For **Node 22.20.0 / npm 10.9.3**, use the
[source-free tarball path](#build-and-install-boundaries), then run
`node test/preview.mjs` from its `dev-tools/react/examples` directory.
From the repository root on the copied-local setup:

```sh
npm --prefix dev-tools/react ci --ignore-scripts
npm --prefix dev-tools/react run build
cd dev-tools/react/examples
npm ci --install-links --ignore-scripts
npm run build
node test/preview.mjs
```

Open **http://127.0.0.1:15373/showcase.html**; stop the owned foreground
listener with Ctrl+C. `P7_WEB_PORT` can select another free loopback port.
The page uses deterministic, clearly labelled fixtures and makes no API/SSE
requests. Retained reconnecting/resynchronizing rows are stale, **not loading**;
initial loading has no baseline. Retry and refresh completion are simulation
controls, not a real network-recovery test.

This command only serves built frontend files. Its live-page links cannot
supply a backend: use the separate real startup below for those entries.
**`npm start` intentionally starts the real server and signed-plugin setup**;
it is not the server-free preview command. Automated browser checks exercise
the preview without substituting it for actual runtime or human AT evidence.

## Run the real example

Use Node **22.20.0** or **24.19.0**, React/React DOM **18.3.1**, the repository's
Rust toolchain, Python **3.11+** and access to GHCR/Sigstore. The current
development setup selects server **0.2.3**, registry library **0.9.2** and
host/plugin/FFI crates **0.11.2**. Only engine **0.5.9**, AST **0.3.5** and
Cypher **0.3.6** use the user-approved temporary source
`211d0f2a79aa2ad0f7cb841937f52013fe95ded6` from drasi-project/drasi-core#810.
The sibling's equal-version SDKs are not consumed.
Signed plugins from merged release `70ca432c0f12623ab9b371b2d515180ccc80c2dd`
use SDK **0.11.2** and native ABI **0.14.0**, including SSE **0.3.7**,
HTTP source **0.2.12** and scriptfile bootstrap **0.2.14**.
See [the repository's approved runtime and immutable pin provenance](https://github.com/drasi-project/drasi-server/blob/main/docs/main-runtime-integration.md).
Earlier ABI 0.11/0.13 evidence is historical, not a startup fallback.
The temporary engine pin is not a released fix or a stored-data repair:
registry library 0.9.2 does not consume drasi-project/drasi-core#909's codec
change or the new outbox trim methods. No core merge/publication is implied.
Linux builds
also need libjq/Oniguruma development libraries as documented there.
No database, Trading API or Docker container is needed by this example.

The local checkout commands below use **Node 24.19.0 / npm 11.17.0**.
For **Node 22.20.0 / npm 10.9.3**, build the backend/package explicitly, then
use the [tarball consumer](#build-and-install-boundaries) and its `npm start`
command. That npm 10.9.3 directory-copy path invokes package `prepare` despite
`--ignore-scripts`; it is not accepted as lifecycle-free installation.
From the repository root:

```sh
# Prepare only an absent sibling; reject a wrong or dirty foreign checkout.
bash scripts/prepare-build.sh
python3 scripts/plugin_origin.py mode

# Build this checkout's real server and embedded UI, not an unrelated binary.
npm --prefix ui ci --ignore-scripts
npm --prefix ui run build
cargo build --locked

# Build the package explicitly. Consumer installation must not rebuild it.
npm --prefix dev-tools/react ci --ignore-scripts
npm --prefix dev-tools/react run build

cd dev-tools/react/examples
npm ci --install-links --ignore-scripts
npm run build
npm start
```

Open **http://127.0.0.1:5373/** after the command prints `Example ready`.
The other pages are linked in the navigation. Leave the command in the
foreground; **Ctrl+C** stops only its own server and web listener.
Each start uses a fresh `.runtime/live-...` directory for plugins, logs and
WAL data. The source's two seed nodes are loaded from `config/readings.jsonl`.
Resources are created by the explicit **server configuration**, before the
web listener is exposed. The browser only reads them and never provisions or
starts resources.

`scripts/runtime.mjs` runs the existing immutable, signature-verifying plugin
installer and validates actual loaded hashes/versions/ABI. It reuses the
reviewed five-plugin backend pin set; only HTTP source, scriptfile bootstrap
and SSE reaction are used here. The unused PostgreSQL plugins do not start a
database. This reuse of backend setup policy is not a Trading frontend import.
No local-SDK rebuild, unsigned download or verification bypass is used.

The frontend uses a loopback, **GET-only** same-origin proxy to the explicit
REST and SSE endpoints. It forwards real response/stream bytes, not fixture
data. It is a development helper, **not a production authenticated gateway**.
For your own deployment, replace `connectionOptions()` in `src/readings.ts`
with your operator-supplied URLs/instance/references and follow the package
[authentication and transport reference](../docs/connection.md#authentication-and-injected-transports).

Backend ports are allocated independently on loopback. An explicitly occupied
port fails; no existing service is reused or killed. `P7_WEB_PORT` overrides
5373 (`0` chooses a free port); `P7_REST_PORT`, `P7_FEED_PORT` and `P7_SSE_PORT`
can pin the otherwise ephemeral backend ports. Use the URLs printed by the
current run. The startup script renders the three quoted port markers in
the declarative template to **concrete numbers** in the owned `server.yaml`.
Plugin full-view metadata preserves unresolved environment expressions, so
leaving such an expression in the SSE port would correctly fail the client's
strict read contract even if the plugin itself resolved its bind port.
Before installation, startup rejects a stale server/SDK binary version against
the checked-out manifest/lock and resolved SDK. It reuses the shared backend
source verifier to record the exact revision, manifest/lock hashes, selected
engine/SDK identities and approved plugin-lock hash; actual loaded binary
hashes/native ABI/signatures remain independently checked. The startup also validates the actual pre-created references with the public
REST-only client before exposing the browser listener; it does not rewrite
responses or loosen that contract. The bootstrap file is copied into the owned working directory
and uses a literal relative JSONL path; it does not rely on environment
expansion inside plugin-owned arrays.

### Try the convenient QueryTable

Open `/query-table.html` after the real startup. `src/query-table.tsx` passes
the same explicit references, raw `probeKey`, validating `readingOptions`,
typed columns and separate rendered `rowKey` into public `QueryTable`.
Each table owns one query subscription; loading/empty/stale/error slots
receive the query state and local/shared retry callbacks. No extra query hook
is mounted just to read status.

The **Reject North projection (client-only demo)** checkbox selects a
deliberately failing projection without changing server data or the stream.
North shows its scoped error and last-good data; South remains live.
**Retry query** rereads only North and cannot fix that callback: uncheck to
restore the validating projection. This control is not a network simulation.
The live tests separately exercise real source updates/deletes, empty views,
stream outages and operator-owned missing-resource repair.

### Update, delete and recover

Startup prints a complete command such as `npm run feed -- <feed-url> warm`.
In another terminal under `dev-tools/react/examples`, use the **actual printed URL**:

```sh
# Set FEED to the loopback Feed URL printed by this run.
npm run feed -- "$FEED" warm
npm run feed -- "$FEED" delete
npm run feed -- "$FEED" restore
```

`warm` changes North's `probe-1001` from 3 to 8 C. `delete` removes South's
`probe-2001`; North must stay intact. `restore` inserts the two original
example nodes again. These are deliberate example-owned source-data writes,
not client/provider resource creation. The helper allowlists exactly those
node identities and a loopback source endpoint.

To observe recovery interactively, switch the browser's developer tools to
offline, send updates/deletes from the other terminal, then restore browser
network access. Retained rows are explicitly labelled last-good during
recovery. Browser tools may differ in whether they close an already-open
stream; the automated real-server gate closes only the owned transparent SSE
proxy and proves recovery without navigation. It never manufactures deltas.

Do not infer atomicity: REST and SSE have **no shared authoritative cursor**.
Known snapshot overlap triggers bounded, visible refresh; a no-known-overlap
baseline is best effort. An undetectably delayed older event can temporarily
replace newer state until refresh. There is no timestamp-based ordering,
exactly-once claim or replay rollback workaround.

### Resource errors and cleanup

If a configured query/reaction is removed or stopped, the page shows the
structured error code, the owner/setup guidance and **Retry connection**.
Start or recreate it through the owning configuration/operator, then retry.
A query-local data/refresh error offers **Retry query** instead; it does not
restart healthy shared subscriptions. Auth, network or malformed-payload
errors are never permission to create resources.

Stopping retains diagnostics and the owned data directory. To remove one
completed run, pass its **exact printed path**, after retaining any evidence:

```sh
npm run clean:run -- "$RUN_DIRECTORY"
```

The cleanup helper requires a direct `live-...` child of this example's
`.runtime` directory and a successful, example-owned `cleanup.json`. It
refuses unfinished/unowned paths. It never prunes Docker, searches/kills by
process name, or removes another run. There are no cleanup wildcard commands.
These seed data are disposable; this is not a production durability demo.

## Build and install boundaries

The example's `file:` dependency still resolves **public built exports** on
the tested npm 11.17.0 copied-local path.
Its `.npmrc` enables npm's `install-links`: the local dependency is packed
into a regular installed directory rather than linking to the package's
development dependencies. Always use `--ignore-scripts`; rebuild the package
explicitly first and repeat the example's `npm ci` to copy updated artifacts.
There is no `predev`, `prebuild`, package-source Tailwind scan or source alias.

For a clean tarball proof from the repository root, choose a **new,
nonexistent destination directory**:

```sh
(cd dev-tools/react && npm pack --ignore-scripts --pack-destination /tmp)
node examples/trading/app/test/tools/packed-consumer.mjs \
  /tmp/drasi-react-0.1.0.tgz /tmp/drasi-packed-consumer
```

The shared test runner copies these examples and Trading into that new
consumer, substitutes only the tarball in their locks, verifies every other
locked dependency version/integrity, installs with scripts disabled and
rejects a package symlink. Package source is absent. It runs the installed
public declaration/import/SSR/README contracts and these examples'
type-check, tests and production build.
It copies the installed package guides and notices beside the example so
their local documentation links work there too; no parent package source,
manifest or build output supplies module resolution.

To run the **new examples** from that source-free consumer:

```sh
# Set P7_SOURCE_ROOT to the original backend checkout built above, not a frontend alias.
cd /tmp/drasi-packed-consumer/dev-tools/react/examples
P7_SOURCE_ROOT="$BACKEND_CHECKOUT" npm start
```

`P7_SOURCE_ROOT` supplies only the actual backend executable, Cargo/core
provenance and shared plugin installer/pins. The reused source verifier lives
under the backend's `examples/trading/app/test/live/` tooling; it imports no
Trading frontend or business code. The browser assets, frontend
source, declarations and runtime dependencies come from the copied example
and installed package. No package or Trading source can satisfy an import.
The package remains private and unpublished; do not use a registry
`npm install @drasi/react` command.

Each build writes `dist/build-graph.json` from Rollup's actual rendered module
graph and `dist/build-evidence.json` from emitted file bytes/hashes. The gate
walks static **and lazy** imports from `hooks.html`, including shared chunks,
and inspects the installed package's shipped source-map labels. It rejects
retained component, Radix, geometry, tutorial/Trading or CSS dependencies.
It reads no original package source. The HTML must also have no stylesheet.
Virtual CommonJS IDs are normalized to their actual consumer-relative files;
an enclosing directory named `trading-consumer` is not a Trading import.
Inputs outside the example's source, installed dependencies and named Vite/
CommonJS helpers fail the gate, including real sibling Trading/source aliases.
React DOM is the example's explicitly installed renderer, not a hook
dependency or bundled hidden peer.

Size evidence recursively includes every emitted JS/MJS/CJS and CSS asset at
the output root or any nested directory, including entry/shared/lazy chunks, totals
each file once for the workspace, and separately totals every entry's
reachable files. Per-entry totals intentionally overlap; do not sum them to
claim a combined download size. The lazy modal chunk is included even when
not opened. The existing package/Trading measurements and future-growth
policy remain separate.
`test/baseline-metrics.json` applies the same **2% future-growth ceiling** to
each example entry and the complete workspace; the hooks entry's zero CSS
budget is literal, not an omitted metric.

## Verification and accessibility

```sh
# In dev-tools/react/examples, after the explicit build/install steps:
npm run typecheck
npm test
npm run build
npm run test:browser
npm run test:live
```

The browser commands require the pinned Playwright **1.56.1** browser
binaries. If missing, install them with the existing runner:

```sh
npx --no-install playwright install chromium firefox webkit
```

`test:browser` runs **only the labelled simulation** in Chromium, Firefox and
WebKit: every state, stale/error/retry, controlled sort, row identity,
reduced motion, narrow layout, local/default themes, body-portal theme
inheritance, focus containment/return and Escape. It asserts zero API/SSE
requests from the showcase. This is presentation evidence, not real recovery.

`test:live` uses the **same `startExample()` path as `npm start`**, the real
checked-out server and the declarative resources. It checks both live pages
in all three engines, real updates/deletes/recovery, same-shaped query
isolation, existing-resource reload with no browser writes and meaningful
resource errors. It records actual REST/request/stream evidence and owns
bounded cleanup. The browser assertion bound is 5 seconds, with zero retries;
missing tools, failed signatures, startup failures and missing resources
fail rather than becoming successful skips.

Both suites retain full-rule axe results, including violations and incomplete
checks, in their Playwright reports. Whole-document scans and explicitly
scoped active-dialog scans are labelled separately. All generic/default and
example-theme audits require **zero violations**. Real-browser keyboard
behavior, DOM assertions, rule scans and actual human screen-reader review
are different evidence categories. None proves universal WCAG compliance.

**Human AT acceptance remains pending.** P7 development was expressly
authorized while the predecessor's nine-part
[repository screen-reader checklist](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md#manual-screen-reader-checklist-pending)
remains open. Use that checklist on the examples' table/sort/actions,
connection and query notices, retry, dialog/focus/scroll, narrow layout,
themes and reduced motion; mark Trading-specific tabs/fullscreen items not
applicable to these pages, not passed. Record date/revision, exact
OS/browser/AT versions, spoken output, focus behavior and each actual outcome.
No automated result is human approval.

Trading's separate strict non-regression policy, its preserved pre-existing
contrast findings and all five original zero-diff images are unchanged.
The authoritative integrated, source-free Linux gate remains
[`npm run test:browser:linux`](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md#reproducible-visual-and-packed-consumer-gate)
under `examples/trading/app`; it now also runs this showcase and retains its
independent graph/size/browser evidence. The actual Trading backend gate is
still mandatory and separate.

## Adapt or contribute

- `src/readings.ts`: raw key and validating projection, explicit references.
- `src/table.tsx`: one subscription per table, typed `DataTable` and pure
  `queryTableState`.
- `src/query-table.tsx`: minimal live `QueryTable`, state/error slots and
  explicitly client-only projection-failure/retry composition.
- `src/hooks.tsx`: custom HTML with the headless entrypoint only.
- `src/showcaseState.ts` / `src/showcase.tsx`: deterministic presentation
  fixtures, not alternate transport/reconciliation logic.
- `config/` / `scripts/`: all setup, seed writes and service ownership.
- `test/`: behavior, dependency-graph guards, real browser and live contracts.

Keep new consumer recipes small and test them against installed exports.
Do not copy query provisioning, financial transforms, tutorial/fullscreen
wrappers or implicit instance selection from Trading. The complete [package guides](../README.md#choose-the-next-step) cover public
props/options/defaults, sorts/slots/actions, auth/transports, compatibility,
CSS tokens/portals, migrations and troubleshooting, with one
[API reference](../docs/reference.md). Do not fork a second API reference here.
These examples share the repository's Apache-2.0 license;
package attribution is retained in its LICENSE and NOTICE.
