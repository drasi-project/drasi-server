# Changelog

All notable changes to `@drasi/react` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Parent integration
- Normally integrate the approved server 0.2.3 / registry library 0.9.1 /
  host SDK 0.11.0 / signed SSE 0.3.6 (plugin SDK 0.11.1, native ABI 0.13)
  runtime while retaining the reviewed engine correction and all P3/P4 contracts.
- Keep original runtime fixtures and P1/P2/P3/P4/P5 schema-2 measurements historical.
  Current accounting includes both shipped declaration formats, all package
  chunks/CSS and recursive Trading assets without changing coverage floors or
  the 2% growth policy. The original P5 tarball contains 37,834 ESM and 37,851
  CommonJS declaration bytes; counting both adds no product bytes.
- Document that new archive/memory-budget controls are server/instance settings:
  the query read DTO is unchanged and `storageBackend` remains opaque JSON.
  Existing P5 composition is preserved; no P6/P7 features or query/resource-
  creation defaults are introduced by this integration.
- Retain all 17 actual SSE 0.3.6 records from P4's own rebuilt normal-merge
  server separately, with manifest/lock/binary/signature provenance. Exercise
  the unchanged `sse034ResultAdapter` and default transport against them,
  including query-ID routing and aggregation before/after without `data`.
  The adapter name/API and P4 identity/recovery semantics do not change;
  native ABI compatibility is not a universal wire-format claim.

### Added
- P5 / #164 Part A provider-free `DataTable<T, E extends Error = Error>` with
  readonly rows/columns, application-owned state/retries, typed loading/empty/
  error/stale/header slots, right-hand header controls, card ref/style hooks and
  keyed custom-row composition. No provider, query identity or network is
  needed to present supplied rows.
- Headless `useTableSort`, `UseTableSortOptions` and `UseTableSortResult` through
  `/react`; `SortConfig` is shared with `/components`. Defined `sort` (including
  null) is controlled, undefined is uncontrolled, and null restores input order.
  One notification per setter/header action, outside state updaters; controlled
  changes preserve stored uncontrolled state. Header toggling remains asc/desc,
  not an automatic three-state cycle.
- Pure `/components` `queryTableState` adapter and full typed query-state slot
  contexts, preserving P4 status/staleness and query-local versus shared retries.
- Optional readonly animation maps for shared table presentations and readonly
  `useRowAnimation.data` / `updateData` inputs.
- Installed type contracts and provider-free non-Trading SSR coverage for the
  composition API; runtime graphs reject tutorial/dialog implementations while
  headless sort types remain allowed. Five additional marked README recipes
  cover supplied rows, both sort modes, scoped recovery and app-owned views.
- P4 / #163 Part B normalized `ResultChange`, `QuerySnapshot`, `QueryDelta` and
  `QueryResult` contracts, exported with adapters and the framework-independent
  `accumulateResult` raw reducer through `/client` and root.
- Strict default `sse034ResultAdapter` for pinned, untemplated SSE 0.3.4;
  explicit `createLegacyResultAdapter` for legacy op/lowercase/keyed/batch
  formats and app-owned unidentified routing. Custom adapter output is also
  runtime-validated. Source-defined aggregation with null `before` becomes an
  upsert. Noop contributes no changes; empty normalized batches, heartbeat and
  valid unsubscribed batches are ignored rather than treated as corrupt results.
- Named callable `QuerySubscription` cleanup handles with query-local `.retry()`
  and `.getState()`, an optional subscription-state callback, and hook
  `status`, `stale`, `errorScope` and `retry`. Per-query processing/subscription
  REST faults are isolated while the transport stays healthy; low-level SSE
  exposes `getQueryError`. Initial/shared reconnect validation still checks
  every configured reference and may fail the shared connection.
- Bounded known-overlap recovery with retryable `SNAPSHOT_OVERLAP` and
  `RESULT_BUFFER_OVERFLOW`. Pending changes are counted, not retained/replayed;
  `reconciliation.maxPendingChanges` defaults to 10000 and does not cap result
  size. Exhaustion is terminal and can retain useful stale last-good data.
