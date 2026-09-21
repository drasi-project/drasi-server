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
| `@drasi/react/client` | `DrasiClient`, `DrasiSSEClient`, `DrasiError` and connection/transport/read DTO types. **No React/React DOM runtime or type imports.** |
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
      getKey: row => row.id,
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
  return <span>{status.error?.message ?? (status.connected ? 'Live' : 'Connecting')}</span>;
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
timeout or retry-policy changes replace the lifecycle. Old requests are aborted,
the old stream closes, and late events/snapshots/definition reads cannot update
the new scope. Rows/config from another scope are hidden while replacement
work starts. This is not Part B's complete stale/reconnect state model.

**Callable options compare by identity:** `fetch`, `eventSourceFactory`,
`routeUnidentified` and a header-provider function. Memoize them when their
meaning is unchanged. Replacing a function replaces the lifecycle; a stable
auth function may return a fresh token on the next request. Query-hook
`getKey`/`transform`/`postProcess` option changes apply to subsequent batches
without reopening the stream; they do not reprocess stored rows retroactively.

`useDrasiClient().retry()` restarts that provider's **shared**, read-only
connection/snapshot lifecycle. A retained retry from an obsolete configuration
does not restart its replacement. Automatic transient snapshot retries stay
local to their subscription and do not disconnect other queries.
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
| `routeUnidentified?: RouteUnidentified` | Application adapter for legacy batches without a query ID. Receives `ResultRow[]` and a delivery callback. |
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

### Hooks and result boundary

| Hook | Named result |
| --- | --- |
| `useDrasiClient()` | `DrasiContextValue`: `{ client, initialized, error, retry }`. |
| `useDrasiQuery<T = ResultRow>(queryId, options?)` | `UseDrasiQueryResult<T>`: `{ data: T[] \| null, loading, error, lastUpdate: Date \| null }`. |
| `useDrasiConnectionStatus()` | `ConnectionStatus`: `connected`, optional `reconnecting`, `DrasiError`, `lastConnected`. A socket opening does not prove query synchronization. |
| `useDrasiQueryDefinition(queryId)` | `UseDrasiQueryDefinitionResult`: `{ config: QueryConfig \| null, loading, error }`. |
| `useDrasiServerUiUrl()` | Instance-scoped UI URL when initialized, otherwise `null`. |
| `useRowAnimation(options)` | `UseRowAnimationResult`; exported `AnimationDirection` and `UseRowAnimationOptions`. |

All provider/hooks preserve `DrasiError` objects, not just messages.
`UseDrasiQueryOptions<T>` supplies `getKey(row: T)`, optional
`transform(row: ResultRow): T`, and `postProcess(rows: T[]): T[]`.
Key extraction is **after** transformation; returning `null` skips a row.

```ts
// @drasi-docs: query-options.ts
import type { UseDrasiQueryOptions } from '@drasi/react/react';

interface Reading { device: string; value: number }
export const readingOptions: UseDrasiQueryOptions<Reading> = {
  getKey: row => row.device,
  transform: row => {
    if (typeof row.device !== 'string' || typeof row.value !== 'number') {
      throw new Error('Expected a Reading');
    }
    return { device: row.device, value: row.value };
  },
  postProcess: rows => rows.sort((a, b) => b.value - a.value),
};
```

Raw rows are `Record<string, unknown>`. A generic alone does not validate fields:
without `transform`, choosing `T` is the caller's schema assertion. Prefer a
validating transform for external data; failures become typed hook errors.

