# Connections, lifecycle and hosting

[Package overview](../README.md)

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
| `Component<T>` | `{ id: string, status: ComponentStatus, links: ComponentLinks, config: T, error_message?: string }`; ID and links must match the requested instance/resource. |
| `ComponentLinks` | `{ self: string, full: string }`; resource and `?view=full` links, normalized to encoded instance-scoped paths. |
| `ComponentStatus` | `'Starting' \| 'Running' \| 'Stopping' \| 'Stopped' \| 'Error' \| 'Reconfiguring' \| 'Added' \| 'Removed'`. |
| `QueryLanguage` | `'Cypher' \| 'GQL'`; the server must return an explicit value. |
| `QueryConfig` | Required `id: string`, `autoStart: boolean`, `query: string`, `queryLanguage: QueryLanguage`, `middleware: readonly QueryMiddleware[]`, `sources: readonly QuerySource[]`, `enableBootstrap: boolean`, `bootstrapBufferSize: number`, `outboxCapacity: number`, `bootstrapTimeoutSecs: number`. |
| Optional `QueryConfig` fields | `joins?: readonly QueryJoin[]`, `priorityQueueCapacity?: number`, `dispatchBufferCapacity?: number`, `dispatchMode?: 'Channel'`, `storageBackend?: JsonValue`. Capacities/timeouts are nonnegative safe integers; absent fields do not acquire client defaults. |
| `QuerySource` | `{ sourceId: string, pipeline: readonly string[], nodes: readonly string[], relations: readonly string[] }`. |
| `QueryJoin` / `QueryJoinKey` | `{ id: string, keys: readonly QueryJoinKey[] }` / `{ label: string, property: string }`. Source and join ordering is retained. |
| `QueryMiddleware` | `{ kind: string, name: string, config: Readonly<Record<string, JsonValue>> }`; configuration belongs to its server plugin. |
| `ReactionConfig` | Required `id: string`, `kind: string`, `queries: readonly string[]`; flattened plugin fields have type `JsonValue \| undefined`, not a nested creation body. |
| `SseReactionConfig` | Extends `ReactionConfig` with `kind: 'sse'`, `host: string`, `port: number` (integer 1–65535), `ssePath: string` (starts with `/`) and `heartbeatIntervalMs: number` (nonnegative safe integer). `getReaction()` still returns the general reaction read type. |
| `JsonValue` / `ResultRow` | JSON null/boolean/number/string/readonly arrays/readonly string-keyed objects, versus `Record<string, unknown>` object rows. Wire numbers must be finite; applications must narrow row fields themselves. |