- Terminal `INVALID_ROW_KEY`, `RESULT_PROCESSING_FAILED` and
  `UNROUTABLE_RESULT` errors, with existing typed error identity preserved.
- Adapter context includes optional read-only `DrasiErrorDetails` and required
  receipt metadata. New unidentified stream faults retain configured reaction
  details, while keyed faults carry query details; existing context literals
  remain valid.
- Real `@drasi/react/client`, `/react` and `/components` ESM/CommonJS exports
  with `.d.ts`/`.d.cts` declarations. The client runtime/type graph is
  React-independent; hooks do not import composed components or CSS. Root and
  explicit `styles.css` imports remain supported.
- Complete guarded v1 read DTOs, validated object-row boundaries, scoped
  component links and versioned unmodified real-server contract fixtures.
- Package-local SSE 0.3.4 evidence: all 20 original recording lines copied
  verbatim into `test/fixtures/server-v1/sse-0.3.4.ndjson`, with exact upstream
  tag/revision/serializer provenance recorded in `contract.json`.
- Portable `test/ResultRegression.test.jsx` proof: the same three behavioral
  cases fail with observable wrong results on archived P3 `a0569c2` and pass
  on P4, covering raw identity before projection/sparse deletion, key-changing
  official updates and bounded refresh instead of overlapping snapshot replay.
  Real provider/client/transport implementations are exercised.
- Shared request credentials, copied headers and asynchronous per-request auth
  providers. Custom stream factories receive headers/credentials/cancellation;
  unsupported native EventSource auth fails explicitly when opening a stream.
- Named generic/concrete client, connection, hook-result, column/action/sort and
  error types; clean packed positive/negative type, client-only and SSR tests.
- Material configuration lifecycle: equivalent inline data props preserve the
  connection; changed references/auth/policy/callable identities dispose old
  work and suppress stale results. Retry callbacks are bound to their scope.
- Executable README snippets, instance/auth/import/SSR/reconfiguration/migration
  and compatibility documentation shipped in the tarball.
- Explicit `instanceId`, `queryIds` and `ReactionReference` connect-only contract;
  all validation/snapshot/definition reads use the selected instance.
- `DrasiError` with stable codes, safe messages, identity and retryability,
  preserved through hooks/context; REST classification of opaque SSE failures.
- Bounded opening, REST and snapshot retries; permanent failures stop work.
- Controlled `DrasiClientProvider` for an app-owned lifecycle without a second
  connection. Trading now owns idempotent setup, conflicts and cancellation.
- Extracted the reusable React building blocks into a private, unpublished
  package (`@drasi/react`) under `dev-tools/react`.
- `tsup`-based build emitting ESM, CommonJS, and TypeScript declarations.
- Source reorganized into `client/` (framework-agnostic core), `react/`
  (provider + hooks), and `components/` (ready-made UI) with barrel exports.
- Initial release: `DrasiProvider`, `useDrasiQuery`, `useDrasiConnectionStatus`,
  `useDrasiServerUiUrl`, `useDrasiQueryDefinition`, `QueryTable`,
  `useRowAnimation`, and the low-level `DrasiClient` /
  `DrasiSSEClient` classes.
- Package-owned namespaced CSS, lifecycle-safe SSE reconnection and
  package/consumer CI.

### Changed
- `QueryTable` is now a small `useDrasiQuery` + `DataTable` composition.
  `queryOptions.getKey` and `queryOptions.transform` remain required, with
  `rowKey` separate. Slots retain last-good rows; `renderEmpty` returns content
  inside one spanning cell. Sort callbacks accept null, and custom-row column
  arguments are readonly.
- Removed package `CodeViewerDialog`/`CodeViewerDialogProps` exports and the
  table's `codeSnippet`, implicit inspection and fullscreen behavior. Generic
  icons remain optional exports; application controls use `headerControls`.
  Trading's local `TradingQueryTable` owns normal/fullscreen presentations with
  one query/sort/animation owner and the existing FLIP transition/markup.
