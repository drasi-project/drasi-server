# Migration and retained history

[Package overview](../README.md)

## #119 bootstrap-to-connect-only migration

Consumers migrating from the extracted
[#119 Trading bootstrap behavior](https://github.com/drasi-project/drasi-server/pull/119)
must move **resource management**, not just rename an import. Mounting the
current provider, initializing a client or pressing Retry never creates,
updates, starts, stops or deletes an instance/query/reaction.

1. Move query creation definitions (`queries`, removed `QueryDefinition`) and
   reaction deployment definitions (removed `ReactionDefinition`, bind
   host/port/settings) to an application/operator provisioning path. Supply
   explicit `serverUrl`, `instanceId`, `queryIds` and
   `reaction: { id, endpoint }` to the connection owner instead. Do not choose
   the first discovered instance or derive a browser URL from a bind address.
   `QueryConfig` is a complete **read** type, not a replacement creation body.
2. Provision and start the known resources **before** connecting, or explicitly
   own a bounded recovery workflow. Trading's app-owned provisioner only
   handles eligible, REST-confirmed known-resource missing/stopped failures,
   then retries once. Authentication, incompatible membership, malformed
   payloads, missing instances and opaque network/SSE failures are not blanket
   permission to provision. Starting/bootstrapping resources need bounded
   waiting, not a duplicate start or overwrite.
3. Choose exactly one lifecycle owner. Use `DrasiProvider` for existing
   resources, or bind your owned client/state/retry with `DrasiClientProvider`
   as in [ownership and reconfiguration](connection.md#ownership-and-reconfiguration).
   An outer provisioner must handle cancellation, conflict/race behavior,
   allowlisted definitions and cleanup itself; the binding adds no second
   stream or hidden management mode.
4. Update old row/error assumptions with the
   [P4 migration](#p4--163-part-b-migration): typed errors, normalized results,
   required raw `getKey` plus validating `transform`, separate rendered
   `rowKey`, scoped retry and best-effort consistency. If a legacy stream
   needs `routeUnidentified`, select `createLegacyResultAdapter` explicitly;
   neither shape guessing nor legacy routing is enabled by default.
5. Apply the [P5 composition](#p5--164-part-a-migration) and
   [P6 presentation](#p6--164-part-b-migration) changes below. Keep Trading's
   provisioning, tutorial/inspection, query snippets and fullscreen behavior
   in Trading; none is a generic package export.

The [quickstart](getting-started.md#quickstart) is the connect-only target, not a bootstrap script.
Server-side **data bootstrap** for a continuous query (the observed
`enableBootstrap` field) is a separate concept and is not disabled or
implemented by this migration.

## P6 / #164 Part B migration

P6 completes the bounded presentation contracts on top of P5; it does not
change P3/P4 connection, result, raw-key, retry or state-slot semantics.

1. Replace the legacy height **class** prop: `height="h-[400px]"` becomes
   `height={400}` or `height="400px"`. Earlier length-like strings were merely
   added as class names and did not set a length. Move real classes to
   `className`. Review `style.height` precedence and percentage parent sizing;
   put `calc()`/`clamp()` in a custom property when using `height`.
2. Keep a visible table `title`, or supply `ariaLabel`. Sorting uses a native
   button inside each column header, not an interactive `<th>`; custom CSS and
   automation should target the button for activation and the header for
   `aria-sort`. Preserve the named viewport's native keyboard scroll stop,
   action labels and focus-visible styling.
3. Compose generic overlays with `Modal`/`ModalProps`. Supply controlled
   `open`, `onClose`, a meaningful `title` and a visible close/cancel control.
   Use typed focus refs rather than competing global key/focus/scroll handlers.
   Replace optional surface/overlay decorations only when the app supplies
   suitable positioning, scrolling, contrast and focus styles.
4. Set scoped theme tokens explicitly. Default components are light, not
   implicitly Trading-dark. Portals inherit from their inline source or
   `themeRef`, subject to the documented refresh signals. Trading retains its
   exact dark values on its own `body` and owns CodeViewer CSS.
5. Use `useReducedMotion` for app-specific transitions. Automatic row animation
   and controlled table animation maps already respect the preference; do not
   add a second animation lifecycle or block data updates on transition events.
6. Keep tutorial inspection, its asynchronous display/copy/retry/cancellation,
   and fullscreen layout app-owned. Trading's narrow `BaseDialog` adapter uses
   the same Modal; its CodeViewer uses app-only Radix Tabs **1.1.13**, with
   ArrowLeft/ArrowRight/Home/End automatic selection, Enter/Space activation,
   named tab/panel relationships and copy outside the tablist. These are not
   package tutorial APIs.

Trading's normal tables remain **400px**; fullscreen retains the **32px**
inset and `calc(100vw - 64px)` / `calc(100vh - 64px)` bounds. All eleven
queries, raw identity/projection rules, financial calculations, default-sort
discrepancy, snippets and server-UI links remain app-owned and unchanged.
P6's [normal P5 merge](https://github.com/drasi-project/drasi-server/blob/agentofreality-react-independent-examples/examples/trading/TESTING.md#p6-newer-main-development-source-propagation)
retains original/ABI 0.13/quality ancestry and requires its own rebuilt runtime,
never a predecessor binary. Bounded row tokens/phase, paired observations,
complete-graph SSR, controls, Trading design and strict legacy contrast remain.
No P5 budget allowance, P7 feature import, human acceptance or release permission
follows; the package stays private.

### Accessibility evidence and remaining acceptance

Automated rule scans (including axe), DOM/ARIA assertions, browser
accessibility-tree inspection and real-browser keyboard tests provide distinct
evidence; none is a human screen-reader review or universal browser/AT claim.
P6 gate counts and artifact/coverage measurements are recorded in
[historical P6 measured evidence](https://github.com/drasi-project/drasi-server/blob/agentofreality-react-independent-examples/examples/trading/TESTING.md#historical-p6-measured-evidence)
and [historical P6 artifact advance](https://github.com/drasi-project/drasi-server/blob/agentofreality-react-independent-examples/examples/trading/TESTING.md#historical-p6-measured-artifact-advance);
the owning P6 draft PR records its exact-head CI outcomes. The implemented
keyboard, modal, theme, sizing and motion contracts have that automated
evidence. Older P1-P5 passes remain historical, not substitutes for it or for
validation of subsequent changes.

Generic/default-theme audits require **zero automated violations**. Trading deliberately
retains its original colors: full-rule axe reports use a strictly bounded
predecessor comparison to fail new/worsened findings by element, state,
browser, colors and typography. The **19 pre-existing contrast fingerprints
across 86 exact contexts** and incomplete checks remain visible and
**unwaived**. A passing non-regression gate is **not** zero Trading violations,
contrast-clean, WCAG-conformance or a human AT result.

No actual human assistive-technology review is available for P6. Human
acceptance remains **PENDING HUMAN**; complete the reproducible **nine-part**
[manual screen-reader checklist](https://github.com/drasi-project/drasi-server/blob/agentofreality-react-independent-examples/examples/trading/TESTING.md#manual-screen-reader-checklist-pending)
with exact OS/browser/AT versions, date, revision and actual outcomes.
P7 documentation/example development is authorized while that review remains
open; no automation is human approval. Development readiness after measured
gates does not authorize merge, release or publication.

## P5 / #164 Part A migration

P5's [normal P4 integration](https://github.com/drasi-project/drasi-server/blob/agentofreality-react-independent-examples/examples/trading/TESTING.md#p5-newer-main-development-source-propagation)
retains original/ABI 0.13 history, table/DCE fixes and P3/P4 auth, protocol,
identity, recovery and consistency limits. No rebase or P6/P7 feature import.

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
   two views of the same rows. Pass `rowAnimations` to both, and for P6 repeated
   highlights also share `rowAnimationRevisions`; the controlled state takes precedence
   over their local `animateOnChange`. Mount optional inspectors only on demand
   and unmount them on close. Retry an inspector-local read without restarting
   the live socket; when its error is the provider's same non-null error object,
   use the provider's shared connection retry instead.

Keep the explicit CSS import. At P5, existing CSS and the legacy `height`
**class** contract were unchanged; P5 did not complete P6
focus/overlay/theming/reduced-motion/height work. Apply the P6 migration above
when moving past that historical boundary. There is no new example
app or Storybook in P5 itself; P7's [example workspace](getting-started.md#example-workspace)
is a later consumer of those APIs, not a backend protocol or publication change.

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
   [SSE format](reference.md#normalized-results-and-explicit-wire-adapters) (`sse034ResultAdapter`, also exercised with recorded 0.3.6);
   explicitly choose `createLegacyResultAdapter({ routeUnidentified })` for
   legacy formats. `_deleted` is not a normalized-state tombstone.
5. Distinguish query `status`/`stale`/`errorScope` from socket status. Keep useful
   last-good data visible when appropriate. Use the hook/subscription retry for
   a query REST refresh and `useDrasiClient().retry()` for shared recovery.
6. Do not replay overlapping snapshot/delta batches yourself. Respect bounded
   overlap recovery and its terminal exhaustion; stronger handoff guarantees
   depend on a shared backend cursor/resume contract.
