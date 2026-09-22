# Changelog

All notable changes to `@drasi/react` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Preserve opaque restart identity across React-batched expiry/reactivation
  and inactive/active commits before a browser paint. Keep only one per-hook
  scalar and the rendered row's phase; expired/deleted map entries still clear.
  Stable rows/focus, first activation, timing, colors and reduced motion remain.
- Restart successive row highlights, including repeated up/down/string changes,
  without replacing stable rows or losing cell state/focus. The headless hook
  exposes readonly `revisions`; controlled DataTable owners share them through
  `rowAnimationRevisions`, as Trading now does for both presentations.
  Equivalent alternating keyframes preserve the 500ms easing/colors; expiry,
  removal, independent rows and live reduced-motion cleanup remain bounded.
- Compact generated whitespace while preserving syntax, identifiers, debug/
  component names, source maps/content and the complete package inventory.
  Original baselines remain; Trading TESTING.md records the separately
  user-approved, fixed 110-byte P5 Trading gzip allowance, not applied to P6.
- Make table ordering transitive: numeric values precede fixed-English text
  representations, then nullish values; handle NaN, infinities and stable ties.
  Use explicit en-US variant/numeric:false collation for SSR/hydration rather
  than ambient locale, preserving Trading's English name ordering.
- Apply explicit left alignment to body cells as well as headings; omitted
  body alignment still inherits. Cover real cross-locale hydration, host
  alignment and mixed-order permutations without changing original images.
- Prevent suspended or abandoned concurrent option renders from changing the
  active query's raw key callback. Publish committed keys before layout-phase
  event delivery without recreating subscriptions/sockets or adding SSR
  browser-global probes/layout-effect warnings. Committed projection changes
  and hidden raw-row retention remain reactive.
- Preserve explicit SSE endpoint path/query trailing slashes, including signed
  or opaque query values, instead of applying server-base trimming to them.
  Provider identity uses the same separation: equivalent server bases stay
  stable, while meaningful endpoint changes replace the connection. Existing
  URL safety validation, authentication and read-only ownership are unchanged.

### Current development-source integration
- Normally integrate the approved exact `211d0f2a` engine 0.5.9 / registry
  library 0.9.2 / SDK 0.11.2 / index 0.6.3 / signed SSE 0.3.7 / native ABI
  0.14 selection while retaining P5 table/DCE fixes, P3 endpoint identity and P4 contracts,
  including commit-only query keys and their concurrent-render/SSR regressions.
  Only engine/AST/Cypher are path-selected; unused sibling SDKs are not consumed.
- Preserve P6 overlay/focus/theme/sizing/motion contracts, bounded row restart
  identity, paired native-animation observations and complete-graph SSR tests.
  Its active budgets and historical accessibility evidence are unchanged.
- Keep earlier `1284e9f` / library 0.9.1 / ABI 0.13 proof historical and require
  own rebuilt-runtime/packed/browser/live checks. The temporary engine pin is
  not a released fix, library-codec adoption or stored-data repair guarantee.

### Historical ABI 0.13 parent integration
- Normally integrate the approved server 0.2.3 / registry library 0.9.1 /
  host SDK 0.11.0 / signed SSE 0.3.6 (plugin SDK 0.11.1, native ABI 0.13)
  runtime while retaining the reviewed engine correction and all P3/P4 contracts.
- Keep original runtime fixtures and P1/P2/P3/P4/P5/P6 schema-2 measurements historical.
  Current accounting includes both shipped declaration formats, all package
  chunks/CSS and recursive Trading assets without changing coverage floors or
  the 2% growth policy. The original P5 tarball contains 37,834 ESM and 37,851
  CommonJS declaration bytes; original P6 ships 41,244 ESM and 41,261 CommonJS
  declaration bytes. Counting both adds no product bytes.
- Document that new archive/memory-budget controls are server/instance settings:
  the query read DTO is unchanged and `storageBackend` remains opaque JSON.
  Existing P5 composition and P6 modal/focus/theme/sizing/motion contracts are
  preserved; no P7 features or query/resource-creation defaults are introduced
  by this integration.
- Retain all 17 actual SSE 0.3.6 records from P4's own rebuilt normal-merge
  server separately, with manifest/lock/binary/signature provenance. Exercise
  the unchanged `sse034ResultAdapter` and default transport against them,
  including query-ID routing and aggregation before/after without `data`.
  The adapter name/API and P4 identity/recovery semantics do not change;
  native ABI compatibility is not a universal wire-format claim.

### Added
- P6 / #164 Part B provider-free, controlled `Modal` and `ModalProps`, backed
  by pinned Radix Dialog 1.1.15. Required nonempty accessible title rendered
  visually hidden; empty/whitespace titles throw `TypeError`. Optional
  description, typed initial/return/fallback focus refs, topmost dismissal,
  shared focus containment, pointer shielding and scroll ownership. Consumers
  supply visible close controls and application content; no tutorial chrome.