- Trading owns `QueryInspector`, query formatting, code snippets, UI links and
  `CodeViewerDialog`. Definition reads mount only on a code click with a snippet,
  abort on close, and offer Retry query definition for their own read failures,
  remounting only that read. If the error is the same non-null object as the
  provider's error, the inspector instead offers Retry connection through the
  provider callback; matching codes/messages alone does not select that scope.
  Async definition content now updates an open view and its copy action.
  Required initialization and per-subscription resource-validation GETs remain.
- P5 leaves package CSS byte-for-byte unchanged, including legacy app-used
  dialog/fullscreen rules and the existing `height` class-name contract.
- `useDrasiQuery` now requires options containing a nonempty stable raw `getKey`
  and `transform`, including `row => row` for raw reads. Generic output does
  not assert a wire schema. Identity precedes projection; sparse deletes never
  run transforms. Key-changing updates remove the old identity; upserts are
  idempotent by key, with no value-based deduplication or fallback identity.
- A null transform hides a retained raw identity rather than deleting it.
  `getKey`, `transform` and pure derived `postProcess` changes immediately
  reproject raw state without a new subscription/socket. `QueryTable` requires
  `queryOptions`; its transformed `rowKey` is not an accumulation fallback.
- `QueryTable` preserves available last-good rows with the existing error
  styling, offers scope-appropriate Retry query / Retry connection, and shows
  stale reconnect/resync messages only on the exceptional recovery path.
  P5 retains that P4 recovery behavior through `queryTableState` and DataTable;
  healthy table presentation and CSS remain unchanged.
- Connection options replace top-level `routeUnidentified` with `resultAdapter`.
  Explicit IDs precede legacy routing. Unidentified routing must synchronously
  deliver all original callback row references and may fan out; its before/after
  updates are flattened into delete-before then upsert-after, a documented
  compatibility limit. Only the selected legacy adapter interprets `_deleted`.
- Snapshots/deltas use discriminated `rows`/`changes`, not legacy
  `data`/`snapshot` flags. Receipt/source timestamps are display metadata only;
  unsafe upstream `u64` JSON `row_signature`s are not row identities.
- Any known snapshot/delta overlap rejects the ambiguous candidate and
  resynchronizes instead of replaying a guessed order. A no-known-overlap
  baseline remains best effort; delayed events and cross-transport causality
  prevent an atomic, gap-free or exactly-once handoff. The pinned SSE 0.3.4
  serializer does not transmit the library's internal sequence.
- Tests expose the undetectable delayed-event limit: an older change arriving
  after an accepted newer REST baseline can temporarily replace newer row
  state until explicit refresh. Timestamps are never used to guess order.
- Raw public values are validated `unknown`/`ResultRow`, not ambient `any`.
  Consumers narrow fields in transforms; a generic alone is not runtime schema
  validation. Trading owns its distinct query-creation type and typed transforms.
- Read contracts reject incomplete/malformed full-view DTOs and snapshots,
  unsupported enum values and cross-instance links. REST redirects are not
  followed. Timeouts bound even injected callbacks that do not settle on abort.
- React/React DOM peers are optional to install for client-only consumers but
  required for React/component/root execution; the verified peer version is
  18.3.1. Node 22.20.0/24.19.0 tooling is exercised; no React 19/future-major claim.
- Artifact measurement counts all entrypoints and shared runtime/declaration
  chunks rather than only the root barrel; the inherited 2% growth policy and
  coverage floors remain in force.
- Coverage includes all product source and enforces separate client/react
  subtree floors of 90% statements/lines/functions and 85% branches; existing
  artifact-size baselines are unchanged.
- Removed package provisioning and deployment definitions (`queries`,
  `QueryDefinition`, `ReactionDefinition`, bind host/port and implicit defaults).
  Supply existing resource references instead; no management mode replaces them.
- Hook/status errors are `DrasiError` objects rather than strings. Render
  `.message`, inspect `.code`. Missing definition reads throw rather than
  returning `null`. Existing-resource connections never compare desired query text.

### Deferred
- Stronger snapshot/live consistency requires a shared backend snapshot cursor
  and stream resume/replay contract, not client clocks or guessed signatures.
- P6 focus management, coordinated overlays, theming, reduced-motion and
  height-prop completion remain separate from P5 / #164 Part A. Standalone
  examples/Storybook (#165), publication and repository transfer remain separate.