These are **read DTOs, not creation DTOs**. No pass-through creation fields,
ignored deployment options or implicit Cypher selection are offered.
Configuration/component DTO properties and nested configuration arrays are
readonly in declarations; do not mutate them to try to change server resources.
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
old provenance. See [verified compatibility](testing.md#verified-compatibility).

## Authentication and injected transports

`fetch` has the native fetch signature. Every request receives `method: 'GET'`,
fresh headers, credentials, `redirect: 'manual'` and an `AbortSignal`.
Native fetch is invoked with its global receiver, preserving browser binding.
The default credentials are `same-origin`; REST defaults to
`Accept: application/json`.

`headers` accepts `HeadersInit` or an async/sync `DrasiHeadersProvider`.
`DrasiHeaders` is the union of those two inputs. The provider receives a
`DrasiRequestContext`: `{ url: string, transport: 'rest' | 'sse',
signal: AbortSignal, instanceId?: string }`
and has signature
`(context: DrasiRequestContext) => HeadersInit | Promise<HeadersInit>`.
It runs for **each** REST read and stream attempt. A stable function can
refresh tokens without replacing the client. Static headers are copied at
construction; a transport cannot mutate another request's headers. The library
does not log credentials, token-bearing URLs, response bodies or user callback
exception details. `DrasiClient` supplies the explicit instance; a direct
`DrasiSSEClient` supplies it only when included in `errorDetails`.

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
`(url: string, options: EventSourceOptions)` and returns an
`EventSourceLike`. It must apply these options, honor cancellation/`close()`
and expose the declared callbacks/listener interface. SSE headers default to
`Accept: text/event-stream`. A known `DrasiError` thrown by a factory/auth
provider retains its identity and classification.

`EventSourceOptions` contains readonly `headers: Headers`,
`credentials: RequestCredentials` and `signal: AbortSignal`. The
`EventSourceLike` interface requires writable nullable `onopen` and `onerror`
callbacks `(event: Event) => void`, a nullable
`onmessage: (event: MessageEvent<string>) => void`,
`addEventListener(type: string, listener: EventListener): void`, and
`close(): void`. The transport listens to ordinary messages and named
`query-result` events. No `readyState` or `removeEventListener` member is
required. A factory returns the stream object synchronously, **not a Promise**;
perform asynchronous authentication in the headers provider.

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

Callbacks should honor their signal. REST/open-auth timeouts also settle
client-facing work when an injected fetch or headers provider ignores
cancellation; late completions cannot open a stream or publish data. A
standalone SSE `validate` callback must bound its own work as described
in the [SSE client reference](reference.md#drasisseclient). Explicit read cancellation preserves the caller's
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
path/query differences, including trailing slashes, remain material:
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

## Consumer and hosting responsibilities

- Own provisioning, query definitions/languages, source/reaction deployment,
  domain identity, validating projections and any explicit legacy routing.
  The package is a **GET-only consumer**, not an authorization or management
  boundary. Restrict server/proxy access to the intended instances/resources;
  enforce authorization on both REST and SSE. Browser query IDs and disabled
  action buttons do not enforce access control.
- Use trusted HTTPS REST/SSE endpoints in production. Do not put service
  credentials in a frontend bundle, query text, logs or token-bearing URLs.
  Acquire browser credentials through your application's authentication flow
  and the supported header/cookie transport contract. CORS is not authorization;
  credentialed cross-origin use needs explicit allowed origins and credentials
  support, not a wildcard-origin workaround.
- Serve the built application and **all** its emitted shared/lazy chunks and
  styles with correct MIME types and paths. The Modal client chunk must remain
  reachable after initial load; an HTML SPA fallback is not a JavaScript
  response. Importing `/react` does not require a package CSS/portal loader.
  Configure an SSE proxy for streaming `text/event-stream` without response
  buffering/caching and with suitable connection timeouts. REST redirects are
  deliberately rejected, so expose the correct REST URL directly.
- Set production security headers at the host/proxy: a
  **Content-Security-Policy** with narrowly allowed script/style sources and
  REST/SSE `connect-src` destinations (plus `frame-ancestors 'none'` when
  embedding is not allowed), **Strict-Transport-Security** for the deployed
  HTTPS origin after reviewing its scope, **X-Content-Type-Options: nosniff**,
  and **X-Frame-Options: DENY** for non-embedded apps. Account for the
  components' inline layout/portal styles in the actual CSP and test it;
  this package neither sends HTTP headers nor supplies a universal CSP.
- Treat result/config fields as untrusted application input. Validate
  projections and action inputs; render text through React rather than
  inserting unsanitized HTML. Applications own asynchronous action errors,
  business mutations and error boundaries, including lazy import failures.
- Own semantic markup, contrast, focus visibility and announcements in custom
  rows/headers/slots and hooks-only UIs. Keep usable names and visible close
  controls; don't turn every changing cell into an uncontrolled live region.
  Verify themes and motion under the real host CSS and complete human AT
  review, rather than assuming a component's automated checks approve an app.
- Dispose owned clients/subscriptions on scope change and unmount. Bound the
  server query's result size for your workload: `maxPendingChanges` does not
  cap retained rows, and the package provides no pagination or virtualization.

## SSR and import safety

All entrypoints are safe to import/require without DOM/network activity.
`DataTable` can server-render real non-Drasi rows and state slots **without a
provider or browser/network globals**. Rendering providers and an initial
QueryTable does not run effects or open streams. `Modal`, whether `open` or
closed, renders only one hidden inline theme anchor during SSR and initial
hydration, with no body-portal content. Focus, scroll ownership and interactive portals require a browser
DOM. `useReducedMotion` has a deterministic `false` server snapshot.
A direct client can perform REST-only reads in Node with native/injected fetch,
including auth, without React or EventSource. **Default live execution is
browser-only**: missing EventSource produces `INVALID_CONFIGURATION`.
SSR is not a server-side live subscription or a preloaded query snapshot.

Clean packed tests use DOM/network traps, ESM and CommonJS, NodeNext
`.mts`/`.cts` and bundler resolution with `skipLibCheck: false`. Client-only
tests omit peers and verify no React runtime or `@types/react` declaration graph;
hooks do not load presentation, Radix, React DOM or CSS. Marked runnable snippets in the README and guides are
extracted from the installed tarball and type-checked against its real exports.
No source aliases or hidden React copies satisfy these contracts.