- Overlay-bounded focus visibility through pinned
  `scroll-into-view-if-needed` 3.1.0 and locked `compute-scroll-into-view` 3.1.1.
  Non-root focus inside the dialog is revealed only if needed, with nearest
  alignment and instant scrolling even when decorative animations are enabled.
  The maintained geometry helper does not replace Radix focus/lock/dismissal
  ownership or add a custom global focus manager.
- Scoped portal theme propagation from an inline anchor or `themeRef`,
  including resolved local `--drasi-*` values and typography/direction.
  Ancestor attribute, resize and preferred-color-scheme changes refresh it;
  arbitrary CSSOM/stylesheet replacement without those signals is not watched.
- Public `TableHeight` and table `ariaLabel`, plus headless
  `useReducedMotion` with a deterministic false SSR/no-`matchMedia` result.
- Three additional literal README recipes for numeric/token sizing, a scoped
  provider-free table/modal and headless reduced motion. Installed public
  contracts add positive/negative props/ref/height checks, import-boundary
  guards and open/closed modal SSR checks without a fake DOM. P6 gate
  counts and measurements are recorded in Trading TESTING.md; no human screen-reader
  review or universal browser/assistive-technology compatibility is claimed.
- Full-rule Trading accessibility non-regression checks bounded by exact
  predecessor element/state/browser/color/typography evidence. Original colors
  and image expectations are preserved; raw violations and incomplete checks
  remain visible. Generic/default-theme audits still require zero violations.
  Passing this check is not WCAG compliance or acceptance of the retained debt.
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
  P5 composition API; runtime graphs reject app-owned tutorial implementations while
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
- Sort controls are native buttons inside `th scope="col"`, with the column
  label as the button name and `aria-sort` on the active header. Action buttons
  retain labels and expose busy state; the action column has a hidden heading.
- Table viewports are named, tabbable regions for native keyboard scrolling,
  including when all columns are non-sortable and there are no row actions.
  Their focus-visible outline does not change unfocused table geometry.
- `height` is a validated size, not an additional class name. Migrate
  `h-[400px]` to `400` or `'400px'`; finite nonnegative pixels, explicit units,
  percentages, `'0'`, `'auto'` and `var(--token[, length])` are supported.
  Invalid JavaScript values throw `TypeError`. Explicit height overrides
  `style.height`; omission retains inline height or the inherited token's
  400px fallback. Use a token for `calc()`/`clamp()`, defining its value or
  providing a literal fallback. Syntax validation does not evaluate the CSS
  custom-property cascade during SSR.
- Package colors now have light `var()` fallbacks at usage rather than global
  dark defaults. Trading owns its exact body theme and CodeViewer styles;
  ordinary consumers need no Tailwind or source scanning. Row flashes derive
  success/danger/primary colors from tokens via `color-mix` at 20%/20%/25%
  against transparent; Trading's explicit original values retain its colors
  and timing. `--drasi-color-overlay` controls the default modal backdrop
  (70% black fallback) and is copied/reset with the known portal tokens.
- `--drasi-line-height` preserves explicitly supplied unitless typography
  across the portal: table fallback `1.5`, modal-content fallback `inherit`.
  Trading sets `1.5` on its own body. Copying a computed pixel line height
  alone does not preserve an ancestor's unitless scaling for differently sized
  descendants.
- `useRowAnimation` cancels animation timers under reduced motion while
  maintaining the latest baseline. DataTable also suppresses controlled maps,
  and package CSS disables animations/transitions. Trading preserves its
  ordinary 350ms FLIP behavior and completes reduced-motion transitions
  immediately without delaying live data.
- Trading's narrow shared `BaseDialog` adapter uses Modal while preserving
  app-owned cards, forms and actions. CodeViewer remains lazy/app-owned and
  uses pinned Radix Tabs 1.1.13 for tab keyboard/selection relationships; copy
  remains outside the tablist. Existing inspection reads, cancellations,
  snippets, UI links and business behavior are retained.
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
- P5 left package CSS byte-for-byte unchanged, including legacy app-used
  dialog/fullscreen rules and the then-existing `height` class-name contract.
  P6's bounded presentation changes above supersede that historical boundary.
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
- Human screen-reader acceptance remains pending; automated rule, DOM/ARIA,
  accessibility-tree and keyboard checks are not a substitute for actual AT
  review. Development evidence is not merge/release permission.
- Standalone examples/Storybook (#165), publication and repository transfer
  remain separate. The package stays private and backend/SDK pins are unchanged.
