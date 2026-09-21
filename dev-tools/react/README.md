# @drasi/react

React building blocks for UIs driven by [Drasi](https://drasi.io) Continuous
Queries: one shared SSE connection, query hooks and a sortable, animated,
fullscreen-capable table. The package connects to **existing resources**.
Provisioning, query definitions, transforms, business routing and deployment
defaults belong to the application.

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
| `@drasi/react/react` | Providers, query/status/definition hooks, animation hook and named result/options types. React, but no composed components, tutorial code or CSS. |
| `@drasi/react/components` | `QueryTable`, `CodeViewerDialog`, icons and column/action/sort/props types. React/React DOM, `clsx` and the headless hooks. |
| `@drasi/react` | Deliberate convenience re-exports of all three groups. This is **not** a React-free import. |
| `@drasi/react/styles.css` | Explicit package-owned stylesheet, never imported by the JS modules. |

All four JS entrypoints ship real ESM/CommonJS exports and conditional `.d.ts`
and `.d.cts` declarations. Import these paths, not `src`, `dist` or hashed
shared chunks. The framework-agnostic client is a module of this same package.

React and React DOM are **optional installation peers** so client-only
consumers need not install them. They are required for the React/component/root
entrypoints: install the tested **18.3.1** versions in your application.
No React copy is bundled or hidden in another dependency. The exact peer range
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
or deployment reconciler.

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
cases. `test/fixtures/server-v1-0.2.3/contract.json` separately records the
own rebuilt merged server 0.2.3 / SSE 0.3.6 / ABI 0.13 responses; the same public
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

The default **`sse034ResultAdapter`** accepts the pinned, **untemplated SSE
0.3.4** envelope `{ queryId, results, timestamp }`:

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

All provider/hooks preserve `DrasiError` objects, not just messages.
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

`QueryTable<T>` binds `useDrasiQuery` to the existing sortable, animated table.
Required props are `queryId`, `columns: ColumnDef<T>[]`, `rowKey(row: T)` and
`queryOptions: UseDrasiQueryOptions<T>`. `rowKey` is only the **transformed**
render/animation identity, separate from the raw accumulation `getKey` inside
`queryOptions`; one never substitutes for the other. `ColumnDef`, `RowAction`,
`SortConfig`, `QueryTableProps` and `CodeViewerDialogProps` are
exported by the component entrypoint.

Columns support labels, cell formatting, sortable flags, alignment, cell/header
classes and width. Row actions receive typed rows and support disabled/loading
callbacks, labels, icons and classes. Table props include `defaultSort`,
`animateOnChange`, `actions`, `actionsWidth`, `headerActions`, `title`,
`emptyMessage`, `codeSnippet` and table/header/row class hooks. Sorting,
formatting, animations, fullscreen and code-view behavior are unchanged.
Computed column strings remain supported, so `format`/`className` receive
`(value: unknown, row: T)`. Use the typed row (as in the quickstart) or narrow
the raw value; no unsafe cast is needed. `SortConfig` retains a string column
and `asc | desc` direction, including sorting by non-visible row fields.
On failure the table preserves available last-good rows alongside its existing
error styling. It offers **Retry query** for query-scoped failures and **Retry
connection** for shared failures. Reconnecting/resynchronizing stale-data
messages appear only on the exceptional recovery path; healthy table
presentation and package CSS are unchanged.
Pure presentation/composition, controlled sort, slots, dialog accessibility
and theming redesign are separate #164 work, not claims of this release.

CSS is namespaced and package-owned. `--drasi-*` variables and existing class
props customize appearance; portals and advanced overlay semantics have not
been redesigned. Consumers do not need Tailwind.

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
Server rendering providers and the initial, closed component state does not run
effects or open streams. Interactive portals require a browser DOM; open
dialogs/fullscreen are not a server-rendering feature.
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
| Server | Current approved runtime: **0.2.3**, registry library **0.9.1**, index **0.6.1**, the same reviewed engine **0.5.8** source at `1284e9f648634c1faa73fd897a21c2712bb0cbbe`, AST **0.3.5**, Cypher/GQL **0.3.6**. The v1 query read DTO is unchanged; archive/memory settings remain server/instance-owned. |
| Plugin / ABI | Current signed SSE **0.3.6**, host/FFI crates **0.11.0**, plugin SDK crate **0.11.1**, native ABI **0.13.0**, from official merged release `3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`. Immutable platform/digest/hash/signature pins come from the approved parent integration. |
| Historical runtime evidence | Server **0.2.1**, library **0.8.9**, SSE **0.3.4**, SDK **0.10.0** / ABI **0.11.0** captures remain versioned separately. They are not proof for every 0.2.1 build or a fallback for current ABI 0.13 startup. |

Protocol capabilities, not a guessed version string, determine acceptance.
Missing full-view fields, unsupported language/status/shape, wrong resource
identity or incompatible reaction membership fail with typed errors. No older
server fallback, query-language substitution or newer SDK/ABI upgrade is
attempted. See [engine prerequisites](../../docs/engine-prerequisite.md) and
the [approved main-runtime integration](../../docs/main-runtime-integration.md).
The client does not attest engine/plugin versions; a semantically wrong result
with a valid shape cannot be detected by DTO validation. Operator setup and
real-server provenance/gates supply the version evidence.

The protocol evidence is the real capture at
`examples/trading/app/test/fixtures/recorded/a2b6480-core-0.5.8/server-sse.ndjson`
and the unmodified REST bodies at `test/fixtures/server-v1/contract.json`.
The complete **20 original recording lines** are copied verbatim into
`test/fixtures/server-v1/sse-0.3.4.ndjson` so isolated package tests use the same
pinned SSE evidence. `contract.json` records its exact source tag, revision and
serializer provenance separately from the later REST capture.
The upstream tag
[`drasi-reaction-sse-v0.3.4`](https://github.com/drasi-project/drasi-core/tree/drasi-reaction-sse-v0.3.4)
resolves to **`ff2fde26d0f33adcec17b19db7d2533f80b0baab`**. Its
`lib/src/channels/events.rs::ResultDiff` serde contract establishes noop,
nullable aggregation-before and before/after updates; its
`components/reactions/sse/src/sse.rs` serializer emits
`queryId`/`results`/`timestamp` but **does not transmit internal
`QueryResult.sequence`**. The capture and exact tagged source have distinct
roles: not every documented variant occurred in that capture. An unused local
SSE 0.3.5 checkout is not evidence for this pinned protocol.

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
4. Remove top-level `routeUnidentified`. The default is strict SSE 0.3.4;
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
new examples/Storybook and the pure UI/composition/accessibility work in #164
remain separately authorized. These API/documentation contracts do not
constitute an all-checks-passed or merge-readiness claim.

## License

Apache License 2.0. See LICENSE and NOTICE.
