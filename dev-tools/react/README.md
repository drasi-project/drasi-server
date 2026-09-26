# @drasi/react

React building blocks for UIs driven by [Drasi](https://drasi.io) Continuous
Queries: one shared SSE connection, query hooks, a provider-free `DataTable`
and a small live `QueryTable` composition. Tables support shared or local
sorting, state slots and optional row animation without requiring tutorial
or overlay UI. The client connects to **existing resources**. Provisioning,
query definitions, transforms, business routing, inspection/fullscreen UI and
deployment defaults belong to the application.

**Private, unpublished staging package.** It remains in `dev-tools/react` in
this repository. Trading uses a local `file:` dependency; no npm publication,
repository transfer or separate framework-agnostic package is implied.

## Installation and entrypoints

Build the local package before consuming it:

```sh
npm --prefix dev-tools/react ci
npm --prefix dev-tools/react run build
```

Repository consumers such as Trading declare
`"@drasi/react": "file:../../../dev-tools/react"`. A source-free test consumer
can instead install the `.tgz` produced by `npm pack`. No source aliases,
Tailwind source scanning or install-time rebuilding are needed to consume it.

| Import | Contents and dependencies |
| --- | --- |
| `@drasi/react/client` | `DrasiClient`, `DrasiSSEClient`, `DrasiError`, result adapters, `accumulateResult` and connection/transport/read/result types. **No React/React DOM runtime or type imports.** |
| `@drasi/react/react` | Providers, query/status/definition hooks, `useTableSort`, `useRowAnimation` and named result/options types, including `SortConfig`. React, but no composed components, React DOM, tutorial code or CSS. |
| `@drasi/react/components` | Provider-free `DataTable`, live `QueryTable`, pure `queryTableState`, optional icons and typed column/action/state/slot/props contracts. React, `clsx` and headless hooks; no tutorial/dialog/fullscreen implementation or implicit CSS. `SortConfig` is also re-exported here. |
| `@drasi/react` | Deliberate convenience re-exports of all three groups. This is **not** a React-free import. |
| `@drasi/react/styles.css` | Explicit package-owned stylesheet, never imported by the JS modules. |

All four JS entrypoints ship real ESM/CommonJS exports and conditional `.d.ts`
and `.d.cts` declarations. Import these paths, not `src`, `dist` or hashed
shared chunks. The framework-agnostic client is a module of this same package.

Build output compacts whitespace only: syntax/identifiers are not minified,
debug/component names are preserved, and source maps retain source content.
Declarations, documentation, CSS and license notices remain in the package.

React and React DOM are **optional installation peers** so client-only
consumers need not install them. React is required to execute the
React/component/root entrypoints; browser/SSR applications supply their renderer.
Install the tested React and React DOM **18.3.1** versions for those applications.
The headless hooks and table implementation do not import React DOM. No React
copy is bundled or hidden in another dependency. The exact peer range
deliberately makes no React 19 or future-major claim.

Import the component stylesheet once when using presentation:

```ts
import '@drasi/react/styles.css';
```

## Quickstart

Provision resources separately. This example requires instance `analytics`, a
running `readings` query and a running `sse` reaction `events` whose query list
includes `readings`. The SSE endpoint must be browser-reachable, including
proxy routing and CORS. Server bind addresses/ports are not client options.

```tsx
// @drasi-docs: quickstart.tsx
import { DrasiProvider, useDrasiConnectionStatus } from '@drasi/react/react';
import { QueryTable } from '@drasi/react/components';
import type { ReactionReference } from '@drasi/react/client';
import '@drasi/react/styles.css';

const QUERY_IDS = ['readings'];
const REACTION: ReactionReference = {
  id: 'events', endpoint: 'https://events.example/changes',
};
interface Reading { id: string; value: number }

function Readings() {
  return <QueryTable<Reading>
    queryId="readings"
    rowKey={row => row.id}
    queryOptions={{
      getKey: row => {
        if (typeof row.id !== 'string' || !row.id) throw new Error('Expected a stable reading ID');
        return row.id;
      },
      transform: row => {
        if (typeof row.id !== 'string' || typeof row.value !== 'number') {
          throw new Error('Expected a Reading');
        }
        return { id: row.id, value: row.value };
      },
    }}
    columns={[
      { key: 'id', label: 'Sensor' },
      { key: 'value', label: 'Value', align: 'right', format: (_raw, row) => row.value.toFixed(2) },
    ]}
    animateOnChange="value"
  />;
}

function ConnectionBadge() {
  const status = useDrasiConnectionStatus();
  return <span>{status.error?.message ?? (status.connected ? 'Stream open' : 'Connecting')}</span>;
}

export default function App() {
  return <DrasiProvider
    serverUrl="https://drasi.example" instanceId="analytics"
    queryIds={QUERY_IDS} reaction={REACTION}
  >
    <ConnectionBadge />
    <Readings />
  </DrasiProvider>;
}
```

Initialization reads the referenced queries concurrently, waits for the bounded
batch to settle, then checks the reaction and opens **one** SSE connection.
Initialization, subscriptions, reconnect and retry perform **GETs only**.
There is no resource-management mode, implicit instance, query-language choice
or deployment reconciler. Each subscription's `getQueryResults` also validates
its query resource before fetching the snapshot. These required validation GETs
are distinct from optional, app-triggered query-definition inspection.

## Server and read-only DTO contract

Every REST operation uses
`/api/v1/instances/{encodedInstanceId}/...`; there is no discovery-order
fallback. Subscriptions must belong to the configured `queryIds`. Optional
inspection may read another query within that same explicit instance.
Validation checks lifecycle status, SSE reaction kind and query membership.
It does not compare desired query text or overwrite deployment settings.

Full-view responses contain
`{ success: true, data: { id, status, links, config, error_message? }, error: null }`.
The request is `GET .../queries/{id}?view=full` or
`GET .../reactions/{id}?view=full`. The exported read types are:

| Type | Validated fields |
| --- | --- |
| `Component<T>` / `ComponentLinks` | Matching resource ID, known lifecycle status, optional string `error_message`, full config and links belonging to the requested instance/resource. |
| `QueryConfig` | `id`, `autoStart`, `query`, explicit `queryLanguage` (`Cypher` or `GQL`), `middleware`, `sources`, `enableBootstrap`, `bootstrapBufferSize`, `outboxCapacity`, `bootstrapTimeoutSecs`; optional joins, priority/dispatch capacities, `dispatchMode: 'Channel'`, storage backend. |
| `QuerySource` / `QueryJoin` / `QueryJoinKey` | Full source subscriptions (`sourceId`, `pipeline`, `nodes`, `relations`) and ordered join keys. No source/join reordering. |
| `QueryMiddleware` | Kind, name and JSON configuration owned by the server plugin. |
| `ReactionConfig` / `SseReactionConfig` | ID, kind, query membership and flattened plugin JSON properties. Supported SSE reads also validate host, port, SSE path and heartbeat interval. |
| `JsonValue` / `ResultRow` | Plugin-owned JSON configuration and object rows whose application-specific fields remain `unknown`. |

These are **read DTOs, not creation DTOs**. No pass-through creation fields,
ignored deployment options or implicit Cypher selection are offered.
Storage backend JSON and other plugin-owned JSON are exposed for inspection,
not interpreted/executed by this client. Unknown additive server fields are
not advertised as writable options. Required fields, enums, integer ranges,
nested shapes and snapshot object rows are checked at runtime from `unknown`.
Malformed/incompatible bodies fail visibly with `INVALID_PAYLOAD`; missing
reads throw rather than returning a success-shaped `null`.

Server 0.2.3's `enableArchive` and `memoryBudgetMiB` settings belong to the
server/instance configuration, not new fields in the unchanged v1 query read
DTO. The client does not expose an instance-creation API. Per-query storage
overrides remain opaque `storageBackend` JSON; no archive or memory-budget
defaults are selected here.

Current v1 links may spell identifiers literally. The client checks their
identity and returns safely encoded, instance-scoped links. It never follows
arbitrary response links, and refuses REST redirects into another path/origin.
The UI helper returns `${serverUrl}/ui?instance={encodedInstanceId}`.

The historical `test/fixtures/server-v1/contract.json` contains **unmodified actual response
bodies** for a full query, full SSE reaction and snapshot, with backend
revision/binary/lock/plugin provenance. They were captured by the real Trading
harness, not inferred from synthetic fakes. Tests consume those records and
separately mutate them to exercise malformed, unauthorized and cross-instance
cases. The historical `test/fixtures/server-v1-0.2.3/contract.json` separately
records the own rebuilt server 0.2.3 / SSE 0.3.6 / ABI 0.13 responses; the same public
read tests consume both versions. New records are never substituted into the
old provenance. See [verified compatibility](#verified-compatibility).

## Authentication and injected transports

`fetch` has the native fetch signature. Every request receives `method: 'GET'`,
fresh headers, credentials, `redirect: 'manual'` and an `AbortSignal`.
Native fetch is invoked with its global receiver, preserving browser binding.
The default credentials are `same-origin`; REST defaults to
`Accept: application/json`.

`headers` accepts `HeadersInit` or an async/sync `DrasiHeadersProvider`.
The provider receives `{ url, transport: 'rest' | 'sse', signal, instanceId }`
and runs for **each** REST read and stream attempt. A stable function can
refresh tokens without replacing the client. Static headers are copied at
construction; a transport cannot mutate another request's headers. The library
does not log credentials, token-bearing URLs, response bodies or user callback
exception details.

```ts
// @drasi-docs: auth.ts
import {
  DrasiClient, DrasiError,
  type DrasiHeadersProvider, type EventSourceFactory,
} from '@drasi/react/client';

export function authenticatedClient(
  getAccessToken: (signal: AbortSignal) => Promise<string>,
  openStream: EventSourceFactory,
) {
  const headers: DrasiHeadersProvider = async ({ signal }) => {
    const token = await getAccessToken(signal);
    if (!token) throw new DrasiError('UNAUTHENTICATED');
    return { Authorization: `Bearer ${token}` };
  };
  return new DrasiClient({
    serverUrl: 'https://drasi.example', instanceId: 'analytics',
    queryIds: ['readings'],
    reaction: { id: 'events', endpoint: 'https://events.example/changes' },
    credentials: 'include', headers, eventSourceFactory: openStream,
  });
}
```

The synchronous custom factory receives
`(url, { headers: Headers, credentials, signal })` and returns an
`EventSourceLike`. It must apply these options, honor cancellation/`close()`
and expose the declared callbacks/listener interface. SSE headers default to
`Accept: text/event-stream`. A known `DrasiError` thrown by a factory/auth
provider retains its identity and classification.

**Native EventSource limitations:** it cannot send custom headers, expose
HTTP status or omit same-origin cookies. `include` maps to
`withCredentials: true`; `same-origin` maps to `false`. Custom headers/header
functions or `omit` therefore require a custom factory **when opening a
stream**. Unsupported native authentication fails with `INVALID_CONFIGURATION`,
not silently dropped auth. REST-only inspection does not require a factory.
Do not wrap native EventSource and pretend it supports bearer headers.
Cross-origin cookie use requires appropriate server/proxy credentials and CORS.

The SSE endpoint is supplied by the operator, never inferred from bind settings.
Shared HTTP(S) URL parsing and safety checks do not trim its path or query:
`/proxy/events/` stays distinct from `/proxy/events`, and `?opaque=part/`
retains the slash in the query value. The stream factory and its authentication
context receive that complete serialized URL. Only the **server base** removes
its final path delimiter before API/UI paths are appended.
The supported plugin protocol cannot attest that a proxy routes to the declared
reaction/instance or reveal native SSE redirect destinations. Use a trusted
proxy or custom transport if enforcement is required. A generic stream error
is **never** evidence that a query/reaction is missing: the client revalidates
using read-only REST before choosing recovery.

Callbacks should honor their signal. Timeouts also settle client-facing work
when a custom callback ignores cancellation; late completions cannot open a
stream or publish data. Explicit read cancellation preserves the caller's
original `signal.reason`. Malformed headers/options are permanent
`INVALID_CONFIGURATION`; invalid JSON/DTOs are `INVALID_PAYLOAD`;
REST network/body/timeouts are retryable `SERVER_UNAVAILABLE`. A 401/403 never
authorizes provisioning.

## Ownership and reconfiguration

`DrasiProvider` owns a client, shared stream, initialization, retries and
cleanup. Equivalent inline reference arrays, reaction/reconnect objects and
headers do **not** bounce the connection. Query IDs compare as a set (duplicates
remain invalid), header names/values are canonicalized, and omitted policy
fields equal the documented defaults.

Equivalent server-base spellings (including an optional trailing slash, host
case or the standard explicit port) retain the same client/connection. Endpoint
path/query differences, including the slash examples above, remain material:
changing one closes the old stream and initializes the replacement.

Material server, instance, reaction ID, endpoint, query-set, credential/header,
timeout, retry-policy or reconciliation-limit changes replace the lifecycle. Old requests are aborted,
the old stream closes, and late events/snapshots/definition reads cannot update
the new scope. Rows/config from another scope are hidden while replacement
work starts; last-good data is retained only within its own scope.

**Callable options compare by identity:** `fetch`, `eventSourceFactory`,
`resultAdapter` and a header-provider function. Memoize them when their
meaning is unchanged. Replacing a function replaces the lifecycle; a stable
auth function may return a fresh token on the next request. Query-hook
`getKey`/`transform`/`postProcess` changes instead reproject retained raw rows
immediately, without resubscribing or reopening the stream.

`useDrasiClient().retry()` restarts that provider's **shared**, read-only
connection/snapshot lifecycle. A retained retry from an obsolete configuration
does not restart its replacement. `useDrasiQuery(...).retry()` and a direct
subscription's `.retry()` refresh only that subscription's REST baseline.
Automatic transient snapshot retries are also query-local and do not disconnect
other queries. A query-local retry cannot repair a failed shared connection.
A direct `DrasiClient` has immutable configuration: construct/disconnect a new
client for material changes. `initialize()` shares in-flight work and reuses a
connected client. Its owner must dispose subscriptions and call `disconnect()`.

`DrasiClientProvider` instead binds an **app-owned** lifecycle. The binding
does not initialize/disconnect, retry, provision or open a second connection.
Its owner supplies coherent state and scoped retry and disposes old clients on
reconfiguration. Descendant hooks perform their own read-only
subscriptions/snapshots once `initialized` is true.

```tsx
// @drasi-docs: controlled.tsx
import type { ReactNode } from 'react';
import { DrasiClientProvider, type DrasiContextValue } from '@drasi/react/react';

export function AppOwnedBinding({ value, children }: {
  value: DrasiContextValue;
  children: ReactNode;
}) {
  return <DrasiClientProvider value={value}>{children}</DrasiClientProvider>;
}
```

Trading's `TradingProvider.tsx` attempts the same client, catches only eligible
known-resource failures, runs its app-owned bounded `ensureTradingResources`,
then retries once. Its serial explicit-Cypher creation bodies, conflicts,
shared work, cancellation and Web Locks stay outside the package.

## API reference

### Connection options

`DrasiProviderProps` extends `DrasiClientOptions` with required React `children`.
`ReactionReference` contains only the existing `id` and absolute HTTP(S)
browser `endpoint`. Relative URLs, URL user/password credentials, fragments,
wildcard bind hosts, empty/dot identifiers and duplicate query IDs are rejected.

| Option | Default / meaning |
| --- | --- |
| `serverUrl: string` | Required absolute HTTP(S) server base, no query/fragment/default. |
| `instanceId: string` | Required explicit instance, never discovered. |
| `queryIds: readonly string[]` | Required existing query references; no definitions. |
| `reaction: ReactionReference` | Required existing SSE reaction and browser URL. |
| `resultAdapter?: ResultAdapter` | Defaults to strict `sse034ResultAdapter`. Select `createLegacyResultAdapter(...)` or supply a validating custom adapter explicitly. |
| `reconciliation?: ResultReconciliationOptions` | `maxPendingChanges: 10000`; a positive safe integer bounding changes observed during a pending snapshot, **not** result-set size. |
| `fetch?: typeof fetch` | Defaults to global native fetch; GETs only. |
| `headers?: DrasiHeaders` | No custom headers by default; static input or per-request provider. |
| `credentials?: RequestCredentials` | `same-origin`; native SSE cannot implement `omit`. |
| `eventSourceFactory?: EventSourceFactory` | Native browser EventSource by default. |
| `requestTimeoutMs?: number` | 10000 ms **per REST request**, including auth/body. Composite reads can make multiple requests. |
| `reconnect?: ReconnectOptions` | Defaults below; same count/backoff for streams and snapshots. |

`ReconnectOptions` defaults: **10 retries after the first attempt**,
`initialReconnectDelayMs: 1000`, `maxReconnectDelayMs: 30000`,
`connectionTimeoutMs: 10000`. Backoff is exponential; counts reset on a
successful open/snapshot. Open timeout includes stream auth and waiting for
open, not idle lifetime of an already-open stream. Permanent failures stop
immediately; exhaustion retains the last typed error and stops timers/work.
Concurrent resource validation drains its bounded batch before failure handoff
so app recovery does not race an abort storm. Explicit cancellation aborts all.

### Normalized results and explicit wire adapters

`/client` and the root export one normalized result contract:

| Type | Shape |
| --- | --- |
| `ResultChange<T = ResultRow>` | `{ kind: 'upsert', after: T }`, `{ kind: 'update', before: T, after: T }` or `{ kind: 'delete', before: T }`. |
| `QuerySnapshot<T = ResultRow>` | `{ kind: 'snapshot', queryId, rows: readonly T[], receivedAt: number }`. |
| `QueryDelta<T = ResultRow>` | `{ kind: 'delta', queryId, changes: readonly ResultChange<T>[], receivedAt: number, sourceTimestamp?: number }`. |
| `QueryResult<T = ResultRow>` | `QuerySnapshot<T> \| QueryDelta<T>`. Narrow `kind` before reading `rows` or `changes`. |
| `ResultAdapter` | `(payload: unknown, context: ResultAdapterContext) => readonly QueryDelta[]`. |
| `ResultAdapterContext` | Extends `Readonly<DrasiErrorDetails>` with required read-only `receivedAt: number`; all error-detail fields remain optional. |

Rows crossing the wire boundary must be JSON objects; their fields remain
`unknown`. Adapters are synchronous, and even a custom adapter's normalized
output is validated at runtime. An adapter does not establish an application's
row schema. `receivedAt` and `sourceTimestamp` are display metadata only,
**never** ordering, identity, deduplication or synchronization cursors.
The stream supplies configured reaction and instance metadata in the context.
New malformed/unroutable unidentified failures retain those reaction details;
identified query failures use query details instead. Existing `DrasiError`
objects retain their identity rather than being rewrapped.

The default **`sse034ResultAdapter`** accepts the recorded **untemplated SSE**
envelope `{ queryId, results, timestamp }`. Its name identifies the original
0.3.4 contract. Historical 0.3.6 captures exercise the same observed
ADD/DELETE/UPDATE/aggregation shapes without changing this API. This is not a
claim about every plugin version, custom template or unobserved variant:

- `ADD` with object `data` becomes an upsert; `DELETE` with object `data`
  becomes a delete.
- `UPDATE` requires object `before`, `after` and `data`, with optional string
  array `grouping_keys`. Normalized updates retain both `before` and `after`.
- `aggregation` requires `before` (an object or `null`) and object `after`.
  A null `before` becomes an upsert; otherwise it becomes an update.
- `noop` contributes no normalized changes. Noop-only and empty result batches
  have empty `changes` and are ignored by the transport; they do not trigger
  snapshot-overlap recovery. `{ type: 'heartbeat', ts }` produces no deltas.
  Valid batches for queries with no subscribers are also ignored, not
  classified as corruption.

There is no field-shape guessing, lowercase-operation fallback, implicit
`query_id` alias or custom-template interpretation in this default adapter.
Official `row_signature` values are JSON numbers representing upstream `u64`s:
unsafe integers can already have lost precision in `JSON.parse`. They are
ignored, not exposed as stable row identities or used to deduplicate results.

For explicit compatibility, use
`resultAdapter: createLegacyResultAdapter({ routeUnidentified })`.
`LegacyResultAdapterOptions` accepts the app-owned `RouteUnidentified`
callback. This adapter accepts `query_id`, `results` entries with `op`
`c`/`r`/`u`/`d` or lowercase operation `type`, keyed `data`/`_deleted` rows,
and `addedResults`/`updatedResults`/`deletedResults` arrays with before/after
wrappers or direct rows. It is a migration adapter, not another claim about
the official plugin protocol. An explicit query ID is always honored before
content routing; the callback cannot override it.

Unidentified routing receives rows and `deliver(queryId, rows)`. It must
**synchronously deliver every original row reference supplied to that callback**
at least once, to a valid query ID. Fan-out is supported. Do not clone, replace,
silently drop or asynchronously deliver those rows. Missing/failed routing
raises `UNROUTABLE_RESULT`; an empty/no-op batch needs no route.
The legacy row callback cannot retain a before/after update as one operation:
an unidentified update is flattened into **delete-before, then upsert-after**.
Route both halves correctly, or implement a `ResultAdapter` that preserves the
normalized update. This compatibility path does not promise atomic routing of
a key-changing update.

Raw `_deleted` has deletion semantics **only inside the selected legacy
adapter**. In a normalized result or REST row it is an ordinary application
field. `accumulateResult(rows, result, getKey, details?)` is the exported,
framework-independent raw reducer: snapshots replace raw state, same-key
upserts replace idempotently, deletes remove their `before` key, and updates
remove a changed `before` key before storing `after`. Equal-valued rows with
different domain keys remain distinct. There is no serialized-value deduplication.

### Hooks, raw identity and derived views

| Hook | Named result |
| --- | --- |
| `useDrasiClient()` | `DrasiContextValue`: `{ client, initialized, error, retry }`. |
| `useDrasiQuery<T extends object = ResultRow>(queryId, options)` | `UseDrasiQueryResult<T>`: `{ data: T[] \| null, status, stale, loading, error, errorScope, lastUpdate: Date \| null, retry }`. **Options are required.** |
| `useDrasiConnectionStatus()` | `ConnectionStatus`: `connected`, optional `reconnecting`, `DrasiError`, `lastConnected`. A socket opening does not prove query synchronization. |
| `useDrasiQueryDefinition(queryId)` | `UseDrasiQueryDefinitionResult`: `{ config: QueryConfig \| null, loading, error }`. |
| `useDrasiServerUiUrl()` | Instance-scoped UI URL when initialized, otherwise `null`. |
| `useRowAnimation(options)` | `UseRowAnimationResult`; exported `AnimationDirection` and `UseRowAnimationOptions`. |
| `useTableSort(options?)` | `UseTableSortResult`: `{ sort, setSort, toggleSort }`; provider-free, with `UseTableSortOptions` and `SortConfig`. See [sorting](#sorting). |

All Drasi provider/query hooks preserve `DrasiError` objects, not just messages.
`UseDrasiQueryOptions<T extends object = ResultRow>` requires both:

- `getKey: RowKey`, where `RowKey = (raw: Readonly<ResultRow>) => string`.
  Return a **nonempty stable domain identity** from the raw row, including
  sparse deletes and both sides of updates. Identity is extracted **before**
  transformation. There is no `id`, `symbol`, serialization or table-key fallback.
  Invalid/throwing key extraction is visible as `INVALID_ROW_KEY`, not a skip.
- `transform: (raw: Readonly<ResultRow>) => T | null`. Validate unknown fields
  to produce a typed object; use `row => row` for raw reads. **Deletes never run
  through this transform.** Returning `null` hides an identity from the derived
  view but retains its raw row for later updates and option changes.

Optional `postProcess: (rows: T[]) => T[]` is a pure derived sort/filter; it
does not remove or mutate the accumulated raw state. All three options reproject
retained rows reactively without a new socket or subscription. Keep callbacks
pure and do not mutate their read-only raw input.

Active stream batches use the **latest committed `getKey`**, never a callback
from a suspended or abandoned render. During a `startTransition` that suspends,
the current view and its subscription keep their committed identity policy.
A successful option commit publishes that key before consumer layout effects
can deliver events; committed transforms and post-processing still reproject
retained rows without reconnecting. This commit-only publication performs no
browser-global detection or server-rendering layout effect.

```ts
// @drasi-docs: query-options.ts
import { useDrasiQuery, type UseDrasiQueryOptions } from '@drasi/react/react';
import type { RowKey } from '@drasi/react/client';

interface Reading { device: string; value: number }
const readingKey: RowKey = raw => {
  if (typeof raw.device !== 'string' || !raw.device) throw new Error('Expected a stable device ID');
  return raw.device;
};

export const readingOptions: UseDrasiQueryOptions<Reading> = {
  getKey: readingKey,
  transform: row => {
    if (typeof row.device !== 'string' || typeof row.value !== 'number') {
      throw new Error('Expected a Reading');
    }
    return { device: row.device, value: row.value };
  },
  postProcess: rows => [...rows].sort((a, b) => b.value - a.value),
};

export const rawReadingOptions: UseDrasiQueryOptions = {
  getKey: readingKey,
  transform: row => row,
};

export function useReadings() {
  return useDrasiQuery<Reading>('readings', readingOptions);
}
```

A generic alone is not validation and cannot replace `transform`. Transform
or post-processing failures become `RESULT_PROCESSING_FAILED`; existing
`DrasiError` instances retain their identity. Invalid keys are not hidden by
a transform returning `null`.

`QueryStatus` describes the query, not merely the shared socket:

| `status` | Meaning |
| --- | --- |
| `initial-loading` | No accepted baseline yet. |
| `live` | A best-effort baseline was accepted and subsequent changes are being applied; **not** proof of gap-free synchronization. |
| `empty` | That baseline's current projected view is empty, possibly because transforms/filters hide rows. |
| `reconnecting` | Waiting on the shared stream; any last-good rows are stale. |
| `resynchronizing` | Reading/retrying a fresh REST baseline, including known-overlap recovery. |
| `stale-last-good-data` | A transient query refresh failure left useful last-good rows while recovery continues. |
| `terminal-error` | A permanent fault or exhausted retry budget; correction and explicit retry are needed. Last-good rows may still be available. |

`errorScope: QueryErrorScope | null` is `'query'`, `'connection'` or `null`.
`stale`, `data` and `lastUpdate` let a UI retain and label useful last-good data
during recovery or failure. `lastUpdate` is display metadata, not a server
watermark. `loading` is not a complete synchronization state: inspect `status`
and `stale`. Per-query processing and subscription REST failures are isolated
while the shared transport remains healthy. This is **not whole-connection
fault isolation**: initial validation and shared reconnection revalidate
**all configured query references** and the reaction, as in Part A. A reference
failure during that shared validation can block the shared connection and
affect every subscription. Malformed unidentifiable/protocol failures can also
terminate the shared stream.

### Snapshot/stream consistency and limits

Subscriptions attach to the stream **before** starting REST validation/fetch.
Without a cursor shared by REST and SSE, an overlapping delta cannot be safely
ordered relative to that snapshot. If any changes arrive during the pending
read, the client **rejects the ambiguous snapshot and does not replay those
changes**, reports retryable `SNAPSHOT_OVERLAP` and visibly resynchronizes through
a bounded refresh. It never silently labels that overlapping candidate live.

Pending changes are **counted, not stored as row bodies**. Exceeding
`reconciliation.maxPendingChanges` (default **10000**) aborts the pending work
and reports retryable `RESULT_BUFFER_OVERFLOW` with resynchronizing state.
The limit bounds the overlap window, **not** the size of the query result set.
Refresh/reconnect replaces the baseline; it does not merge old results with a
guessed replay. Retries use the existing bounded policy above (ten retries
after the initial attempt by default). Continuous overlap can exhaust it and
leave `terminal-error` with stale last-good rows until explicit retry.

A read with **no known overlap** is only a **best-effort baseline**.
Undetectably delayed events and causality across the two transports remain
limits: there is no atomic, gap-free or exactly-once snapshot/live handoff.
For example, an older SSE change can arrive only after a newer REST baseline
was accepted, temporarily replacing a row with older state despite no observed
overlap. The client cannot detect that delay or automatically label it stale;
explicit query refresh reads a new baseline. This is a tested limitation, not
an out-of-order-event suppression guarantee.
Never order client/server clocks or row signatures to manufacture one.
Stronger semantics require a future **shared snapshot cursor and stream
resume/replay protocol**; this client does not invent backend support.

### Components

#### Provider-free DataTable

`DataTable<T extends object, E extends Error = Error>` renders supplied rows.
It does not read resources, subscribe, inspect definitions, require a Drasi
provider or own an overlay. Non-Drasi data and ordinary application errors are
supported. `DataTableProps` defaults `T` to `Record<string, unknown>`.

```tsx
// @drasi-docs: data-table.tsx
import { DataTable, type ColumnDef } from '@drasi/react/components';
import '@drasi/react/styles.css';

interface Delivery { id: string; destination: string; parcels: number }
const rows: readonly Delivery[] = [
  { id: 'north', destination: 'Warehouse A', parcels: 12 },
  { id: 'south', destination: 'Warehouse B', parcels: 3 },
];
const columns: readonly ColumnDef<Delivery>[] = [
  { key: 'destination', label: 'Destination', align: 'left' },
  { key: 'parcels', label: 'Parcels', align: 'right' },
  {
    key: 'summary', label: 'Summary', sortable: false,
    format: (_value, row) => `${row.parcels} parcels to ${row.destination}`,
  },
];

export default function Deliveries() {
  return <DataTable<Delivery>
    title="Deliveries" rows={rows} columns={columns} rowKey={row => row.id}
  />;
}
```

All presentation props are optional except the first three:

| `DataTableProps<T, E>` prop | Default / callback contract |
| --- | --- |
| `rows: readonly T[] \| null` | Required. `null` means no accepted baseline; `[]` is an accepted empty view. Supplied arrays are never sorted in place or otherwise mutated. |
| `columns: readonly ColumnDef<T>[]` | Required ordered column definitions, never mutated. |
| `rowKey: (row: T) => string` | Required stable render/animation identity. Use domain keys, not array positions. This is not raw query accumulation identity. |
| `state?: DataTableState<E>` | `{}`; presentation-only loading/error/stale/retry state, described below. |
| `sort`, `defaultSort`, `onSortChange` | Same contract as `useTableSort`; initially unsorted unless a sort is provided. See [sorting](#sorting). |
| `title?: string` | No title by default. |
| `headerActions?: ReactNode` | Content beside the title on the left. |
| `headerControls?: ReactNode` | Content on the right; no built-in code-view/fullscreen controls. |
| `headerSlot?: ReactNode` | Content below the header, above the table. |
| `renderHeader?: (context: DataTableHeaderContext<T, E>) => ReactNode` | Replaces the header. Call `context.defaultRender()` to wrap/augment the default title/actions/controls; return `null` to omit it. |
| `actions?: readonly RowAction<T>[]` | No actions by default. Nonempty actions add one trailing column. |
| `actionsWidth?: string` | Optional CSS width of the actions heading. |
| `animateOnChange?: keyof T` | Disabled by default. Track the named row property; numbers animate up/down and changed strings animate `change`. |
| `rowAnimations?: ReadonlyMap<string, AnimationDirection>` | Optional owner-supplied animation state keyed by `rowKey`; takes precedence over `animateOnChange`, including an empty map. |
| `renderRow?: (row, columns, animation, defaultRender) => ReactNode` | Replaces one row's rendering. `row` is `T`, `columns` is readonly, and `animation` is `'up' \| 'down' \| 'change' \| null`. Return a `<tr>` or a fragment of table rows, or call `defaultRender()` for the built-in `<tr>`. A parent fragment already owns the stable `rowKey`; wrappers need not invent an index key. |
| `emptyMessage?: string` | `"No data available"`; `renderEmpty` takes precedence. |
| `renderLoading`, `renderEmpty`, `renderError`, `renderStale` | Optional state slots; details below. A slot returning `null` suppresses its default rather than falling back. |
| `className?: string`, `style?: CSSProperties` | Additional card class and inline styles. |
| `containerRef?: Ref<HTMLDivElement>` | Ref to the card for app-owned layout/composition. |
| `tableClassName?: string` | Additional **scroll-container** class, not a class on `<table>`. |
| `headerClassName?: string` | Additional `<thead>` class; individual headings use `ColumnDef.headerClassName`. |
| `titleClassName?: string` | Additional normal-header title class. |
| `headerSlotClassName?: string` | Additional wrapper class for `headerSlot`. |
| `rowClassName?: string \| ((row: T, index: number) => string)` | Additional class on the default row; `index` is its position in the sorted view. |
| `height?: string` | Legacy additional **class name**, not a CSS length. The existing CSS default is 400px. Height-prop completion belongs to P6; passing `"500px"` here does not set a pixel height. |

`DataTableState<E>` has no connection/query semantics:

| Field | Default / meaning |
| --- | --- |
| `loading?: boolean` | Not loading unless true. |
| `error?: E \| null` | No error unless supplied. Any `Error` subtype is supported, not only `DrasiError`. |
| `stale?: boolean` | Marks supplied data as last-known rather than current. |
| `retry?: () => void` | App-chosen recovery; omitted means no default retry button. |
| `retryLabel?: string` | `"Retry"` on the default error button. |
| `staleMessage?: string` | `"Showing last known data."` for the default stale notice. |

Notice precedence is **error > stale > loading**; lower-priority slots do not
run. An error's default notice also says "Showing last known data." when stale.
When `rows === null` and error or loading is set, the component shows a state
card with the optional title and selected notice, not a table or normal
header/header slot. Supplied rows, including `[]`, always retain the table.
Without that state-card condition, `null` and `[]` use the empty cell; neither
is silently converted into invented data.

`DataTableRenderContext<T, E>` contains
`{ rows: readonly T[] | null, state: DataTableState<E>, sort: SortConfig | null,
setSort: (next: SortConfig | null) => void }`. `rows` is the **sorted view**.
Loading, empty and stale slots receive that context. `renderError` receives
`DataTableErrorContext<T, E>` with an additional **guaranteed `error: E`**.
`renderHeader` receives `DataTableHeaderContext<T, E>` with `defaultRender`.
`renderEmpty` runs **inside one spanning `<td>`**: return cell content, not a
`<tr>` or another `<td>`. These presentation callbacks never receive a client.

#### Columns and actions

`ColumnDef<T>` supports computed string keys, so its raw cell argument is
`unknown`, not an asserted property type. Use the typed row or narrow the value.

| Column field | Default / meaning |
| --- | --- |
| `key: keyof T \| string`, `label: string` | Required property/computed key and heading text. |
| `format?: (value: unknown, row: T) => ReactNode` | Default: nullish values render `"-"`, otherwise `String(value)`. Computed labels do not create a sortable row property. |
| `sortable?: boolean` | `true`; `false` disables that heading's sort interaction. |
| `align?: 'left' \| 'center' \| 'right'` | Explicit alignment applies to header and body. Omitted: header left, body inherits host alignment. |
| `className?: string \| ((value: unknown, row: T) => string)` | Additional cell class. |
| `headerClassName?: string` | Additional heading class. |
| `width?: string` | Optional CSS width of the heading. |

`RowAction<T>` requires `icon: ReactNode`, `label: string` and
`onClick: (row: T) => void`. The label supplies the button's accessible name
and title. Optional `disabled(row)` and `loading(row)` return booleans and
default to false; either disables the button and prevents its click callback.
Loading shows the existing spinner instead of the icon. Optional `className`
and `hoverClassName` customize the button; the hover class applies only when
enabled and not loading.

#### Sorting

`useTableSort(options?: UseTableSortOptions)` is a provider-free headless
controller exported by `/react`. Both `DataTable` and Trading's two table
presentations use this same controller. `SortConfig` is
`{ column: string, direction: 'asc' | 'desc' }`, exported by **both** `/react`
and `/components`.

| Option/result | Contract |
| --- | --- |
| `sort?: SortConfig \| null` | Defined, including `null`, means controlled. `undefined` means uncontrolled. Controlled owners must apply requested changes. |
| `defaultSort?: SortConfig \| null` | `null`; read once on mount for stored uncontrolled state, ignored for the displayed sort while controlled. Later prop changes do not reset that stored value. |
| `onSortChange?: (next: SortConfig \| null) => void` | One notification per `setSort`/`toggleSort` call or sortable-header user action, even under StrictMode. Runs outside the state updater; prop changes do not notify. |
| Returned `sort` | Effective `SortConfig \| null`. Controlled state takes precedence. Returning to uncontrolled restores its stored state; controlled changes do not overwrite it. |
| Returned `setSort(next)` | Requests a config or `null`. Explicit `null` clears sorting to input order. |
| Returned `toggleSort(column)` | New column starts ascending; active column toggles ascending/descending. It does **not** cycle automatically through a third, unsorted state. |

The hook controls state, not row comparison. Ascending `DataTable` ordering is:
JavaScript numbers numerically (including infinities, with NaN after other
numbers), then other non-nullish values by `String(value)`, then null/undefined.
Numeric strings are not coerced. Text uses the shared fixed
`Intl.Collator('en-US', { sensitivity: 'variant', numeric: false })` policy,
not the environment's default locale. Descending reverses this ordering,
including putting nullish values first. Equal keys, -0/0, NaNs and nullish ties
preserve input order. Readonly rows are copied; `sort={null}` restores input order.

SSR and hydration must use the same rows, sort state, string representations
and compatible Intl data. The fixed English policy preserves Trading name
collation across differing default locales; it is not a claim of identical
Unicode ordering across arbitrary ICU versions. Hidden fields remain sortable;
formatted/computed display text is not a comparator. No comparator API is added.

Uncontrolled sorting with an explicit reset action:

```tsx
// @drasi-docs: sort-uncontrolled.tsx
import { DataTable } from '@drasi/react/components';
import type { SortConfig } from '@drasi/react/react';
import '@drasi/react/styles.css';

interface Job { id: string; priority: number }
export function JobQueue({ rows, onSortChange }: {
  rows: readonly Job[];
  onSortChange?: (next: SortConfig | null) => void;
}) {
  return <DataTable<Job>
    rows={rows} rowKey={row => row.id} title="Job queue"
    columns={[{ key: 'id', label: 'Job' }, { key: 'priority', label: 'Priority' }]}
    defaultSort={{ column: 'priority', direction: 'desc' }}
    onSortChange={onSortChange}
    renderHeader={({ defaultRender, setSort }) => <>
      {defaultRender()}
      <button type="button" onClick={() => setSort(null)}>Input order</button>
    </>}
  />;
}
```

Controlled sorting; the owner stores requests, including explicit `null`:

```tsx
// @drasi-docs: sort-controlled.tsx
import { useState } from 'react';
import { DataTable } from '@drasi/react/components';
import type { SortConfig } from '@drasi/react/react';
import '@drasi/react/styles.css';

interface Job { id: string; priority: number }
export function ControlledJobs({ rows }: { rows: readonly Job[] }) {
  const [sort, setSort] = useState<SortConfig | null>(null);
  return <DataTable<Job>
    rows={rows} rowKey={row => row.id} title="Jobs"
    columns={[{ key: 'id', label: 'Job' }, { key: 'priority', label: 'Priority' }]}
    sort={sort} onSortChange={setSort}
    headerControls={<button type="button" onClick={() => setSort(null)}>Input order</button>}
  />;
}
```

#### Live QueryTable and scoped state slots

`QueryTable<T extends object = ResultRow>` is a small composition of
`useDrasiQuery` and `DataTable<T, DrasiError>`. It requires a provider, `queryId`,
`queryOptions: UseDrasiQueryOptions<T>`, `columns` and `rowKey`. Both raw
`getKey` and validating `transform` remain **required** inside `queryOptions`.
`rowKey` is the separate **transformed** render/animation identity, never a
fallback for raw accumulation. All DataTable presentation/sort props apply
except `rows` and `state`, which the composition owns.

Its four state slots use `QueryTableRenderContext<T>`:
the DataTable context plus the full typed `query: UseDrasiQueryResult<T>` and
`retryConnection: () => void`. Inspect `query.status`, `stale`, `errorScope`,
`lastUpdate` and query-local `retry`, not just `loading`.
`QueryTableErrorContext<T>` additionally guarantees `error: DrasiError`.
`renderHeader` still uses the presentation-only `DataTableHeaderContext`;
it is not a query-state slot. All named contexts and props are exported from
`/components`.

`queryTableState(query, retryConnection)` is an exported **pure adapter** to
`DataTableState<DrasiError>`, shared by QueryTable and Trading. It copies
loading/error/stale, chooses shared retry and `"Retry connection"` only when
`errorScope === 'connection'`, otherwise query-local retry and `"Retry query"`.
Its stale message is `"Reconnecting. Showing last known data."` for
`status === 'reconnecting'`, otherwise `"Resynchronizing. Showing last known data."`.
Only exceptional states show these notices; available last-good rows remain.

This component belongs beneath the quickstart provider. The error slot below
keeps P4 state and explicitly selects the correct recovery scope; using
`state.retry` would make the same choice.

```tsx
// @drasi-docs: query-states.tsx
import { QueryTable } from '@drasi/react/components';
import type { UseDrasiQueryOptions } from '@drasi/react/react';
import '@drasi/react/styles.css';

interface Reading { id: string; value: number }
const options: UseDrasiQueryOptions<Reading> = {
  getKey: raw => {
    if (typeof raw.id !== 'string' || !raw.id) throw new Error('Expected a stable reading ID');
    return raw.id;
  },
  transform: raw => {
    if (typeof raw.id !== 'string' || typeof raw.value !== 'number') {
      throw new Error('Expected a Reading');
    }
    return { id: raw.id, value: raw.value };
  },
};

export function ReadingsWithState() {
  return <QueryTable<Reading>
    queryId="readings" queryOptions={options} rowKey={row => row.id}
    columns={[{ key: 'id', label: 'Sensor' }, { key: 'value', label: 'Value' }]}
    renderLoading={({ query }) => <p role="status">{query.status}: waiting for a baseline</p>}
    renderEmpty={() => <span>No readings in this view.</span>}
    renderError={({ error, query, state, retryConnection }) => {
      const retry = query.errorScope === 'connection' ? retryConnection : query.retry;
      return <div role="alert">
        <p>{error.code}: {error.message}</p>
        {query.stale && <p>Showing last known data.</p>}
        <button type="button" onClick={retry}>{state.retryLabel}</button>
      </div>;
    }}
    renderStale={({ query, rows, state }) => <p role="status">
      {state.staleMessage} {rows?.length ?? 0} rows.
      {query.lastUpdate && <> Last update: {query.lastUpdate.toISOString()}</>}
    </p>}
  />;
}
```

#### App-owned composition

For multiple presentations of the same query, own **one** query subscription,
sort controller and animation tracker, then pass their state to DataTable.
`useRowAnimation<T>` accepts `rowKey`, `getValue`, optional
`animationDuration` (500ms) and optional readonly `data`. It also returns
`updateData(readonlyRows)` for manual updates. Null/undefined data retains its
previous baseline; an empty array clears it. Supply `rowAnimations` to avoid
each presentation tracking its own changes.

```tsx
// @drasi-docs: composed-table.tsx
import { DataTable, queryTableState, type ColumnDef } from '@drasi/react/components';
import {
  useDrasiClient, useDrasiQuery, useRowAnimation, useTableSort,
  type UseDrasiQueryOptions,
} from '@drasi/react/react';
import type { DrasiError } from '@drasi/react/client';
import '@drasi/react/styles.css';

interface Reading { id: string; value: number }
const rowKey = (row: Reading) => row.id;
const getValue = (row: Reading) => row.value;
const columns: readonly ColumnDef<Reading>[] = [
  { key: 'id', label: 'Sensor' }, { key: 'value', label: 'Value' },
];

export function ReadingViews({ queryId, queryOptions, showAlternate = false }: {
  queryId: string;
  queryOptions: UseDrasiQueryOptions<Reading>;
  showAlternate?: boolean;
}) {
  const query = useDrasiQuery(queryId, queryOptions);
  const { retry: retryConnection } = useDrasiClient();
  const sorting = useTableSort({ defaultSort: { column: 'value', direction: 'desc' } });
  const { animations } = useRowAnimation({ data: query.data, rowKey, getValue });
  const presentation = {
    rows: query.data, columns, rowKey,
    state: queryTableState(query, retryConnection),
    sort: sorting.sort, onSortChange: sorting.setSort,
    rowAnimations: animations,
  };
  return <>
    <DataTable<Reading, DrasiError> {...presentation} title="Readings" />
    {showAlternate && <DataTable<Reading, DrasiError>
      {...presentation} title="Alternate view"
      headerControls={<button type="button" onClick={() => sorting.setSort(null)}>Input order</button>}
    />}
  </>;
}
```

Trading uses this pattern in its app-owned `TradingQueryTable` for normal and
fullscreen cards. Its `QueryInspector`/`CodeViewerDialog`, query formatting,
snippets and UI links are **not package exports**. An app may opt into
`useDrasiQueryDefinition` by mounting an inspector only after a code click
with a snippet. Trading unmounts it on close (aborting the optional GET).
For the inspector's own read failure, **Retry query definition** rekeys just
that read, not the query subscription or socket. If its non-null error is the
**same object** as `useDrasiClient().error`, the inspector instead offers
**Retry connection** through the provider's retry callback. Matching error
codes/messages alone does not select shared recovery. This is app-owned
composition, not a new package API. Async definition results update an
already-open code view and copy action.
Plain QueryTable/DataTable and closed inspectors perform no tutorial reads.
Required initialization and subscription resource-validation GETs still run
for live queries; removing tutorial coupling does not remove those checks.

Generic `CodeIcon`, `ExpandIcon` and `CollapseIcon` remain optional exports;
rendering an icon alone creates no behavior. CSS remains namespaced,
package-owned and an **explicit** import; consumers do not need Tailwind.
This P5 composition work leaves CSS byte-for-byte unchanged, including legacy
dialog/fullscreen rules still used by Trading. P6 focus management, overlay
coordination, theming, reduced-motion and height completion are separate.
No pagination, selection or virtualization is provided.

### Low-level clients

`DrasiClient` exposes `initialize()`, `validateResources(signal?)`,
`getQuery(id, signal?)`, `getReaction(signal?)`, `getQueryConfig(id, signal?)`,
`getQueryResults(id, signal?)`, `subscribe(id, onResult, onError?, onStateChange?)`,
`getConnectionStatus()`, `onConnectionStatusChange(callback)`,
`getServerUiUrl()`, `isInitialized()` and `disconnect()`.
Direct reads perform one attempt; initialization and subscriptions own the
documented retry policy. Each subscription returns a named `QuerySubscription`:
call it to unsubscribe, call `.retry()` for a query-local REST refresh, or
`.getState()` for its `QuerySubscriptionState`. The optional fourth callback
receives that same state: `{ status, stale, error, errorScope }`. Its status
excludes `'empty'` because only the accumulated/projected hook view knows whether
it has rows.
Provide `onError` to handle subscription failures; if omitted, the library logs
only the safe error code rather than silently swallowing a malformed snapshot.

```ts
// @drasi-docs: client.ts
import {
  DrasiClient, accumulateResult,
  type DrasiError, type QuerySubscriptionState, type ResultRow, type RowKey,
} from '@drasi/react/client';

const readingKey: RowKey = raw => {
  if (typeof raw.id !== 'string' || !raw.id) throw new Error('Expected a stable reading ID');
  return raw.id;
};

// Execute in a browser, not while importing or server rendering.
export async function monitorReadings(
  onRows: (rows: ResultRow[]) => void,
  onError: (error: DrasiError) => void,
  onStateChange?: (state: QuerySubscriptionState) => void,
) {
  const client = new DrasiClient({
    serverUrl: 'https://drasi.example', instanceId: 'analytics',
    queryIds: ['readings'],
    reaction: { id: 'events', endpoint: 'https://events.example/changes' },
  });
  try {
    await client.initialize();
  } catch (error) {
    await client.disconnect();
    throw error;
  }
  let rows: ResultRow[] = [];
  const subscription = client.subscribe('readings', result => {
    rows = accumulateResult(rows, result, readingKey, {
      instanceId: 'analytics', resourceKind: 'query', resourceId: 'readings',
    });
    onRows(rows);
  }, onError, onStateChange);
  return {
    retry: subscription.retry,
    getState: subscription.getState,
    dispose: async () => { subscription(); await client.disconnect(); },
  };
}
```

`DrasiSSEClient` is the lower-level stream multiplexer. `DrasiSSEClientOptions`
adds an optional read-only `validate(signal)` callback and `errorDetails` to
the stream/auth/reconnect options. Direct use does not validate REST resources
unless that callback is supplied and cannot infer their absence. It exposes
`connect(queryIds, endpoint, signal?)`, `subscribe`, `getQueryError(queryId)`,
`getConnectionStatus`, `onConnectionStatusChange`, `isConnected` and `disconnect`.
Its `subscribe` callback receives only normalized `QueryDelta`s and returns a
plain cleanup function; it does not fetch snapshots or offer query-local REST
retry. `getQueryError` exposes a query-scoped fault without conflating it with
the shared connection status.

## Errors and recovery

`DrasiError extends Error` has stable `DrasiErrorCode`, `instanceId`,
`resourceKind`, `resourceId`, `resourceStatus`, optional HTTP `status` and
`retryable`. `DrasiErrorDetails` and `DrasiResourceKind` are exported.
Render `.message`; discriminate with `instanceof DrasiError` and `.code`.
Safe messages do not include raw server details. Explicit cancellation is the
original abort reason rather than a `DrasiError`.

| Code | Meaning / appropriate owner action |
| --- | --- |
| `INSTANCE_NOT_FOUND` | Explicit instance is absent. Correct configuration; not query provisioning. |
| `QUERY_NOT_FOUND`, `REACTION_NOT_FOUND` | Structured REST-confirmed absence. Only the owning app may provision allowlisted resources. |
| `RESOURCE_STOPPED` | Stopped/Added resource. Only its owner may start it. |
| `RESOURCE_STARTING` | Starting/Reconfiguring/bootstrap. Retryable, not absent; no duplicate start. |
| `RESOURCE_UNAVAILABLE` | Error/Stopping/Removed; operator attention. |
| `SERVER_UNAVAILABLE`, `STREAM_UNAVAILABLE` | Retryable service/transport failure, **never permission to create**. |
| `UNAUTHENTICATED`, `FORBIDDEN` | Fix credentials/authorization. No automatic auth/provisioning loop. |
| `INVALID_CONFIGURATION`, `INCOMPATIBLE_RESOURCE`, `INVALID_PAYLOAD` | Invalid references/options, wrong reaction contract, redirect or unsupported data. Permanent until corrected. |
| `SNAPSHOT_OVERLAP` | Retryable known snapshot/delta overlap. The ambiguous candidate is discarded; a bounded query-local baseline refresh follows. |
| `RESULT_BUFFER_OVERFLOW` | Retryable pending-change limit exceeded. Abort the pending read and resynchronize; never truncate to a live result. |
| `INVALID_ROW_KEY` | Terminal raw identity failure, including sparse deletes. Fix `getKey`/the query projection; there is no silent fallback. |
| `RESULT_PROCESSING_FAILED` | Terminal transform, post-processing or subscriber failure. Correct the callback and retry as needed. |
| `UNROUTABLE_RESULT` | Terminal missing/failed explicit legacy routing. Fix the adapter/routing policy; do not guess a query by shape. |

`retryable` means another attempt may be useful, not that retry continues
forever. REST 408/429/5xx and network/body timeouts are retryable; 401/403 and
malformed/incompatible data are permanent. Only a structured matching REST 404
establishes absence; proxy HTML/unstructured 404s do not. Native SSE errors
expose no HTTP status, even when the underlying cause is an auth failure.

## SSR and import safety

All entrypoints are safe to import/require without DOM/network activity.
`DataTable` can server-render real non-Drasi rows and state slots **without a
provider or browser/network globals**. Rendering providers and an initial
QueryTable does not run effects or open streams. App-owned interactive portals
require a browser DOM; open dialogs/fullscreen are not a package SSR feature.
A direct client can perform REST-only reads in Node with native/injected fetch,
including auth, without React or EventSource. **Default live execution is
browser-only**: missing EventSource produces `INVALID_CONFIGURATION`.
SSR is not a server-side live subscription or a preloaded query snapshot.

Clean packed tests use DOM/network traps, ESM and CommonJS, NodeNext
`.mts`/`.cts` and bundler resolution with `skipLibCheck: false`. Client-only
tests omit peers and verify no React runtime or `@types/react` declaration graph;
hooks do not load presentation/CSS. Marked runnable snippets in this README are
extracted from the installed tarball and type-checked against its real exports.
No source aliases or hidden React copies satisfy these contracts.

## Verified compatibility

Claims are intentionally narrow, not open-ended minimum versions:

| Surface | Exercised configuration |
| --- | --- |
| React / React DOM | **18.3.1**, including real providers, StrictMode, unmount, equivalent/material rerenders and SSR. React 19 is not yet claimed. |
| Node / tooling | **22.20.0** pinned Linux gate; **24.19.0** native development gates. TypeScript **5.9.3** (package) / **5.9.2** (Trading), tsup **8.5.1**, Vite **5.4.19**, Vitest **3.2.7**, committed lockfiles. |
| Browsers | Playwright **1.56.1** Chromium, Firefox and WebKit in the pinned Linux/amd64 image from [Trading TESTING.md](../../examples/trading/TESTING.md). This is not an all-browser/all-version claim. |
| Server | Current development runtime: **0.2.3**, registry library **0.9.2**, index **0.6.3**, engine **0.5.9** from the user-approved temporary source `211d0f2a79aa2ad0f7cb841937f52013fe95ded6` (drasi-project/drasi-core#810), AST **0.3.5**, Cypher/GQL **0.3.6**. Only engine/AST/Cypher are path-selected; equal-version sibling SDKs are not consumed. |
| Plugin / ABI | Current signed SSE **0.3.7**, host/plugin/FFI crates **0.11.2**, native ABI **0.14.0**, from official merged release `70ca432c0f12623ab9b371b2d515180ccc80c2dd`. All six immutable platform/digest/hash/signature locks are inherited from the approved parent. No prior ABI cache fallback or trust relaxation. |
| Historical runtime evidence | Original server **0.2.1** / library **0.8.9** / SSE **0.3.4** / ABI **0.11.0**, and server **0.2.3** / library **0.9.1** / SSE **0.3.6** / ABI **0.13.0** records remain separately versioned. They are not current ABI 0.14 validation or a fallback for it. |

Protocol capabilities, not a guessed version string, determine acceptance.
Missing full-view fields, unsupported language/status/shape, wrong resource
identity or incompatible reaction membership fail with typed errors. No older
server fallback, query-language substitution or newer SDK/ABI upgrade is
attempted. See [engine prerequisites](../../docs/engine-prerequisite.md) and
the [approved main-runtime integration](../../docs/main-runtime-integration.md).
The client does not attest engine/plugin versions; a semantically wrong result
with a valid shape cannot be detected by DTO validation. Operator setup and
real-server provenance/gates supply the version evidence.
`211d0f2a` is unreleased development source: three aggregate and two additive
outbox paths, not stored-data repair. Registry library 0.9.2 still uses compact
records and `append`, not the new trim methods or drasi-project/drasi-core#909's
codec fix. No migration, dropping, broader recovery or publication is implied.

Historical `test/fixtures/server-v1/sse-0.3.4.ndjson` copies all **20 raw lines**
verbatim from Trading's `test/fixtures/recorded/a2b6480-core-0.5.8/server-sse.ndjson`.
Adjacent `contract.json` retains actual REST bodies and separate capture,
source-tag and serializer provenance. The tag
[`drasi-reaction-sse-v0.3.4`](https://github.com/drasi-project/drasi-core/tree/drasi-reaction-sse-v0.3.4)
resolves to **`ff2fde26d0f33adcec17b19db7d2533f80b0baab`**:
`lib/src/channels/events.rs::ResultDiff` defines noop, nullable aggregation-before
and before/after updates; `components/reactions/sse/src/sse.rs` emits
`queryId`/`results`/`timestamp`, **not internal `QueryResult.sequence`**.
The capture does not exercise every source-defined variant; an unused local
SSE 0.3.5 checkout is not evidence for this protocol.

Historical ABI 0.13 evidence remains separate:
`test/fixtures/server-v1-0.2.3/sse-0.3.6.ndjson` preserves all **17 raw CDP
records**; adjacent `sse-0.3.6.provenance.json` retains P4's merge revision
and source/manifest/lock/binary/signature hashes.
Both recordings still exercise the adapter; 0.3.6 also tests default transport
and query-ID routing: `ADD`/`DELETE` data, `UPDATE` before/after/data, lowercase
aggregation before/after **without data**, and unsafe numeric signatures.
Neither captures a shared cursor. These records and Part A's DTO fixture are
unchanged, not relabeled as ABI 0.14 proof. Current SSE 0.3.7 needs its own live
gate; native ABI compatibility alone does not establish wire semantics.

## P5 / #164 Part A migration

Original P5 and ABI 0.13 history remains in [Trading TESTING.md](../../examples/trading/TESTING.md).
The current normal merge adopts #207 at
`6bb523b65ed8ca6a090dcc92d4ff79eb7c12731d`, preserving P5 table/DCE corrections
and P3/P4 auth, protocol, identity, recovery and consistency limits.
No rebase or P6/P7 feature import is involved.

1. Use `/components` `DataTable` for supplied readonly rows, no provider or
   query identity required. Pass application errors through `state`; the error
   generic defaults to ordinary `Error`.
2. Keep `QueryTable` for a live query. It still requires `queryOptions.getKey`
   and `queryOptions.transform`, independently of transformed `rowKey`.
   Do not remove initialization/subscription validation GETs.
3. Remove package `CodeViewerDialog`/`CodeViewerDialogProps` imports and
   `QueryTable.codeSnippet`. QueryTable no longer provides code buttons,
   definition reads, UI links or implicit fullscreen. Compose application
   controls via `headerControls` and own the inspector/overlay. Trading uses
   its local `TradingQueryTable`, `QueryInspector` and `CodeViewerDialog`.
4. Update sort callbacks to accept `SortConfig | null`. Use `sort` for
   controlled state, `defaultSort` for one-time uncontrolled initialization;
   explicit `null` clears to input order. `useTableSort` and `SortConfig` are
   available from `/react` without importing presentation.
5. Accept **readonly** columns in `renderRow`, and preserve the typed row
   callback rather than asserting unknown cells. Custom row output sits in a
   keyed parent fragment. Return content, not table elements, from `renderEmpty`.
6. Replace hard-coded loading/error UI with the state slots as needed. Query
   slots expose the complete P4 result and both retry scopes. For an app-owned
   query composition, reuse `queryTableState` rather than treating every error
   as shared-connection failure.
7. Share one query, sort controller and `useRowAnimation` tracker when rendering
   two views of the same rows. Pass `rowAnimations` to both; it takes precedence
   over their local `animateOnChange`. Mount optional inspectors only on demand
   and unmount them on close. Retry an inspector-local read without restarting
   the live socket; when its error is the provider's same non-null error object,
   use the provider's shared connection retry instead.

Keep the explicit CSS import. Existing CSS and the legacy `height` **class**
contract are unchanged; P6 focus/overlay/theming/reduced-motion/height work is
not completed by this migration. There is no new example app/Storybook (#165),
backend protocol or publication change.

## P4 / #163 Part B migration

Use `/client`, `/react` or `/components` for the dependency boundary you need;
root imports remain supported. Import `/styles.css` explicitly for components.
React peers are now optional for installation, not optional when executing
React APIs, and support is limited to the exercised version.

Connection options still require server, instance, query IDs and an explicit
reaction endpoint. Keep creation definitions in your app; a complete
`QueryConfig` read now includes real server fields and is not a convenient
creation-body type. Trading uses its own `TradingQueryDefinition`.

Part A's guarded `unknown` rows, named types, auth, explicit resource references
and physical import boundaries remain. Equivalent inline data configuration is
safe; callable connection options remain material. Custom stream factories must
honor auth/credentials/cancellation when configured.

For Part B, migrate these breaking result contracts:

1. Supply **both** raw `getKey` and validating `transform` on every hook call.
   For raw reads, still supply the key and `transform: row => row`. Keys receive
   unknown raw fields, not typed transform output; sparse deletes need only
   identity. Action/render callbacks still receive the typed output.
2. Supply required `QueryTable.queryOptions`; `rowKey` remains only its
   transformed render/animation key. Do not move accumulation identity there.
3. Narrow normalized `QueryResult.kind`. Replace `result.data`,
   `result.snapshot` and `result.timestamp` with snapshot `rows` or delta
   `changes` and display-only `receivedAt`/optional `sourceTimestamp`. Preserve
   `before` and `after` on updates, especially key changes.
4. Remove top-level `routeUnidentified`. The default is the strict recorded
   SSE format above (`sse034ResultAdapter`, also exercised with recorded 0.3.6);
   explicitly choose `createLegacyResultAdapter({ routeUnidentified })` for
   legacy formats. `_deleted` is not a normalized-state tombstone.
5. Distinguish query `status`/`stale`/`errorScope` from socket status. Keep useful
   last-good data visible when appropriate. Use the hook/subscription retry for
   a query REST refresh and `useDrasiClient().retry()` for shared recovery.
6. Do not replay overlapping snapshot/delta batches yourself. Respect bounded
   overlap recovery and its terminal exhaustion; stronger handoff guarantees
   depend on a shared backend cursor/resume contract.

## Development and Trading verification

The source/type graphs are physically separated under `src/client`,
`src/react` and `src/components`; `src/types.ts` is only a root compatibility
barrel, not an import used by the client. `npm run build` emits ESM, CJS and
both declaration formats. `npm run typecheck`, `npm test`,
`npm run test:coverage` and `npm run dev` use the existing tooling.
The tarball includes README, CHANGELOG, LICENSE, NOTICE, dist and CSS only.

`test/ResultRegression.test.jsx` is a portable behavioral proof run unchanged
on archived P3 **`a0569c2`** and current P4. All three cases fail on P3 with
observed wrong results and pass on P4:

| Regression | Observed P3 result | P4 result |
| --- | --- | --- |
| Projection omits the raw identity field | `null` instead of the projected row. | Projected value `1`, then removal by a sparse identity-only delete. |
| Official before/after update changes the key | Both old and new identities remain. | Only the new identity with value `2`. |
| Snapshot value `2` overlaps an older pending delta value `1` | Replay rolls the result back to `1`, without refreshing. | Reject the ambiguous candidate and accept value `3` through a bounded REST refresh. |

These tests use the real provider, hooks, client and transport pipeline with
injected REST/EventSource endpoints, not mocks that bypass those product
implementations. They prove these targeted regressions, not stronger handoff
semantics; the undetectable delayed-event limit above still applies.

Vitest coverage includes all `src/**/*.{ts,tsx}` product code. It enforces
**90% statements, lines and functions and 85% branches**, separately for
`src/client/**` and `src/react/**`. These subtree floors do not replace the
existing package/consumer gates or change artifact-size baselines.

Trading continues to consume built local exports. Its unchanged query
definitions, financial transforms, provisioning and tutorial snippets stay
app-owned. [Trading TESTING.md](../../examples/trading/TESTING.md) documents
the locked clean-tarball gate, all three browsers, five original exact PNG images,
coverage/size budgets and authoritative real-server financial/CRUD/reconnect
assertions. Synthetic tests are not proof that a real plugin emits a shape.
Artifact accounting includes all entrypoints/shared chunks, both declaration
formats and all shipped CSS; historical single-format measurements remain
preserved separately. The 2% future-growth policy and coverage floors do not
change with this accounting correction.

Keep `"private": true`. Publishing, repository transfer, credentials/workflows,
new examples/Storybook (#165) and P6 focus/overlay/theming/reduced-motion/height
completion remain separately authorized. P5 covers only #164 Part A composition.
These API/documentation contracts do not constitute an all-checks-passed or
merge-readiness claim.

## License

Apache License 2.0. See LICENSE and NOTICE.
