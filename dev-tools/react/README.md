# @drasi/react

Reusable [React](https://react.dev/) building blocks for UIs whose content is
kept continuously up to date by [Drasi](https://drasi.io) Continuous Queries.

[Drasi](https://drasi.io) is an open-source Data Change Processing platform that
simplifies building change-driven solutions. You declare a **Continuous Query** —
an [openCypher](https://opencypher.org/) or GQL query describing exactly the data
you care about — and Drasi incrementally maintains that query's result set as the
underlying sources change, without polling or re-scanning them. Each change to the
result set is delivered to subscribed **Reactions** as a precise set of added,
updated, and deleted rows. A single query can span multiple sources, and the
meaning of "a change" is defined by your query rather than by the source systems.

`@drasi/react` is the client side of that picture: it binds a React app to one or
more Continuous Queries through Drasi Server's **SSE Reaction** and surfaces each
query's live result set as idiomatic React state:

- **One shared connection.** A single [SSE](https://developer.mozilla.org/docs/Web/API/Server-sent_events)
  connection is opened to a Drasi SSE Reaction and **multiplexed** across every
  Continuous Query in your app — not one connection per query.
- **Hooks for live results.** `useDrasiQuery` folds a query's added/updated/
  deleted result changes into a ready‑to‑render array.
- **A batteries‑included table.** `QueryTable` renders a sortable, animated,
  full‑screen‑capable table bound to a query, with a built‑in “view the code”
  dialog.

The library is **completely application agnostic**: it knows nothing about your
queries, your data shapes, or how rows should be keyed/sorted. You provide all of
that through props and options, which makes it reusable across any Drasi project.

> This is currently an **unpublished, private staging package** at
> `dev-tools/react` in the
> [drasi-server](https://github.com/drasi-project/drasi-server) repository. The
> Trading example installs and builds it through a local `file:` dependency so
> the same exports used by a future npm consumer are exercised now.

## Table of contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [Concepts](#concepts)
- [Errors, retries and provisioning ownership](#errors-retries-and-provisioning-ownership)
- [API reference](#api-reference)
  - [`DrasiProvider`](#drasiprovider)
  - [`useDrasiQuery`](#usedrasiquery)
  - [`useDrasiConnectionStatus`](#usedrasiconnectionstatus)
  - [`useDrasiServerUiUrl`](#usedrasiserveruiurl)
  - [`useDrasiQueryDefinition`](#usedrasiquerydefinition)
  - [`QueryTable`](#querytable)
  - [Low‑level classes](#low-level-classes)
- [Styling](#styling)
- [Project structure](#project-structure)
- [Building from source](#building-from-source)
- [Future publishing](#future-publishing)
- [Using it in the Trading example](#using-it-in-the-trading-example)
- [License](#license)

## Installation

The package is not published to npm yet. Inside this repository, install its
development dependencies and build it first:

```bash
cd dev-tools/react
npm ci
npm run build
```

Repository consumers declare it as a local dependency:

```json
{
  "dependencies": {
    "@drasi/react": "file:../../../dev-tools/react"
  }
}
```

React and `react-dom` are peer dependencies, ensuring the library uses the
application's copies. React 18.3.1 / Node 22 are the tested Trading baseline;
the broad peer range is not a claim of React 19 validation. No publication or
repository move is part of this work.

Import the complete component stylesheet once:

```ts
import '@drasi/react/styles.css';
```

## Quick start

Provision resources separately, then connect to them by reference. This example
requires the `analytics` instance, a **running** `readings` query, and a
**running** `sse` reaction named `events` whose `queries` includes `readings`.
The explicit SSE URL must be reachable from the browser (including CORS and
any reverse-proxy routing). Server bind addresses/ports are not client options.

```tsx
import {
  DrasiProvider,
  QueryTable,
  useDrasiConnectionStatus,
  type ReactionReference,
} from '@drasi/react';
import '@drasi/react/styles.css';

const QUERY_IDS = ['readings'];
const REACTION: ReactionReference = {
  id: 'events',
  endpoint: 'https://events.example/changes',
};

interface Reading { id: string; value: number }

function Readings() {
  return (
    <QueryTable<Reading>
      queryId="readings"
      rowKey={(row) => row.id}
      columns={[
        { key: 'id', label: 'Sensor' },
        { key: 'value', label: 'Value', align: 'right' },
      ]}
      animateOnChange="value"
    />
  );
}

function ConnectionBadge() {
  const status = useDrasiConnectionStatus();
  return <span>{status.error?.message ?? (status.connected ? 'Live' : 'Connecting…')}</span>;
}

export default function App() {
  return (
    <DrasiProvider
      serverUrl="https://drasi.example"
      instanceId="analytics"
      queryIds={QUERY_IDS}
      reaction={REACTION}
    >
      <ConnectionBadge />
      <Readings />
    </DrasiProvider>
  );
}
```

When the provider mounts it:

1. reads each query and the reaction in the **explicitly selected instance**,
2. validates lifecycle status, SSE kind and subscribed-query membership, and
3. opens **one** SSE connection and fans updates out to each `useDrasiQuery`.

Initialization, subscription, reconnect and explicit retry perform **GETs only**.
There is no resource-management mode, implicit first-instance selection, query
language selection, deployment reconciliation, or Trading-specific default.
All REST reads use `/api/v1/instances/{encodedInstanceId}/...`.

Each query hook subscribes to live changes before requesting its REST snapshot.
Changes received while that request is in flight are buffered and replayed after
the snapshot. Reconnect fetches a fresh snapshot. This preserves the existing
buffer/replay behavior; it does **not** establish an atomic, ordered or
exactly-once snapshot/live handoff without server cursors. Result identity and
broader transport contracts are separate follow-up work.

## Concepts

**Multiplexed connection.** A Drasi SSE Reaction can deliver the result changes
for many queries over a single connection. `@drasi/react` opens that connection once
([`DrasiSSEClient`](#low-level-classes)) and routes each batch to the right
subscribers by query id, so adding more `QueryTable`s does not add more sockets.

**Content routing for aggregations.** Some Drasi result changes (for example from
aggregating queries) arrive without a query id. The library cannot know which
query they belong to, so you may supply a `routeUnidentified` callback that
inspects a row and decides which query id(s) it belongs to. This keeps all
application‑specific shape knowledge in your app.

**Generic accumulation.** `useDrasiQuery` folds each change batch into a `Map`
keyed by `getKey(row)`; rows flagged `_deleted` are removed. Optional `transform`
(normalize a row) and `postProcess` (sort/filter/slice the final array) options
let you adapt any data model without the library hard‑coding it.

## API reference

### `DrasiProvider`

Establishes the shared connection and makes it available to descendants. Render
it once near the root of your tree.

| Prop | Type | Description |
| --- | --- | --- |
| `serverUrl` | `string` | Absolute HTTP(S) server base URL. **Required; no default.** |
| `instanceId` | `string` | Explicit instance identity. **Required; never discovered.** |
| `queryIds` | `readonly string[]` | Identifiers of existing queries. **Required.** |
| `reaction` | `ReactionReference` | Existing SSE reaction identity and explicit browser URL. **Required.** |
| `routeUnidentified` | `(rows, deliver) => void` | Optional router for result-change payloads that arrive without a query id. Call `deliver(queryId, rows)` for each matching query. |
| `fetch` | `typeof fetch` | Optional fetch implementation for authentication, polyfills, or tests. |
| `eventSourceFactory` | `(url) => EventSourceLike` | Optional EventSource implementation for polyfills or tests. |
| `reconnect` | `object` | Optional maximum-attempt and backoff overrides. |
| `requestTimeoutMs` | `number` | REST request/body timeout, default 10000 ms. |
| `children` | `ReactNode` | Your application. |

`ReactionReference`:

```ts
interface ReactionReference {
  id: string;
  endpoint: string;              // absolute browser-reachable HTTP(S) URL
}
```

The endpoint is never derived from server bind settings. A reverse proxy may
expose a different host, port and path. URL syntax is validated, but operators
must route it to this reaction; the current SSE protocol cannot attest endpoint
identity. URL credentials, fragments, wildcard bind hosts and relative URLs
are rejected. Supply authenticated fetch/EventSource implementations as needed;
native EventSource does not support custom headers. These transports must honor
cancellation. Keep configuration object/array identities stable across renders;
full material/equivalent configuration lifecycle work is deferred.

### `useDrasiQuery`

```ts
function useDrasiQuery<T = any>(
  queryId: string,
  options?: UseDrasiQueryOptions<T>,
): { data: T[] | null; loading: boolean; error: DrasiError | null; lastUpdate: Date | null };
```

Subscribes to a query over the shared connection and returns its accumulated
result set.

`UseDrasiQueryOptions<T>`:

| Option | Type | Description |
| --- | --- | --- |
| `getKey` | `(row) => string \| null` | Stable unique key used to accumulate adds/updates/deletes. Returning `null` skips the row. Defaults to `row.id ?? row.symbol ?? JSON.stringify(row)`. |
| `transform` | `(row) => T` | Normalize a raw row before it is stored (e.g. parse numeric strings). |
| `postProcess` | `(rows: T[]) => T[]` | Sort/filter/slice the accumulated rows before render. |

```tsx
const { data, loading } = useDrasiQuery('portfolio-query', {
  getKey: (row) => `portfolio-${row.id}`,
  transform: (row) => ({ ...row, quantity: Number(row.quantity) }),
  postProcess: (rows) => rows.sort((a, b) => b.currentValue - a.currentValue),
});
```

### `useDrasiConnectionStatus`

```ts
function useDrasiConnectionStatus(): ConnectionStatus;
// { connected: boolean; reconnecting?: boolean; error?: DrasiError; lastConnected?: Date }
```

### `useDrasiServerUiUrl`

```ts
function useDrasiServerUiUrl(): string | null;
```

Returns a deep link to the Drasi Server UI for the connected instance, or `null`
before the connection is established.

### `useDrasiQueryDefinition`

```ts
function useDrasiQueryDefinition(queryId: string):
  { config: QueryConfig | null; loading: boolean; error: DrasiError | null };
```

Fetches a query's full configuration from the server (used, for example, to show
the live Cypher definition in the `QueryTable` code viewer).

### `QueryTable`

A sortable, animated table bound to a single query. It calls `useDrasiQuery`
internally, so you only describe how to render rows.

Key props (see `QueryTableProps<T>` for the full list):

| Prop | Type | Description |
| --- | --- | --- |
| `queryId` | `string` | Query to subscribe to. **Required.** |
| `columns` | `ColumnDef<T>[]` | Column definitions. **Required.** |
| `rowKey` | `(row: T) => string` | Unique React key per row. **Required.** |
| `queryOptions` | `UseDrasiQueryOptions<T>` | Forwarded to `useDrasiQuery` (key/transform/sort). |
| `title` | `string` | Card title. |
| `defaultSort` | `{ column: string; direction: 'asc' \| 'desc' }` | Initial sort. |
| `animateOnChange` | `keyof T` | Field whose changes trigger the row flash animation. |
| `actions` | `RowAction<T>[]` | Per‑row action buttons. |
| `actionsWidth` | `string` | CSS width for the actions column, such as `3rem`. |
| `headerActions` | `ReactNode` | Header slot (e.g. an “add” button). |
| `emptyMessage` | `string` | Shown when there are no rows. |
| `codeSnippet` | `string` | Consumer code shown in the “view code” dialog. |
| `className` / `tableClassName` / `headerClassName` / `rowClassName` | `string` | Style hooks for full theming. |

`ColumnDef<T>`:

```ts
interface ColumnDef<T> {
  key: keyof T | string;
  label: string;
  format?: (value: any, row: T) => React.ReactNode;
  sortable?: boolean;            // default true
  align?: 'left' | 'center' | 'right';
  className?: string | ((value: any, row: T) => string);
  headerClassName?: string;
  width?: string;                  // CSS width, e.g. '5rem' or '120px'
}
```

### Low‑level classes

If you are not using React, or you want full control, the underlying classes are
exported too:

- **`DrasiClient`** — connect-only; exposes `initialize()`,
  `validateResources(signal?)`, `subscribe(queryId, cb, onError?)`,
  `getQueryResults(queryId, signal?)`, `getQueryConfig(queryId, signal?)`,
  `getQuery(queryId, signal?)`, `getReaction(signal?)`,
  `onConnectionStatusChange(cb)`, `getServerUiUrl()` and `disconnect()`.
- **`DrasiSSEClient`** — the raw multiplexing SSE client used by `DrasiClient`.
  Direct use does not perform REST validation unless its read-only `validate`
  callback is supplied; it cannot infer missing resources from stream errors.

```ts
const client = new DrasiClient({
  serverUrl: 'https://drasi.example',
  instanceId: 'analytics',
  queryIds: ['readings'],
  reaction: { id: 'events', endpoint: 'https://events.example/changes' },
});
await client.initialize();
const unsubscribe = client.subscribe('readings', (result) => {
  console.log(result.data);
}, (error) => console.error(error.code, error.message));
// On teardown:
unsubscribe();
await client.disconnect();
```

Full-view reads consume `{ success: true, data: { id, status, config }, error }`.
Query text, `queryLanguage`, source subscriptions (`sourceId`, `pipeline`,
`nodes`, `relations`) and joins are inside `config`; reaction `kind`, `queries`
and plugin properties are **flattened within `config`**, not `config.properties`.
`getQueryConfig` returns that config; absence throws a typed error, not `null`.
Read-only validation does not compare query text or deployment definitions.

## Errors, retries and provisioning ownership

`DrasiError` extends `Error`. Its stable `code`, `instanceId`, `resourceKind`,
`resourceId`, `resourceStatus`, optional HTTP `status` and `retryable` fields
survive through the client, context and hooks. Messages are safe summaries;
raw server responses, stack traces and network details are not displayed.
Render `error.message`; branch on `error instanceof DrasiError` and `error.code`.
Never parse a message to authorize setup.

| Code | Meaning / ownership |
| --- | --- |
| `INSTANCE_NOT_FOUND` | Wrong/missing explicitly selected instance. Operator/app configuration, not query setup. |
| `QUERY_NOT_FOUND`, `REACTION_NOT_FOUND` | Structured REST-confirmed absence. App may provision only resources it owns. |
| `RESOURCE_STOPPED` | Stopped/Added resource. An owning app may start it; the package never does. |
| `RESOURCE_STARTING` | Starting/Reconfiguring, including bootstrap. Retryable; **not absent** and no duplicate start. |
| `RESOURCE_UNAVAILABLE` | Error/Stopping/Removed. Surface for operator attention. |
| `SERVER_UNAVAILABLE`, `STREAM_UNAVAILABLE` | Retryable transport/service failure. **Never permission to create resources.** |
| `UNAUTHENTICATED`, `FORBIDDEN` | Fix credentials/authorization; no automatic retry/provisioning. |
| `INVALID_CONFIGURATION`, `INCOMPATIBLE_RESOURCE`, `INVALID_PAYLOAD` | Fix references, reaction contract, deployment conflict or unsupported data; no blind retry. |

Native EventSource hides HTTP status. After an opaque stream failure the
client validates references using **read-only REST**, then either surfaces a
confirmed permanent error or retries transport. A proxy's unstructured 404
is an invalid payload, not evidence of a missing query.

The default policy allows **10 retries after the first attempt**, with
1000 ms exponential backoff capped at 30000 ms. Each EventSource opening has a
10000 ms timeout; each REST request/body also has a 10000 ms timeout. The
`reconnect` options are `maxReconnectAttempts`, `initialReconnectDelayMs`,
`maxReconnectDelayMs`, and `connectionTimeoutMs`. Counts reset on a successful
open/snapshot. This bounds consecutive failed attempts, not idle time on an
already-open stream. Snapshot transient retries use the same count/backoff;
missing/stopped/auth/protocol errors terminate immediately. Exhaustion surfaces
the last typed error and stops work. `retryable` says another attempt may be
useful, not that the package will retry forever.

`useDrasiClient()` returns `{ client, initialized, error, retry }`; `retry()`
starts a new **read-only** connection/snapshot lifecycle. Unmount/cleanup aborts
requests, closes the stream and cancels timers. A direct client owner must call
`disconnect()`; every subscription returns an unsubscribe function.

Applications needing setup should first attempt `client.initialize()`, catch
only eligible typed errors, run their own bounded provisioner, then retry.
Keep desired-definition checks, POST bodies and conflict policy outside the
package. Do not use SSE errors as absence detection.

For this application-owned lifecycle, `DrasiClientProvider` is a controlled
React context binding: pass `value={{ client, initialized, error, retry }}`.
It opens **no** connection and performs **no** initialization, retry or cleanup;
the owner must manage cancellation and disconnect on teardown. It lets an app
reuse the *same* initialized client/stream with all package hooks, without a
second preflight client or an implicit resource-management callback.
See Trading's `src/drasi/TradingProvider.tsx` and `ensureTradingResources.ts`
for the bounded, shared recovery implementation.

## Styling

The package ships complete, namespaced component CSS. Consumers do not need
Tailwind and do not need to scan package source:

```ts
import '@drasi/react/styles.css';
```

Colors and the default table height are themeable through the `--drasi-*` CSS
custom properties. Visual elements are also overridable through the `className`,
`tableClassName`, `headerClassName`, `rowClassName`, and per‑column `className`
props, so you can match any design system.

## Project structure

The source is organized so the public API is a single barrel (`src/index.ts`)
re‑exporting three groups:

```
src/
  index.ts          # public barrel
  types.ts          # shared public types
  client/           # framework-agnostic core (no React): DrasiClient, DrasiSSEClient
  react/            # React bindings: DrasiProvider, hooks, useRowAnimation
  components/       # ready-made UI: QueryTable, CodeViewerDialog, icons
```

Consumers only ever import from the package root (`@drasi/react`); the internal
folders are an implementation detail.

## Building from source

```bash
# from this directory (dev-tools/react)
npm ci           # install committed-lock dependencies
npm run build    # bundle ESM + CJS + .d.ts to ./dist
```

Other scripts:

| Script | Description |
| --- | --- |
| `npm run dev` | Rebuild on change (`tsup --watch`). |
| `npm run typecheck` | Type-check without emitting. |
| `npm test` | Run client, hook, lifecycle, and component regression tests. |
| `npm run test:coverage` | Measure V8 coverage across all package source files. |
| `npm run clean` | Remove `dist/`. |

The build (via [`tsup`](https://tsup.egoist.dev/)) emits ES modules
(`dist/index.js`), CommonJS (`dist/index.cjs`), and TypeScript declarations to
`dist/`. `styles.css` is shipped alongside those artifacts. Both `dist/` and
`node_modules/` are git‑ignored.

## Future publishing

Publishing is intentionally outside the scope of the current staging work.
`package.json` is marked `"private": true` so it cannot be published
accidentally. The later repository move must update repository links, remove the
private flag, choose the initial version, reserve the npm scope, and add a
trusted release workflow with npm provenance. `prepack` already performs a clean
build, and CI verifies the packed artifact before that transition.

## Using it in the Trading example

The Trading dashboard (`examples/trading/app`) consumes this package as
`@drasi/react` through a local `file:../../../dev-tools/react` dependency. Its
`predev` and `prebuild` scripts build the package first, and the app imports the
package stylesheet. There are no Vite or TypeScript source aliases and no
consumer Tailwind scanning, so broken exports, declarations, or styles cannot be
hidden by monorepo-only configuration.

The [Trading behavior baseline](../../examples/trading/TESTING.md) documents
app integration tests, deterministic three-engine browser checks, fixed-viewport
visual comparisons, and the separate real-server gate. CI also tests Trading
from a clean tarball consumer with lifecycle rebuilding disabled; synthetic
transport fixtures are not evidence of a supported server/plugin wire contract.
Package tests include non-Trading telemetry fixtures without adding another demo.

All trading‑specific behaviour (provisioning, the query list, the SSE Reaction, the
content‑router for aggregation result changes, and per‑query key/transform/sort
options) lives in the app under `src/drasi/`, demonstrating how an application
supplies its domain knowledge to these otherwise‑generic components.

## License

Apache License 2.0. See the license headers in each source file and the
repository's top‑level `LICENSE`.
