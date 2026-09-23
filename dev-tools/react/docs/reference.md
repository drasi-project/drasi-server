# API and recipe reference

[Package overview](../README.md)

Find [connection options](#connection-options), [query hooks and raw identity](#hooks-raw-identity-and-derived-views),
[DataTable](#provider-free-datatable), [QueryTable](#live-querytable-and-scoped-state-slots),
[sorting](#sorting), [Modal](#modal), [clients](#low-level-clients), or
[errors and retry](#errors-and-recovery). This is a lookup reference;
start with the [first table](../README.md#your-first-table) or
[live quickstart](getting-started.md#quickstart).

## API reference

### Public symbol map

This is the complete named export surface; the sections below specify
the contracts. Root re-exports the same symbols. It is not an additional client
implementation or a headless dependency shortcut.

| Entrypoint / group | Exported symbols |
| --- | --- |
| `/client` runtime | `DrasiClient`, `DrasiSSEClient`, `DrasiError`, `sse034ResultAdapter`, `createLegacyResultAdapter`, `accumulateResult` |
| `/client` connection and transport types | `DrasiClientOptions`, `DrasiSSEClientOptions`, `ReactionReference`, `ReconnectOptions`, `ResultReconciliationOptions`, `ConnectionStatus`, `DrasiRequestContext`, `DrasiHeaders`, `DrasiHeadersProvider`, `EventSourceOptions`, `EventSourceFactory`, `EventSourceLike` |
| `/client` read and error types | `Component`, `ComponentLinks`, `ComponentStatus`, `QueryLanguage`, `QueryConfig`, `QuerySource`, `QueryJoin`, `QueryJoinKey`, `QueryMiddleware`, `ReactionConfig`, `SseReactionConfig`, `JsonValue`, `DrasiErrorCode`, `DrasiErrorDetails`, `DrasiResourceKind` |
| `/client` result and subscription types | `ResultRow`, `RowKey`, `ResultChange`, `QuerySnapshot`, `QueryDelta`, `QueryResult`, `ResultAdapter`, `ResultAdapterContext`, `LegacyResultAdapterOptions`, `RouteUnidentified`, `QueryStatus`, `QueryErrorScope`, `QuerySubscription`, `QuerySubscriptionState` |
| `/react` runtime | `DrasiProvider`, `DrasiClientProvider`, `useDrasiClient`, `useDrasiQuery`, `useDrasiConnectionStatus`, `useDrasiQueryDefinition`, `useDrasiServerUiUrl`, `useRowAnimation`, `useTableSort`, `useReducedMotion` |
| `/react` types | `DrasiProviderProps`, `DrasiContextValue`, `UseDrasiQueryOptions`, `UseDrasiQueryResult`, `UseDrasiQueryDefinitionResult`, `AnimationDirection`, `UseRowAnimationOptions`, `UseRowAnimationResult`, `SortConfig`, `UseTableSortOptions`, `UseTableSortResult` |
| `/components` runtime | `DataTable`, `QueryTable`, `queryTableState`, `Modal`, `CodeIcon`, `ExpandIcon`, `CollapseIcon` |
| `/components` types | `DataTableProps`, `DataTableState`, `DataTableRenderContext`, `DataTableErrorContext`, `DataTableHeaderContext`, `QueryTableProps`, `QueryTableRenderContext`, `QueryTableErrorContext`, `ColumnDef`, `RowAction`, `SortConfig`, `ModalProps`, `TableHeight` |

`SortConfig` is the same type through `/react` and `/components`. Client types
such as `ConnectionStatus` and `QueryStatus` should be imported from `/client`,
not assumed to be re-exported by `/react`. Private module helpers and icon
implementation prop aliases are not additional named exports.

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
| `requestTimeoutMs?: number` | 10000 ms; finite and positive, **per REST request**, including auth/body. Composite reads can make multiple requests. |
| `reconnect?: ReconnectOptions` | Defaults below; same count/backoff for streams and snapshots. |

| `ReconnectOptions` field | Default / allowed value |
| --- | --- |
| `maxReconnectAttempts?: number` | **10 retries after the first attempt**; nonnegative integer. `0` disables automatic retries, not the first attempt. |
| `initialReconnectDelayMs?: number` | **1000** ms; finite, positive initial delay. |
| `maxReconnectDelayMs?: number` | **30000** ms; finite, positive backoff cap. |
| `connectionTimeoutMs?: number` | **10000** ms; finite, positive stream-open timeout. |

Backoff is exponential; counts reset on a
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
`createLegacyResultAdapter(options?: LegacyResultAdapterOptions): ResultAdapter`
defaults to `{}`: without `routeUnidentified`, nonempty unidentified rows fail
rather than acquiring a guessed query. `LegacyResultAdapterOptions` accepts
that optional app-owned `RouteUnidentified` callback. This adapter accepts
`query_id`, `results` entries with `op`
`c`/`r`/`u`/`d` or lowercase operation `type`, keyed `data`/`_deleted` rows,
and `addedResults`/`updatedResults`/`deletedResults` arrays with before/after
wrappers or direct rows. It is a migration adapter, not another claim about
the official plugin protocol. An explicit query ID is always honored before
content routing; the callback cannot override it.

`RouteUnidentified` is
`(rows: ResultRow[], deliver: (queryId: string, rows: ResultRow[]) => void) => void`.
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
field. `accumulateResult(rows: readonly ResultRow[], result: QueryResult,
getKey: RowKey, details?: DrasiErrorDetails): ResultRow[]` is the exported,
framework-independent raw reducer; `details` defaults to `{}`. Snapshots
replace raw state, same-key
upserts replace idempotently, deletes remove their `before` key, and updates
remove a changed `before` key before storing `after`. Equal-valued rows with
different domain keys remain distinct. There is no serialized-value deduplication.
It returns a new array without changing the input array/rows; key failures
throw before a partial result is returned. It does not transform rows,
subscribe, validate arbitrary wire payloads or reconcile snapshot overlap.
Use the client/adapter boundary for wire validation before this raw reducer.

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
| `useReducedMotion()` | Live boolean for `prefers-reduced-motion: reduce`; `false` during SSR or without `matchMedia`. Provider-free; see [reduced motion](#reduced-motion). |

The five Drasi hooks require `DrasiProvider` or a `DrasiClientProvider` binding;
calling them outside either throws an ordinary provider-usage `Error`.
Sorting, animation and motion hooks do not require a provider.

`DrasiContextValue` has four required fields: `client: DrasiClient | null`,
`initialized: boolean`, `error: DrasiError | null`, and `retry: () => void`.
The initial provider state has no initialized client; invalid configuration may
leave `client` null. `error` describes shared initialization/connection failure,
not an individual query's projection failure. The controlled binding's `value`
must supply all four fields; its React children are application-owned.

`ConnectionStatus` starts with `connected: false`. `connected: boolean` means
the stream is open; optional `reconnecting: boolean` means the shared
connection is recovering, and optional `error: DrasiError` carries its failure.
Optional `lastConnected: Date` is the local time of a successful open, not a
server cursor; it need not remain present in a later disconnected status.

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

This deterministic reducer example projects away the raw `deviceId` field and
then removes that row using a **sparse identity-only delete**. The transform
requires a value on current rows, not on delete records. For a table, use
`queryOptions={readingViewOptions}` and the separate
`rowKey={readingViewKey}`; a render key cannot repair a missing raw key.

```ts
// @drasi-docs: raw-identity.ts
import {
  accumulateResult, type QueryResult, type ResultRow, type RowKey,
} from '@drasi/react/client';
import type { UseDrasiQueryOptions } from '@drasi/react/react';

interface ReadingView { renderId: string; value: number }
const rawKey: RowKey = raw => {
  if (typeof raw.deviceId !== 'string' || !raw.deviceId) {
    throw new Error('Expected a stable device ID');
  }
  return raw.deviceId;
};

export const readingViewOptions: UseDrasiQueryOptions<ReadingView> = {
  getKey: rawKey,
  transform: raw => {
    const deviceId = rawKey(raw);
    if (typeof raw.value !== 'number' || !Number.isFinite(raw.value)) {
      throw new Error('Expected a finite reading');
    }
    return { renderId: `sensor:${deviceId}`, value: raw.value };
  },
};
export const readingViewKey = (row: ReadingView) => row.renderId;

function project(rows: readonly ResultRow[]): ReadingView[] {
  return rows.flatMap(raw => {
    const view = readingViewOptions.transform(raw);
    return view === null ? [] : [view];
  });
}

export function sparseDeleteExample() {
  const snapshot: QueryResult = {
    kind: 'snapshot', queryId: 'readings', receivedAt: 0,
    rows: [{ deviceId: 'freezer-1', value: -18 }],
  };
  const deletion: QueryResult = {
    kind: 'delta', queryId: 'readings', receivedAt: 1,
    changes: [{ kind: 'delete', before: { deviceId: 'freezer-1' } }],
  };
  const rawRows = accumulateResult([], snapshot, rawKey);
  const beforeDelete = project(rawRows);
  const afterDelete = project(accumulateResult(rawRows, deletion, rawKey));
  return { beforeDelete, afterDelete };
}
```

`beforeDelete` contains `{ renderId: 'sensor:freezer-1', value: -18 }`;
`afterDelete` is empty. Neither the raw reducer nor the hook transforms the
delete's `before` row. The example's receipt values are display metadata, not
the reason the delete applies.

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
and `stale`. Specifically, `loading` is true only with no data, no error and
no terminal state; `stale` is true when retained data exists outside `live` or
`empty`. `data: null` means no usable view yet, while `[]` is an accepted empty
view. `lastUpdate: Date | null` records the last accepted result's local receipt
time. Query-local `retry: () => void` returns immediately; watch the state for
its result, not a returned promise.

`useDrasiQueryDefinition` is a cancellable, optional read, not a result
subscription. Its `config: QueryConfig | null`, `loading: boolean` and
`error: DrasiError | null` describe that read; it exposes **no retry field**.
Mount only when inspection is needed and unmount to cancel. Rekey/remount the
reader for a local retry; use the shared retry if the provider itself failed.
`useDrasiServerUiUrl(): string | null` builds a link, without probing that UI.

Per-query processing and subscription REST failures are isolated
while the shared transport remains healthy. This is **not whole-connection
fault isolation**: initial validation and shared reconnection revalidate
**all configured query references** and the reaction, as in Part A. A reference
failure during that shared validation can block the shared connection and
affect every subscription. Malformed unidentifiable/protocol failures can also
terminate the shared stream.

### Subscriber state and computation cost

One provider multiplexes **one SSE connection**, not one shared query cache.
Each `useDrasiQuery` call independently fetches its baseline, reconciles
overlap/recovery, accumulates raw rows and derives its view. Two hooks or two
`QueryTable`s with the same query ID still mean two subscriptions with their
own REST reads, state, memory and projection work.

Accepted updates rekey/project the retained set; a small delta does not imply
that only one row is transformed or rendered. Sparse delete payloads are
handled by raw identity, never passed to `transform`; remaining current rows
can still be reprojected. A `null` transform hides a row but retains its raw
identity/data. `postProcess` is a derived view, not a raw-state eviction policy:
filtering or slicing visible rows does not bound retained memory.

Keep `getKey`, `transform` and `postProcess` pure. Keep their references stable
when their semantics have not changed, with correct closure dependencies;
do not hide changing semantics behind stale callbacks. Committed callback
changes reproject retained rows without resubscription, while abandoned renders
cannot replace the active ingestion key. The raw `getKey` and transformed
table `rowKey` remain separate contracts.

For shared data, [hoist one query hook](#app-owned-composition) and pass its
result to multiple presentations rather than mounting extra `QueryTable`s.
Share both animation direction and revision maps when using `useRowAnimation`.
This reduces duplicate subscriber work without introducing a cache API.

For larger or faster feeds, use supported operator-owned query features
such as `WHERE`, selective `RETURN` projections and supported aggregations
(for example `sum`/`count`) to reduce upstream result volume where appropriate.
Measure representative snapshot row counts, visible versus filtered rows,
update burst size/rate, memory, snapshot/retry frequency, and projection,
React render and layout time on your actual target devices. Browser performance
tools and React profiling can separate transformation cost from rendering;
avoid logging row contents or credentials while measuring.

There is no built-in virtualization or large/high-rate throughput guarantee.
`maxPendingChanges` bounds observed overlap during a pending snapshot, **not**
result-set rows or memory for accepted data. These choices do not strengthen
the [best-effort snapshot/live contract](#snapshotstream-consistency-and-limits):
there is no shared REST/SSE cursor, and delayed older events can still require
an explicit refresh. Measure against your application's latency/memory needs,
not an invented universal row limit or benchmark.

#### Hooks-only UI and scoped retry

This is a live query consumer, not a presentation mock. It uses the same
`readings` / `events` resources as the quickstart, but only `/react` imports
from this package and native semantic markup: no `QueryTable`, `DataTable`,
`Modal` or package stylesheet. The app owns layout, formatting and notices.
It distinguishes a query-local refresh (also useful for the delayed-event
limit below) from shared connection recovery, and keeps available stale rows.

```tsx
// @drasi-docs: hooks-only.tsx
import {
  DrasiProvider, useDrasiClient, useDrasiConnectionStatus, useDrasiQuery,
  type UseDrasiQueryOptions,
} from '@drasi/react/react';

interface Reading { id: string; value: number }
const options: UseDrasiQueryOptions<Reading> = {
  getKey: raw => {
    if (typeof raw.id !== 'string' || !raw.id) throw new Error('Expected a stable reading ID');
    return raw.id;
  },
  transform: raw => {
    if (typeof raw.id !== 'string' || typeof raw.value !== 'number' || !Number.isFinite(raw.value)) {
      throw new Error('Expected a Reading');
    }
    return { id: raw.id, value: raw.value };
  },
};

function ReadingList() {
  const query = useDrasiQuery('readings', options);
  const connection = useDrasiConnectionStatus();
  const { initialized, retry: retryConnection } = useDrasiClient();
  const sharedFailure = query.errorScope === 'connection';
  return <section aria-label="Reading monitor">
    <h2>Reading monitor</h2>
    <p role="status">
      Stream: {connection.connected ? 'open' : 'not open'}. Query: {query.status}.
    </p>
    {query.error && <div role="alert">
      <p>{query.error.code}: {query.error.message}</p>
      <button type="button" onClick={sharedFailure ? retryConnection : query.retry}>
        {sharedFailure ? 'Retry connection' : 'Retry query'}
      </button>
    </div>}
    {query.loading && <p role="status">Waiting for a baseline.</p>}
    {query.stale && <p role="status">Showing last known readings while recovery is needed.</p>}
    <button
      type="button" onClick={query.retry}
      disabled={!initialized || !connection.connected}
    >Refresh query</button>
    {query.lastUpdate && <p>
      Last received: <time dateTime={query.lastUpdate.toISOString()}>
        {query.lastUpdate.toISOString()}
      </time>
    </p>}
    {query.data !== null && <table>
      <caption>Readings</caption>
      <thead><tr><th scope="col">Sensor</th><th scope="col">Value</th></tr></thead>
      <tbody>{query.data.length === 0
        ? <tr><td colSpan={2}>No readings in this view.</td></tr>
        : query.data.map(row => <tr key={row.id}>
          <th scope="row">{row.id}</th><td>{row.value.toFixed(2)}</td>
        </tr>)}</tbody>
    </table>}
  </section>;
}

export function HooksOnlyApp({ serverUrl, instanceId, endpoint }: {
  serverUrl: string; instanceId: string; endpoint: string;
}) {
  return <DrasiProvider
    serverUrl={serverUrl} instanceId={instanceId}
    queryIds={['readings']} reaction={{ id: 'events', endpoint }}
  >
    <ReadingList />
  </DrasiProvider>;
}
```

Do not refresh the whole provider after every query failure. Conversely,
query-local retry cannot open a failed shared socket. Custom live cell
announcement policy, styling and keyboard scrolling remain application work;
the headless hooks do not silently add them.

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

See the [first provider-free table](../README.md#your-first-table) for a complete copyable example.

All presentation props are optional except the first three:

| `DataTableProps<T, E>` prop | Default / callback contract |
| --- | --- |
| `rows: readonly T[] \| null` | Required. `null` means no accepted baseline; `[]` is an accepted empty view. Supplied arrays are never sorted in place or otherwise mutated. |
| `columns: readonly ColumnDef<T>[]` | Required ordered column definitions, never mutated. |
| `rowKey: (row: T) => string` | Required stable render/animation identity. Use domain keys, not array positions. This is not raw query accumulation identity. |
| `state?: DataTableState<E>` | `{}`; presentation-only loading/error/stale/retry state, described below. |
| `sort`, `defaultSort`, `onSortChange` | Same contract as `useTableSort`; initially unsorted unless a sort is provided. See [sorting](#sorting). |
| `title?: string` | No title by default. |
| `ariaLabel?: string` | Accessible name on `<table>`; defaults to `title`. Supply a meaningful name when there is no visible title. |
| `headerActions?: ReactNode` | Content beside the title on the left. |
| `headerControls?: ReactNode` | Content on the right; no built-in code-view/fullscreen controls. |
| `headerSlot?: ReactNode` | Content below the header, above the table. |
| `renderHeader?: (context: DataTableHeaderContext<T, E>) => ReactNode` | Replaces the header. Call `context.defaultRender()` to wrap/augment the default title/actions/controls; return `null` to omit it. |
| `actions?: readonly RowAction<T>[]` | No actions by default. Nonempty actions add one trailing column. |
| `actionsWidth?: string` | Optional CSS width of the actions heading. |
| `animateOnChange?: keyof T` | Disabled by default. Track the named row property; numbers animate up/down and changed strings animate `change`. Reduced motion suppresses these animations. |
| `rowAnimations?: ReadonlyMap<string, AnimationDirection>` | Optional owner-supplied animation state keyed by `rowKey`; takes precedence over `animateOnChange`, including an empty map. Reduced motion also suppresses this controlled map's presentation. |
| `rowAnimationRevisions?: ReadonlyMap<string, number>` | Optional restart tokens for controlled `rowAnimations`. Pass `useRowAnimation().revisions` to restart repeated same-direction decoration. Changed tokens are compared with `Object.is`, including skipped/batched revisions; they never replace `rowKey`. Without tokens, controlled decoration restarts only when its direction changes or is cleared and reapplied. |
| `renderRow?: (row, columns, animation, defaultRender) => ReactNode` | Replaces one row's rendering. `row` is `T`, `columns` is readonly, and `animation` is `'up' \| 'down' \| 'change' \| null`. Return a `<tr>` or a fragment of table rows, or call `defaultRender()` for the built-in `<tr>`. A parent fragment already owns the stable `rowKey`; wrappers need not invent an index key. |
| `emptyMessage?: string` | `"No data available"`; `renderEmpty` takes precedence. |
| `renderLoading`, `renderEmpty`, `renderError`, `renderStale` | Optional state slots; details below. A slot returning `null` suppresses its default rather than falling back. |
| `className?: string`, `style?: CSSProperties` | Additional card class and inline styles. |
| `containerRef?: Ref<HTMLDivElement>` | Ref to the card for app-owned layout/composition. |
| `tableClassName?: string` | Additional class on the named, focusable **scroll-container** region, not on `<table>`. Preserve scrolling and focus visibility when overriding it. |
| `headerClassName?: string` | Additional `<thead>` class; individual headings use `ColumnDef.headerClassName`. |
| `titleClassName?: string` | Additional normal-header title class. |
| `headerSlotClassName?: string` | Additional wrapper class for `headerSlot`. |
| `rowClassName?: string \| ((row: T, index: number) => string)` | Additional class on the default row; `index` is its position in the sorted view. |
| `height?: TableHeight` | Finite nonnegative pixel number, explicit CSS length/percentage, `'0'`, `'auto'` or custom-property reference. Overrides `style.height`. Omitted: respect `style.height`, then inherited `--drasi-table-height`, then `400px`. See [table sizing](#table-sizing). |

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
Loading sets `aria-busy="true"` and shows the spinner instead of the icon.
The action column has a visually hidden **Actions** heading. Optional `className`
and `hoverClassName` customize the button; the hover class applies only when
enabled and not loading.

The owner below supplies data, pending-action state and recovery; no Drasi
provider or resource mutation is involved. Dispatch is an application
callback, not a package API. State slots preserve their roles and last-good
notice, the custom header keeps the default controls, and `renderEmpty`
returns **cell content**.

```tsx
// @drasi-docs: actions-slots.tsx
import {
  DataTable, type ColumnDef, type DataTableState, type RowAction,
} from '@drasi/react/components';
import '@drasi/react/styles.css';

interface Delivery { id: string; parcels: number; locked: boolean }
const columns: readonly ColumnDef<Delivery>[] = [
  { key: 'id', label: 'Delivery' },
  { key: 'parcels', label: 'Parcels', align: 'right' },
];

export function DeliveryActions({ rows, state = {}, pendingId, onDispatch }: {
  rows: readonly Delivery[] | null;
  state?: DataTableState;
  pendingId?: string;
  onDispatch: (row: Delivery) => void;
}) {
  const actions: readonly RowAction<Delivery>[] = [{
    icon: <span aria-hidden="true">↗</span>,
    label: 'Dispatch delivery',
    onClick: onDispatch,
    disabled: row => row.locked,
    loading: row => row.id === pendingId,
  }];
  return <DataTable<Delivery>
    rows={rows} columns={columns} rowKey={row => row.id} title="Dispatch queue"
    state={state} actions={actions} actionsWidth="6rem"
    headerSlot={<p>Locked deliveries cannot be dispatched.</p>}
    renderHeader={({ defaultRender, setSort }) => <>
      {defaultRender()}
      <button type="button" onClick={() => setSort(null)}>Input order</button>
    </>}
    renderLoading={() => <p role="status">Loading deliveries.</p>}
    renderEmpty={() => <span>No deliveries in this view.</span>}
    renderError={({ error, state: current }) => <div role="alert">
      <p>{error.message}</p>
      {current.stale && <p>Showing last known deliveries.</p>}
      {current.retry && <button type="button" onClick={current.retry}>
        {current.retryLabel ?? 'Retry'}
      </button>}
    </div>}
    renderStale={({ rows: visible, state: current }) => <p role="status">
      {current.staleMessage ?? 'Showing last known deliveries.'}
      {' '}{visible?.length ?? 0} deliveries.
    </p>}
  />;
}
```

The owner catches asynchronous action failures and updates `pendingId` and
`state`; `onClick` is a void callback, not an awaited mutation or automatic
loading controller. Supplying `state.retry` chooses application recovery.
For live query state use the [scoped QueryTable slots](#live-querytable-and-scoped-state-slots)
or `queryTableState`, rather than treating every error as shared recovery.

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

Each sortable column has a native `<button type="button">` inside
`<th scope="col">`. Its accessible name is the column label; decorative sort
icons are hidden from assistive technology. The active header, not its button,
has `aria-sort="ascending"` or `"descending"`; unsorted headers omit it.
The `<th>` itself is not interactive or separately focusable. Tab reaches the
button, and native Enter/Space activation makes one sort request per action.
`sortable: false` renders plain heading text with no sort button.

Every rendered table also has a native keyboard scroll stop: its viewport is
`role="region"` with `tabIndex={0}`, named
`${ariaLabel ?? title ?? 'Data'} table viewport`. Tab can focus the viewport;
native Arrow/PageUp/PageDown scrolling works when its content overflows,
without requiring a sortable column or row-action button. This does not turn
rows/cells into interactive controls or add custom key handlers. Sorting and
enabled action buttons keep their own native focus stops. The viewport's
focus-visible outline uses `--drasi-color-focus`.

Default loading/stale notices use `role="status"` and default errors use
`role="alert"`. Individual live cell changes are **not** automatically announced
as a live region. Applications own any additional announcement policy and the
semantics/focus behavior of replacement headers, rows and state slots.

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

This component belongs beneath the [quickstart provider](getting-started.md#quickstart). The error slot below
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
`useRowAnimation<T>(options: UseRowAnimationOptions<T>): UseRowAnimationResult<T>`
is provider-free and tracks application rows, not raw query identities:

| Animation option/result | Contract |
| --- | --- |
| `rowKey: (row: T) => string` | Required stable, unique render/animation identity; must match the consuming table's `rowKey`. |
| `getValue: (row: T) => number \| string \| undefined` | Required tracked value; `undefined` supplies no comparable baseline for that row. This is not a query transform. |
| `animationDuration?: number` | **500** ms until decoration state expires after the latest change. This does not change the stylesheet's duration; supply a suitable finite nonnegative duration for your own tracker. |
| `data?: readonly T[] \| null` | Optional automatic updates after rendering. Null/undefined retains the previous baseline; an empty array clears tracking and timers. |
| `animations: Map<string, AnimationDirection>` | Current animation entries, initially empty; treat this returned state as read-only. Supply it as `rowAnimations` to share it across presentations. |
| `revisions: ReadonlyMap<string, number>` | Opaque restart tokens, including repeated changes in the same direction. Share as `rowAnimationRevisions`; not row keys or persistent counters. |
| `updateData: (data: readonly T[]) => void` | Manual tracking update for owners that do not supply `data`. It updates animation state, not query rows. |
| `AnimationDirection` | `'up' \| 'down' \| 'change' \| null`. Numeric increases/decreases select up/down; other unequal defined string/number pairs select change. Null or an absent map entry means no animation. |

The first observed value does not animate. Later changes reset that row's
timer; removed rows lose their entries/timers, and unmount clears timers.
Reduced motion cancels active entries while retaining the latest baseline.
The hook returns state, not markup or CSS; only a consuming presentation maps
directions to classes. See [reduced motion](#reduced-motion) for live preference
changes. Share both maps to avoid each presentation tracking its own changes.

`animations` retains the direction map; `revisions: ReadonlyMap<string, number>`
adds opaque per-row restart tokens for relevant changes, even in the same
direction. Unchanged tracked values preserve both maps.
Tokens are removed with their decoration on expiry, row removal, empty input or
reduced motion. A single per-hook scalar in React state survives that cleanup;
there is no history of deleted rows, and token values are not persistent counters
or React keys. Expiry and reactivation coalesced into one React batch therefore
still have different decoration identities. The renderer retains its last active
phase across inactive commits, even when the browser has not painted between
them, while preserving the first activation's original phase. The lifetime
restarts after the latest change; updates stopping means the decoration expires,
not an endless animation. The built-in stylesheet retains its 500ms
ease-in-out pulse and original theme colors, alternating equivalent keyframes
without replacing DOM rows, losing cell state/focus or forcing layout. The hook's
custom `animationDuration` controls state expiry, not the stylesheet duration.

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
  const { animations, revisions } = useRowAnimation({ data: query.data, rowKey, getValue });
  const presentation = {
    rows: query.data, columns, rowKey,
    state: queryTableState(query, retryConnection),
    sort: sorting.sort, onSortChange: sorting.setSort,
    rowAnimations: animations, rowAnimationRevisions: revisions,
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
each accepts only optional `className?: string`, defaulting to `'drasi-icon'`.
Their SVGs are decorative (`aria-hidden`, not focusable); name the containing
button. There is no exported `IconProps` type, click handler, inspection or
fullscreen behavior attached to an icon. CSS remains namespaced,
package-owned and an **explicit** import; consumers do not need Tailwind.
P5 left CSS byte-for-byte unchanged. P6 adds the generic modal, scoped theme
fallbacks, keyboard semantics, reduced motion and explicit sizing described
below; Trading's code-viewer CSS now belongs to the app.
No pagination, selection or virtualization is provided.

#### Table sizing

`TableHeight` is exported by `/components` and root. Accepted values are:

- A **finite, nonnegative number**, in pixels, including `0`.
- A nonnegative explicit length/percentage string using `px`, `rem`, `em`,
  `ch`, `ex`, `lh`, `rlh`, `vw`, `vh`, `vmin`, `vmax`, `svw`, `svh`, `lvw`,
  `lvh`, `dvw`, `dvh`, `cm`, `mm`, `in`, `pt`, `pc` or `%`.
- Unitless `'0'`, or `'auto'`.
- `var(--name)`, optionally with one literal nonnegative length, `'0'` or
  `'auto'` fallback, for example `var(--drasi-panel-height, 20rem)`.

Percentages require a sized parent. Use a custom property for `calc()` or
`clamp()` expressions; they are not direct `height` prop values. The property
name must start with `--` followed by a letter or underscore, then letters,
digits, underscores or hyphens. Invalid runtime values throw `TypeError`,
including negative/nonfinite numbers, unsupported expressions and class names.
TypeScript rejects arbitrary/class strings, but its number/template types
cannot express every runtime constraint; validate untrusted JavaScript input.

Validation checks prop syntax and numeric bounds, not the computed CSS
cascade. Define referenced custom properties with usable size values or supply
a literal fallback, such as `var(--drasi-panel-height, 20rem)`. SSR cannot
evaluate inherited custom properties. The omitted prop's 400px default is not
an implicit fallback for an explicit unresolved `var(--name)`.

An explicit `height` wins over `style.height`. Without it, inline
`style.height` wins over the inherited `--drasi-table-height` token, whose
stylesheet fallback is **400px**. This applies to DataTable state cards and
QueryTable as well as tables with rows. Class-based layout belongs in
`className`, not `height`.
CSS border/padding minima still apply; `0` is not a visibility or collapse
flag. Provide enough space for headers and controls in the chosen layout.

```tsx
// @drasi-docs: table-sizing.tsx
import type { CSSProperties } from 'react';
import { DataTable, type ColumnDef, type TableHeight } from '@drasi/react/components';
import '@drasi/react/styles.css';

interface Job { id: string; priority: number }
const rows: readonly Job[] = [{ id: 'dispatch', priority: 1 }];
const columns: readonly ColumnDef<Job>[] = [
  { key: 'id', label: 'Job' }, { key: 'priority', label: 'Priority' },
];
const responsiveHeight: TableHeight = 'var(--drasi-panel-height, 20rem)';
const layout: CSSProperties & { '--drasi-panel-height': string } = {
  '--drasi-panel-height': 'clamp(18rem, 50vh, 36rem)',
};

export function SizedJobs() {
  return <section style={layout}>
    <DataTable<Job>
      rows={rows} columns={columns} rowKey={row => row.id}
      title="Fixed-size jobs" height={400}
    />
    <DataTable<Job>
      rows={rows} columns={columns} rowKey={row => row.id}
      ariaLabel="Responsive jobs" height={responsiveHeight}
    />
  </section>;
}
```

#### Modal

`Modal` is a controlled, provider-free dialog primitive exported by
`/components` and root. Import `styles.css`; no Drasi connection, table,
Tailwind setup or domain-specific header/footer is required.

| `ModalProps` prop | Default / contract |
| --- | --- |
| `open: boolean` | Required controlled visibility. The owner closes by setting `false` or unmounting. |
| `onClose: () => void` | Required close request for a topmost Escape/outside dismissal. No event or boolean argument. Update `open`; ignoring the request leaves the modal open. Programmatic prop changes do not request another close. |
| `title: string` | Required meaningful, nonempty accessible name, rendered as a visually hidden title connected by `aria-labelledby`. Empty/whitespace or non-string runtime values throw `TypeError`, even when closed. Render a visible heading in children when appropriate. |
| `description?: string` | Optional short, visually hidden description connected by `aria-describedby`. Omit for complex structured content rather than flattening it into one announcement. |
| `children: ReactNode` | Required app-owned content. Include a **visible, keyboard-operable close/cancel control** wired to the same owner. No close button is inserted for you. |
| `initialFocusRef?: RefObject<HTMLElement>` | Valid focusable descendant to focus on opening; otherwise the dialog content itself receives focus. A target outside the content is not used. |
| `returnFocusRef?: RefObject<HTMLElement>` | Preferred focus target after closing, subject to validity and any surviving top modal. |
| `fallbackFocusRef?: RefObject<HTMLElement>` | App-owned fallback when the explicit return target and focused opening control are unavailable. |
| `themeRef?: RefObject<HTMLElement>` | Explicit theme source; defaults to a hidden inline anchor at the modal's React position. Does not change the portal destination. |
| `className?: string` | Replaces the optional default `drasi-modal-surface` decoration. Structural `drasi-modal-content` remains. |
| `style?: CSSProperties` | Inline content styles. No default inline content override. |
| `overlayClassName?: string` | Replaces the default centered/dimmed `drasi-modal-overlay` decoration. Structural `drasi-modal-layer` remains. |
| `overlayStyle?: CSSProperties` | Inline overlay styles, applied after the copied theme. |
| `closeOnEscape?: boolean` | `true`; Escape requests closing only the top layer. Setting `false` does not dismiss a lower modal instead. |
| `closeOnOutsideClick?: boolean` | `true`; a primary pointer press outside the content requests closing. The pointer's default focus movement is prevented even when dismissal is disabled, retaining focus while a controlled close is pending or declined. Background controls remain shielded. |

SSR and initial hydration render exactly one hidden inline theme anchor,
preserved when the client layer loads. The public wrapper imports only React;
the primitive and portal-theme layout effects stay in the client-only layer.
After browser mount, that layer loads lazily; mounting a closed Modal also
warms it. Only a ready, open layer acquires focus/scroll ownership. Closing or
unmounting during loading cannot open a late overlay. Loading failures
propagate to the owner's React error boundary rather than silently acting like
a working dialog.

The body portal has `role="dialog"` and `aria-modal="true"`. Pinned Radix
Dialog **1.1.15** owns focus containment, the Tab/Shift+Tab loop, topmost
dismissal, outside-pointer shielding, background `aria-hidden` management and
reference-counted scroll locking across independent/nested instances. Do not
add a competing document-level Escape handler, body-overflow toggle or global
focus manager around it.

Focus visibility is a separate, bounded scroll operation. When a non-root
focused target is physically inside the dialog content, the client layer uses
`scroll-into-view-if-needed` **3.1.0** with the owning overlay as its boundary,
nearest block/inline alignment and `scrollMode: 'if-needed'`. Movement is
**instant**, including with default animations enabled; it does not animate
focus into view or wait for transition events. Root-content focus and events
from a nested portal outside that content are excluded. Background scrolling
must remain unchanged.

The maintained helper handles scroll geometry, including transformed
containers, instead of adding a custom focus or rectangle-calculation
algorithm. Radix still owns focus movement, containment, restoration,
dismissal and locks. The dependency cost is two additional locked runtime
packages: the helper and `compute-scroll-into-view` **3.1.1**, both using public
npm SHA512 artifacts. Their imports stay behind the client-only modal boundary,
outside the `/client` and `/react` runtime/type graphs. Size evidence must
include every emitted entry/shared/
lazy chunk, not just the public wrapper; final byte measurements are separate
from this dependency inventory.

On close, focus restoration prefers a valid explicit `returnFocusRef`, then
the element focused at opening, then `fallbackFocusRef`, then surviving top
dialog content, then the document body. If another modal survives, targets
outside that modal are not eligible. Disconnected, disabled, hidden, inert
or otherwise unfocusable targets are not focused. Cleanup can restore focus
on the primitive's deferred zero-delay task, not necessarily synchronously
inside the owner's state update.

Styling overrides are **not an accessibility opt-out**. Preserve modal
positioning/stacking, content scrolling on small viewports, labels, focus
visibility and usable close controls. The default surface is 32rem wide,
bounded by the viewport; content height is bounded by
`calc(100dvh - 2rem)` and scrolls when necessary.

#### Scoped themes and portals

The default palette is light. Defaults are `var()` **fallbacks at usage**,
not global `:root` dark tokens or component-local assignments that shadow
ancestor values. Set `--drasi-*` custom properties on a local wrapper to style
one table/dialog subtree without changing unrelated hosts.

| Token | Core fallback / use |
| --- | --- |
| `--drasi-color-surface` | `#fff`; table and sticky headings. |
| `--drasi-color-dialog` | `#fff`; default modal surface. |
| `--drasi-color-overlay` | `rgb(0 0 0 / 70%)`; default modal backdrop. |
| `--drasi-color-border` | `#d1d5db`; table/header/modal borders. |
| `--drasi-color-primary` | `#1d4ed8`; active sorting, loading, actions and neutral row flashes. |
| `--drasi-color-success` | `#047857`; positive row flashes. |
| `--drasi-color-danger` | `#b91c1c`; table error text and negative row flashes. |
| `--drasi-color-text` | `#111827`; table/modal text. |
| `--drasi-color-muted` | `#4b5563`; headings. |
| `--drasi-color-subtle` | `#4b5563`; empty content, icons and small spinners. |
| `--drasi-color-row-border` | `#e5e7eb`; row separators. |
| `--drasi-color-row-hover` | `#f3f4f6`; row hover. |
| `--drasi-color-action-hover` | `#e5e7eb`; action/icon hover. |
| `--drasi-color-focus` | `#1d4ed8`; focus-visible outlines. |
| `--drasi-table-height` | `400px`; omitted table height. |
| `--drasi-radius` | `0.5rem`; table/default modal corners. |
| `--drasi-line-height` | `1.5` for tables; `inherit` for modal content. A unitless value preserves proportional line height when child font sizes change. |

Row-flash peaks use `color-mix(in srgb, <token> <percentage>, transparent)`:
success/danger at **20%**, primary at **25%**. They no longer hard-code Trading
colors. Trading explicitly supplies its original success/danger/primary
values, retaining those flash colors and the default 500ms timing.

`Modal` copies resolved `--drasi-*` values, including arbitrary locally
defined prefixed tokens, and the source's font family, font size, font weight,
line height and direction to its body portal. Known tokens absent at the source, including
the overlay token, are reset in the portal so their usage fallbacks apply.
The default source is its hidden inline anchor; `themeRef` selects another
element. Ancestor attribute changes
(including class/style), resize and preferred-color-scheme changes trigger a
re-read. Arbitrary CSSOM edits or stylesheet replacement without one of those
signals are **not automatically watched**: reopen the modal or change a theme
source/ancestor attribute. This is theme propagation, not a global theme store.

Typography copied from computed styles is not a reconstruction of the
ancestor's CSS inheritance rules. In particular, a computed `24px` line height
does not retain the proportional behavior of an original unitless `1.5` when
descendant font sizes change. Set `--drasi-line-height: 1.5` on the theme source
for that behavior. Trading sets this token explicitly on its own `body`;
modal content uses the token when present and otherwise inherits, while tables
retain their `1.5` fallback.

`--drasi-color-code` is a legacy token copied for Trading's app-owned code
viewer, not a generic component requirement. Its styles and dark palette live
in the Trading app, not in the package. Custom colors/content still require
contrast and focus-visibility review.

This example needs no provider. Its modal uses the same scoped values as the
table even though the dialog is portaled to `body`.

```tsx
// @drasi-docs: scoped-modal.tsx
import { useRef, useState, type CSSProperties } from 'react';
import { DataTable, Modal, type ColumnDef } from '@drasi/react/components';
import '@drasi/react/styles.css';

interface Delivery { id: string; parcels: number }
type ScopedStyle = CSSProperties & Partial<Record<`--drasi-${string}`, string | number>>;
const theme: ScopedStyle = {
  fontFamily: 'system-ui, sans-serif',
  '--drasi-color-surface': '#f8fafc',
  '--drasi-color-dialog': '#f8fafc',
  '--drasi-color-text': '#0f172a',
  '--drasi-color-muted': '#334155',
  '--drasi-color-border': '#94a3b8',
  '--drasi-color-primary': '#075985',
  '--drasi-color-focus': '#075985',
  '--drasi-table-height': '20rem',
};
const columns: readonly ColumnDef<Delivery>[] = [
  { key: 'id', label: 'Delivery' }, { key: 'parcels', label: 'Parcels' },
];
const rows: readonly Delivery[] = [{ id: 'north', parcels: 12 }];

export function DeliveryPanel() {
  const [open, setOpen] = useState(false);
  const scope = useRef<HTMLElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const close = () => setOpen(false);
  return <section ref={scope} style={theme}>
    <h2 ref={heading} tabIndex={-1}>Dispatch desk</h2>
    <DataTable<Delivery>
      rows={rows} columns={columns} rowKey={row => row.id} title="Deliveries"
    />
    <button ref={trigger} type="button" onClick={() => setOpen(true)}>Delivery details</button>
    <Modal
      open={open} onClose={close} title="Delivery details"
      description="Review the current dispatch count."
      initialFocusRef={closeButton} returnFocusRef={trigger}
      fallbackFocusRef={heading} themeRef={scope}
    >
      <h2>Delivery details</h2>
      <p>North route: {rows[0].parcels} parcels.</p>
      <button ref={closeButton} type="button" onClick={close}>Close details</button>
    </Modal>
  </section>;
}
```

#### Reduced motion

`useReducedMotion(): boolean` is exported by `/react` and root, with no
component, Radix, React DOM or CSS dependency in the hook entrypoint. It
subscribes to live `prefers-reduced-motion: reduce` changes. SSR and hosts
without `matchMedia` report `false`; that SSR value is not a claim about the
user's eventual preference.

`useRowAnimation` automatically clears active timers, directions and restart
tokens when reduced motion becomes active and keeps tracking the latest row baseline. DataTable
also suppresses controlled `rowAnimations` and passes `null` animation to
custom row renderers in this mode. The stylesheet disables package animations,
transitions and smooth scrolling under the same media query. Rows, sorting,
errors and live state updates never wait for a transition-end event.

Custom application motion remains the application's responsibility. Trading
keeps its default **350ms** FLIP expansion/collapse; reduced motion completes
it immediately without animation frames or transition timers, including when
the preference changes mid-transition. This does not delay query data updates.

The following headless recipe imports no package UI or stylesheet:

```tsx
// @drasi-docs: reduced-motion.tsx
import { useReducedMotion, useRowAnimation } from '@drasi/react/react';

interface Delivery { id: string; parcels: number }
const rowKey = (row: Delivery) => row.id;
const getValue = (row: Delivery) => row.parcels;

export function DeliveryCounts({ rows }: { rows: readonly Delivery[] }) {
  const reducedMotion = useReducedMotion();
  const { animations } = useRowAnimation({ data: rows, rowKey, getValue });
  return <ul>{rows.map(row => <li
    key={row.id}
    style={{
      transition: reducedMotion ? 'none' : 'background-color 150ms',
      backgroundColor: reducedMotion || !animations.has(row.id) ? 'transparent' : '#e0f2fe',
    }}
  >{row.id}: {row.parcels} parcels</li>)}</ul>;
}
```

### Low-level clients

#### DrasiClient

`new DrasiClient(options: DrasiClientOptions)` validates immutable connection
configuration without opening a stream. The public instance surface is:

| Member | Return / behavior |
| --- | --- |
| `readonly instanceId: string` | The configured instance ID, not a discovered/default instance. |
| `initialize()` | `Promise<void>`; validates references and resolves after the stream opens. Shares in-flight initialization and reuses an already initialized, connected client. It does not await each subscriber's snapshot. There is no signal parameter; `disconnect()` cancels pending initialization. |
| `validateResources(signal?: AbortSignal)` | `Promise<void>`; bounded concurrent query reads, followed by the configured reaction read. Requires usable running resources, SSE kind and membership of all configured query IDs. Does not open a stream or set initialized state. |
| `getQuery(queryId: string, signal?: AbortSignal)` | `Promise<Component<QueryConfig>>`; full-view DTO including lifecycle status. An inspection read need not be a subscribed/configured query and does not require it to be running. |
| `getReaction(signal?: AbortSignal)` | `Promise<Component<ReactionConfig>>`; the **configured** reaction's full-view DTO. No reaction ID argument; the read alone does not enforce running status/SSE membership. |
| `getQueryConfig(queryId: string, signal?: AbortSignal)` | `Promise<QueryConfig>`; optional definition inspection, extracting `config` from the full query read. Missing/failed reads throw, never return `null`. |
| `getQueryResults(queryId: string, signal?: AbortSignal)` | `Promise<ResultRow[]>`; validates the query and requires running status before reading its snapshot. This direct read returns **rows**, not a `QuerySnapshot` wrapper, and does not coordinate overlap with an SSE subscription. |
| `subscribe(queryId, onResult, onError?, onStateChange?)` | `QuerySubscription`; attaches to the shared stream before starting its own REST baseline. `queryId: string` must belong to `queryIds`. Callback shapes and handle methods are below. |
| `getConnectionStatus()` | `ConnectionStatus`; a shallow status copy, not a live mutable object or per-query readiness result. |
| `onConnectionStatusChange(callback: (status: ConnectionStatus) => void)` | `() => void`; immediately calls back with a status copy, then on transitions. Call the returned function to detach. |
| `getServerUiUrl()` | `string`; the instance-scoped UI link. Does not read/discover that UI or require initialization. |
| `isInitialized()` | `boolean`; whether initialization completed since the last disconnect. Not current stream health: use connection status and query state. |
| `disconnect()` | `Promise<void>`; aborts initialization/subscription work, stops retries, closes the stream and clears subscriptions/status listeners. It does not stop or delete server resources. Reinitializing later requires reattaching listeners/subscriptions. |

Direct reads perform one attempt and can run without `initialize()`, including
in a REST-only Node process. Initialization and subscriptions own the documented
bounded retry policy. Every resource read remains in `instanceId`, even when
inspecting an ID outside the live `queryIds` set.

The `subscribe` callbacks are synchronous:

- `onResult: (result: QueryResult) => void` receives a normalized snapshot or
  delta, **not an already accumulated or projected view**.
- `onError?: (error: DrasiError) => void` receives query-local or shared
  failures. Invalid subscription IDs throw synchronously when this is omitted;
  with a handler they report the error and return a terminal handle instead.
  Other unhandled subscription failures log only the safe error code.
- `onStateChange?: (state: QuerySubscriptionState) => void` receives
  `{ status, stale, error, errorScope }`. `status` excludes `'empty'` because
  only an accumulated/projected view knows whether it has rows.

Do not return unhandled promises from these callbacks; the client does not
await them. Result-callback failures are classified as
`RESULT_PROCESSING_FAILED` unless they are already `DrasiError` instances.

The callable `QuerySubscription` handle has three operations:

| Operation | Result |
| --- | --- |
| `subscription()` | `void`; unsubscribe and cancel this subscription's pending reads/timers. No shared disconnect. |
| `subscription.retry()` | `void`; restart only this subscription's REST reconciliation and retry budget. It does not open a failed shared socket, and is inert after unsubscribe. |
| `subscription.getState()` | `QuerySubscriptionState`; a copy of current work state, with `stale: boolean`, `error: DrasiError \| null`, `errorScope: QueryErrorScope \| null` and `status: Exclude<QueryStatus, 'empty'>`. |

For direct ownership, initialize once, retain the handle and dispose it along
with the client when that owner finishes:

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

#### DrasiSSEClient

`new DrasiSSEClient(options?: DrasiSSEClientOptions)` defaults to `{}` and is
the lower-level stream multiplexer, not a replacement for reference validation
or snapshot reconciliation. It accepts **flat** reconnect fields, unlike
`DrasiClientOptions.reconnect`:

| `DrasiSSEClientOptions` field | Default / contract |
| --- | --- |
| `maxReconnectAttempts`, `initialReconnectDelayMs`, `maxReconnectDelayMs`, `connectionTimeoutMs` | Optional numbers inherited from `ReconnectOptions`; the same **10 / 1000 / 30000 / 10000** defaults and constraints [above](#connection-options). |
| `resultAdapter?: ResultAdapter` | `sse034ResultAdapter`; normalized output is validated. |
| `eventSourceFactory?: EventSourceFactory` | Native browser EventSource, with the [authentication limitations](connection.md#authentication-and-injected-transports) in the connection guide. |
| `headers?: DrasiHeaders` | No custom headers; resolved afresh per attempt with the SSE Accept default. |
| `credentials?: RequestCredentials` | `'same-origin'`; native streaming cannot implement `'omit'`. |
| `validate?: (signal: AbortSignal) => Promise<void>` | Absent by default. Read-only validation before opening and after opaque stream errors; reject with a typed error to classify recovery. The owner must honor cancellation **and bound this callback itself**: the stream-open timer starts after pre-open validation. `DrasiClient` supplies validation with bounded REST reads. |
| `errorDetails?: DrasiErrorDetails` | `{}`; safe instance/resource metadata for stream/auth failures and adapter context, not server authorization or resource discovery. |

There is no `fetch`, REST timeout, reconciliation option, initialized flag or
resource-management mode on this class. Without `validate` it does not read
REST resources and cannot infer their absence. Unlike `DrasiClient`, direct
use does not validate resource references/endpoint configuration for you;
the owner must supply a trusted browser endpoint and enforce its routing policy.

| Method | Return / behavior |
| --- | --- |
| `connect(queryIds: string[], endpoint: string, signal?: AbortSignal)` | `Promise<void>`; replaces pending/current connection work and resolves when open, or rejects after permanent failure/exhaustion/cancellation. The signal cancels the **pending opening**; use `disconnect()` for lifetime cleanup after it resolves. `queryIds` is not a membership or authorization check at this layer: delivery is keyed by actual subscriptions and normalized result IDs. |
| `subscribe(queryId: string, onResult: (result: QueryDelta) => void, onError?: (error: DrasiError) => void)` | `() => void`; attaches a delta-only subscriber and returns its plain cleanup function. No REST snapshot, accumulated rows, state callback or query-local refresh method. Provide `onError` for query-scoped protocol/callback failures. |
| `getQueryError(queryId: string)` | `DrasiError \| null`; retained query-scoped fault, separate from shared status. A later valid batch for that query clears it; disconnect clears all faults. |
| `getConnectionStatus()` | `ConnectionStatus`; a status copy. |
| `onConnectionStatusChange(callback: (status: ConnectionStatus) => void)` | `() => void`; immediate status callback plus later transitions; returned function removes the listener. |
| `isConnected()` | `boolean`; whether the stream is currently open, not a snapshot guarantee. |
| `disconnect()` | `Promise<void>`; cancels open/retry work, closes the stream, publishes disconnected status and clears subscribers, query errors and status listeners. |

Both clients close a failed native stream before managing their own bounded
retries; do not add an independent reconnect loop inside an injected transport.
Calling low-level `connect` again replaces connection work, but it is not a
query refresh or a cross-instance subscription scope manager. For normal
application use, prefer `DrasiClient` and its subscription lifecycle.

## Errors and recovery

`new DrasiError(code: DrasiErrorCode, details?: DrasiErrorDetails)` extends
`Error`, sets `name: 'DrasiError'`, chooses a safe message from the code, and
defaults details to `{}`. `code` and `retryable: boolean` are readonly;
retryability is computed from the code, not supplied by the caller.
`DrasiErrorDetails` contains optional `instanceId: string`,
`resourceKind: DrasiResourceKind`, `resourceId: string`,
`resourceStatus: string` and HTTP `status: number`; each is also a readonly
optional field on the error. `DrasiResourceKind` is
`'instance' | 'query' | 'reaction'`.
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

## Troubleshooting

| Symptom | Check / next action |
| --- | --- |
| Package import cannot resolve, or a source checkout works but the tarball does not | Build before local consumption, install the actual packed file, and use only the [exported paths](getting-started.md#installation-and-entrypoints). A browser React app supplies the measured React/React DOM peers; a React-free process must import `/client`, not root. |
| REST inspection works but the stream never opens | Check the **browser** reaction endpoint, proxy route, CSP `connect-src`, CORS and cookies. Header auth or `credentials: 'omit'` needs a real custom SSE transport; native EventSource cannot implement it. Check typed status before retrying. |
| `INVALID_CONFIGURATION` before connection | Check explicit instance/query/reaction IDs, duplicate IDs, URL constraints and positive timeout/backoff values. There are no implicit resource or endpoint defaults. |
| `QUERY_NOT_FOUND`, `REACTION_NOT_FOUND` or stopped resources | Verify the instance and deployment with the resource owner. The client will not create/start anything. Follow the [#119 migration](migration.md#119-bootstrap-to-connect-only-migration); opaque SSE/network errors do not establish absence. |
| `connected` is true but rows are loading, stale or wrong | Stream health and query state differ. Inspect `status`, `errorScope` and `stale`. Overlap triggers a bounded refresh; an undetectably delayed event can still temporarily replace newer state. A query-local refresh reads another best-effort baseline, not a guaranteed cursor. |
| `INVALID_ROW_KEY`, unexpectedly retained deletes or missing projected rows | Derive a stable key from the **raw** projection, including sparse deletes and both update sides. Do not use rendered `rowKey` as the accumulator key. A `null` transform intentionally hides a retained raw identity; a throwing/invalid transform is an error. |
| Repeated connection replacement after rendering | Stabilize callable connection options (`fetch`, factory, adapter, headers provider). Equivalent inline data props do not cause replacement. Hook projection callbacks reproject rather than reopen the stream. |
| A retry does not recover | Correct permanent configuration/auth/callback failures first. Use query retry for subscription REST/projection work and provider retry for shared failures. A definition-reader retry is a separate remount, not a result refresh. Do not layer an unbounded retry loop over the built-in budget. |
| Unstyled components or missing row flashes | Import `/styles.css` once for components and define scoped tokens as needed. Headless hooks never import styles. Initial/unchanged values do not flash, and reduced motion suppresses animation without suppressing updates. |
| Height throws or percentages have no effect | Replace legacy class strings with the [validated size](#table-sizing); give percentage heights a sized parent. Define referenced custom properties or their literal fallbacks. `0` is not a hide/collapse flag. |
| A Modal is blank, will not close, or focus returns unexpectedly | SSR intentionally has no portal. Check the lazy chunk and React error boundary, meaningful `title`, controlled `open` updates and a visible close button. Verify focus refs point to eligible elements and account for a surviving top modal. Do not add competing focus/scroll owners. |