**Deliberately unchanged until Part B (#163):** the legacy key fallback remains
`id`, then `symbol`, then serialized row. `_deleted` is retained through
transforms, but minimal delete payloads still pass through the transform/key
path. Existing keyed `queryId`/`query_id` result/data shapes and the legacy
unidentified added/updated/deleted routing alternatives remain compatibility
behavior, not a new official normalized result contract. Stable identity,
key-changing updates and named adapters are not completed here.

Subscribers listen before their REST snapshot, buffer deltas while it loads,
then replay them; reconnect requests a fresh snapshot. This does **not**
establish an atomic, ordered or exactly-once handoff without a server
sequence/cursor, and arrival timestamps do not solve that problem. Full
snapshot/live reconciliation and initial/live/reconnecting/stale/error states
remain Part B. Current `loading`/transport flags must not be interpreted as
stronger consistency guarantees.

### Components

`QueryTable<T>` binds `useDrasiQuery` to the existing sortable, animated table.
Required props are `queryId`, `columns: ColumnDef<T>[]` and `rowKey(row: T)`.
`queryOptions` forwards the typed hook options. `ColumnDef`, `RowAction`,
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
Pure presentation/composition, controlled sort, slots, dialog accessibility
and theming redesign are separate #164 work, not claims of this release.

CSS is namespaced and package-owned. `--drasi-*` variables and existing class
props customize appearance; portals and advanced overlay semantics have not
been redesigned. Consumers do not need Tailwind.

### Low-level clients

`DrasiClient` exposes `initialize()`, `validateResources(signal?)`,
`getQuery(id, signal?)`, `getReaction(signal?)`, `getQueryConfig(id, signal?)`,
`getQueryResults(id, signal?)`, `subscribe(id, onResult, onError?)`,
`getConnectionStatus()`, `onConnectionStatusChange(callback)`,
`getServerUiUrl()`, `isInitialized()` and `disconnect()`.
Direct reads perform one attempt; initialization and subscriptions own the
documented retry policy. Each subscription returns an unsubscribe function.
Provide `onError` to handle subscription failures; if omitted, the library logs
only the safe error code rather than silently swallowing a malformed snapshot.

```ts
// @drasi-docs: client.ts
import { DrasiClient, type DrasiError, type ResultRow } from '@drasi/react/client';

// Execute in a browser, not while importing or server rendering.
export async function monitorReadings(
  onRows: (rows: ResultRow[]) => void,
  onError: (error: DrasiError) => void,
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
  const unsubscribe = client.subscribe('readings', result => onRows(result.data), onError);
  return async () => { unsubscribe(); await client.disconnect(); };
}
```

`DrasiSSEClient` is the lower-level stream multiplexer. `DrasiSSEClientOptions`
adds an optional read-only `validate(signal)` callback and `errorDetails` to
the stream/auth/reconnect options. Direct use does not validate REST resources
unless that callback is supplied and cannot infer their absence. It exposes
`connect(queryIds, endpoint, signal?)`, `subscribe`, `getConnectionStatus`,
`onConnectionStatusChange`, `isConnected` and `disconnect`.

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

## P3 migration

Use `/client`, `/react` or `/components` for the dependency boundary you need;
root imports remain supported. Import `/styles.css` explicitly for components.
React peers are now optional for installation, not optional when executing
React APIs, and support is limited to the exercised version.

Connection options still require server, instance, query IDs and an explicit
reaction endpoint. Keep creation definitions in your app; a complete
`QueryConfig` read now includes real server fields and is not a convenient
creation-body type. Trading uses its own `TradingQueryDefinition`.

Raw result/transform/route fields are now `unknown`, not ambient `any`.
Narrow them or use a validating transform. Get-key/action callbacks receive
the typed row; adapt formatters to the typed column API rather than casting
rows to `any`. Hook results/options and error/connection/sort types have public
names. Equivalent inline data configuration is safe; callable identities remain
material. Custom stream factories may accept the new auth/credentials/signal
argument; one-argument factories remain assignable but must not ignore
authentication when configured.

Legacy `_deleted`, content routing, key fallback and snapshot buffering are
explicitly retained for the next Part B PR. This migration does not claim
stable-key, key-changing-update, exactly-once or complete stale-state behavior.

## Development and Trading verification

The source/type graphs are physically separated under `src/client`,
`src/react` and `src/components`; `src/types.ts` is only a root compatibility
barrel, not an import used by the client. `npm run build` emits ESM, CJS and
both declaration formats. `npm run typecheck`, `npm test`,
`npm run test:coverage` and `npm run dev` use the existing tooling.
The tarball includes README, CHANGELOG, LICENSE, NOTICE, dist and CSS only.

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
new examples/Storybook, composition/accessibility and Part B result redesign are
separately authorized work, not a next step implied by this package.

## License

Apache License 2.0. See LICENSE and NOTICE.
