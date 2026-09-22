# Trading regression baseline

This is the P1 foundation for [#200](https://github.com/drasi-project/drasi-server/issues/200).
Its original behavior baseline is [#119](https://github.com/drasi-project/drasi-server/pull/119)
at `a2b648062a4c55e036d68b6f26bf73b4e773bcf1`; its current predecessor is the
separately approved [B1 prerequisite #204](https://github.com/drasi-project/drasi-server/pull/204)
at `650ae978b2d23832c2370f172d4a530bc76e4e0f`. It protects the existing Trading
application, not a second demo. P1 does not redesign its query, package API,
CSS or components. The reviewed engine/security/source-backed setup changes
come from B1, whose history and policies are retained.

**Default source integration is now required, not a diagnostic overlay.**
The gate uses the pinned compatible source described below; the original #119
failure recordings remain [historical defect evidence](#historical-119-reproduction),
never new golden expectations. Readiness for another development layer requires
the actual current-branch product gates. It is not a claim that all external
checks or the entire unused core workspace are green, nor merge authorization.

P2 ([#162](https://github.com/drasi-project/drasi-server/issues/162)) originally
built on P1 `a8dd2f68dab9fb7ccbd982dfb6a3f309e36f0059`. Its normal parent update
now incorporates exact P1 `74bebe2e585946ac52fce9b37473790b7c377bea`, including
the approved newer-main graph, temporary `211d0f2a` engine source and official
ABI 0.14 plugins below. Original P2
`3e9833ccb2a4c5fb00a4cf2a2bf1ab9e8c14ae97` and its prior normal-update
`5a24356384a96e396e468e44af9b339fb9005d3b` ancestry are retained.
The package still
connects to explicit existing references with GETs only; Trading owns automatic
setup in `src/drasi/ensureTradingResources.ts` and lifecycle orchestration in
`TradingProvider.tsx`. This does not change the business/visual baseline below.

P3 ([#163 Part A](https://github.com/drasi-project/drasi-server/issues/163))
originally built on P2 `3e9833ccb2a4c5fb00a4cf2a2bf1ab9e8c14ae97`.
Its current normal parent integration retains original P3
`a0569c2ae7b17f51996f431cf2264c636a19f3d1`, the earlier integrations and
endpoint-identity fix `5a198eb3d6b50ce61cdd306173698faa12e6682b`, and incorporates
exact updated P2 `d784446d7e2ca369c2ae0a3e3c560455d54369a6`.
No P4-P7 product code is imported.
It completes the public client/transport/type/entrypoint and material
configuration contract. P3's historical passes do not imply acceptance of the
later Part B identity, canonical-delta or snapshot/reconnect/stale contracts.
The Part B consumer changes and focused proof obligations are described below;
the measured historical evidence remains unchanged.

P4 normally integrates exact updated P3
`887ff332df02b6d42a56a97a853a3522ec7e42db`, retaining original P4 ancestry and
the commit-phase query-key correction at
`2b9890b1adb056bdb419cf94fdd6a701cef53479`. The complete identity, adapter,
reactive-projection and bounded-recovery contract stays intact. Current
`211d0f2a` / registry library 0.9.2 / SDK 0.11.2 / ABI 0.14 evidence must be
proved on this owning branch; earlier `1284e9f` / ABI 0.11 and 0.13 captures
and measurements remain historical. No P5-P7 product code is imported.

P5 normally integrates exact P4 `6bb523b65ed8ca6a090dcc92d4ff79eb7c12731d`
above its completed table/SSR/alignment and DCE head
`79748574285ab4f6cb1738f1c8a0b585bd8ab53c`. Its implementation, regressions,
budgets and historical records remain intact; only the approved parent runtime,
setup/provenance and current-versus-historical documentation are integrated.
P6/P7 features are not downported, and no later human acceptance is implied.

P6's presentation contracts and acceptance status are recorded
[separately below](#p6-presentation-contracts-and-evidence). The version-1
inventory, KB-04 accessibility limitations and P1-P6 original measurements
describe their historical layers; they are not current-runtime P6 proof.
P6's current normal update merges exact P5
`67c7a2fdea39f8fe13fd7f5394e17dd2543cb8ca` above frozen quality head
`bb071b0cf3ee4f509dd2834ecba488a27a26ef90`. Original P6
`959594b1ad1d8b0a891b9526d2376f30fe66182f`, its ABI 0.13 integrations and
subsequent row/observation fixes remain in history. No P7 features, original
design change, accessibility-policy waiver or human acceptance are imported.

## Behavior inventory, version 1

Paths in the assertion column are relative to `app/test`. A synthetic test
proves application behavior under an explicit transport contract, **not** that
Drasi evaluates a query or emits that contract. The live-server gate is separate.

| ID | Preserved behavior | Executable assertions / baseline limitation |
| --- | --- | --- |
| T01 | Existing `./start-demo.sh`, manual setup and URLs: app 5273, REST 8280, SSE 8281, price source 9100, Trading API 9200, PostgreSQL 5632 | `integration/tradingOptions.test.ts` locks app REST/reaction defaults; browser requests still use production URLs, redirected only by the test runner. B1's approved source-backed preparation is shared with startup/devcontainer routes; P1 retains those helpers without changing the app's automatic setup or URLs. |
| T02 | Fresh startup automatically creates all **11** queries and `sse-stream`; reload reuses resources | `integration/Trading.test.ts` and `browser/trading.spec.ts`, “fresh automatic setup” / “automatically provisions”; exact query definitions, joins, source IDs, reaction membership, single connection and no second mutation on reload. P2 also exercises partial missing/stopped resources and concurrent tabs, one stream per tab. Live gate is required separately. |
| T03 | Query IDs, `HAS_PRICE`, `ON_WATCHLIST`, `OWNS_STOCK`, `ORDER_HAS_PRICE`, numeric thresholds and P/L meaning | `integration/tradingOptions.test.ts`; known-value row and summary assertions in `Trading.test.ts`. Synthetic projections are illustrative and do not replace real query execution. |
| T04 | Watchlist add/remove, alphabetical rows, duplicate/write errors | Both app and browser CRUD assertions; the app suite also checks failed writes leave existing rows intact and show an error. |
| T05 | Portfolio add/edit/delete, validation and Total Value / Cost / P/L / Return | Both CRUD suites assert quantities, dates, request bodies, rendered prices and exact summaries after every operation. Duplicate-symbol identity remains a known limitation (KB-02), not an endorsed contract. |
| T06 | Limit-order buy/sell payload, expiry duration, create/cancel, filled/expired presentation | App/browser order assertions. Existing `drasi.trueFor` query expressions are protected; synthetic transport does **not** simulate the server's future-function scheduler. |
| T07 | Sector grouping, counts, aggregate change/volume, min/max price range | Fixed sector rows plus exact aggregate volume after price changes in both suites; deterministic fixtures include positive, negative and unchanged prices. |
| T08 | Gainers/losers filtering, ten-row cap, high-volume threshold, initial order and user sorting | `tradingOptions.test.ts` protects data-level sign/cap ordering; app/browser compatibility assertions explicitly characterize KB-01 and then exercise the requested sort, including keyboard activation. A price moving across a threshold removes its old row. |
| T09 | Live price changes, deletes, ticker and green/red value-change animations | App/browser live assertions, including CSS class removal after 500 ms. The browser uses native HTTP streaming/EventSource, not a fake browser EventSource. |
| T10 | Connection indicator and reconnect without a browser refresh | App/browser outage assertions close the transport, mutate/delete while disconnected, reopen it, and verify fresh snapshots replace stale rows. No polling or page reload is used to recover. |
| T11 | Tutorial query definition, React snippet, copy and Drasi Server UI link | Browser “compatible initial sorting…” checks both tabs, exact instance link and actual clipboard contents in Chromium. Firefox/WebKit exercise tabs/link but do not claim clipboard permission coverage. |
| T12 | Fullscreen expansion/collapse, value preservation and body-scroll restoration | Browser interaction assertions plus `watchlist-fullscreen.png`. Nested-overlay/focus/accessibility improvements belong to later layers; no claim of completed P6 semantics. |
| T13 | Existing dashboard/cards/dialog appearance and narrow responsive layout | `browser/visual.spec.ts`: five exact Linux/amd64 Chromium PNG comparisons, 1440x1000 desktop and 390x844 narrow viewport, plus explicit one-column card geometry. |
| T14 | Package remains reusable, private, built-artifact-only and owns its CSS | Package `test/NonTradingContract.test.tsx` uses composite-key telemetry rows with no `id` or `symbol`; identical-shaped queries stay separate. Clean tarball consumer checks ESM/CJS/CSS, rejects symlinks/source files and disables install/build lifecycle scripts. |

### Known baseline limitations (not refactor regressions)

- **KB-01 — market-mover defaults:** before extraction and at #119, `StockList`
  applies `changePercent` descending to all three panels. This reverses the
  intended losers ordering and overrides high-volume ordering. P1 preserves the
  actual initial order, names its compatibility assertion accordingly, and tests
  manual sorting separately. Visual baselines record that unchanged appearance,
  not approval of the business discrepancy. Do not “fix” the defaults in this
  refactor without separate approval.
- **KB-02 — duplicate-symbol identity:** the original portfolio displayed rows
  by symbol. Part B aligns its render/animation identity with the existing raw
  position ID (symbol fallback) and adds a duplicate-symbol render/delete test.
  Edit/delete actions still look up the first API position with that symbol;
  order query options also still prefer symbol to order ID. The original
  fixtures intentionally use unique symbols. Neither the original gates nor
  the focused identity test establishes duplicate-symbol action correctness.
- **KB-03 — legacy unidentified routing:** the app's adapter recognizes some
  snake_case shapes while current queries return camelCase, and can fan price
  rows out to several queries. Part B opts into a named legacy adapter once;
  identified official events bypass these heuristics. Unidentified before/after
  changes are delete-plus-upsert, and unknown rows now fail safely without
  console payloads. Synthetic batches do not validate the heuristic against a
  real SSE plugin. Recorded/live results must stay separate.
- **KB-04 — existing accessibility:** Trading dialogs lack dialog roles and
  associated field labels; Chromium's existing table header semantics differ
  from jsdom's. The browser suite locates existing headings/`th` elements where
  necessary. This is not an accessibility pass; #164 owns those improvements.
- **KB-05 — historical aggregate snapshot authority failure:** the original
  `portfolio-summary-query` REST result contains intermediate/historical rows,
  not one current aggregate. Reload/reconnect can therefore replace a correct
  streamed total with an old total. This reproduces on #119's exact server code
  and dependency lock, not just an older image. The approved B1 source
  prerequisite addresses this engine identity defect. The same live assertions
  remain mandatory, and historical records remain unchanged. No client-side
  selection heuristic or replacement financial computation was added.

## P2 ownership and negative cases

Package tests spy on every REST request across initialize, snapshots, reconnect
and explicit retry: all must be GETs under the same encoded instance path.
They cover query/reaction/instance absence, stopped/starting/error states,
reaction kind/membership, auth/network/opaque-404/bad-payload failures, finite
retry budgets, hung requests/streams, response-body network/timeouts versus
malformed JSON, abort and stale generations. Hook tests
assert the **same `DrasiError` object** reaches context, connection, query and
definition consumers, including invalid configuration and Retry controls.
`referenceTypes.ts` checks required references and rejects old deployment props.

Trading's `integration/provisioning.test.tsx` imports the **built package**, not
a source alias or mocked hook. It covers all 11 allowlisted definitions,
description-free Cypher POST bodies, ordered source/join semantics, no-op
existing resources, partial/stopped/starting states, shared concurrency,
409/read convergence and conflicts, Web Locks, individual/last-consumer abort,
partial success, the 60-second deadline, wrong-instance isolation and
non-provisionable failures. The existing business integration assertions remain.

The three browser engines additionally cover partial setup and native
concurrent first-run tabs. A separate existing-resource tab case deliberately
fails one SSE opening and requires recovery with no provisioning. These
lifecycle cases run their clocks so startup backoff can progress, retaining
the five-second connected assertions, exact 12 creations and one stream per tab.
Both cases also pass 12 consecutive WebKit repetitions (24 checks), keeping
fresh provisioning separate from the additional existing-stream retry scenario.
Independent package validation GETs overlap, and an app waiting on another tab's
setup returns after one fully-ready preflight instead of rereading the bundle.
Validation batches drain before reporting a missing resource, so an app retry
does not race a burst of aborted reads. Explicit cancellation still aborts all
requests. The partial-startup case also passes 12 WebKit repetitions.
Query creation remains serial; source/join order is unchanged.
The original five PNG files are unchanged. The frozen-clock
reconnect test now advances the retry clock while the new asynchronous REST
classification completes, under the same five-second assertion bound. Its
connected/fresh-row/delete/no-navigation assertions are not relaxed.

The real-server gate retains all business actions and now asserts singleton
aggregate snapshots explicitly at 2000 / cost 1800 / count 2, reload 2050,
and offline/reconnect 2150, rejecting historical candidates. In native runs,
the harness still translates only the owned reaction's bind host/port to
ephemeral loopback values. P2 reverses that same translation in its full-view
response so the app can validate its desired definition. Status, membership,
query definitions and financial rows are untouched; raw diagnostic REST reads
still record the actual isolated bind values.

## Fast checks

### P6 presentation contracts and evidence

P6 / #164 Part B adds bounded keyboard, modal, theme, sizing and reduced-motion
contracts to P5 composition. It retains every original query, raw key,
projection, financial calculation, provisioning rule, link and tutorial.
The known default-sort discrepancy is still frozen. No standalone example,
Storybook or publication is included. The original P6 feature did not change
backend/SDK/plugin selections; its approved normal parent integration now
inherits the current runtime separately, as recorded below.

The package reference documents
[Modal props and focus ownership](../../dev-tools/react/README.md#modal),
[portal theme signals and limits](../../dev-tools/react/README.md#scoped-themes-and-portals),
[size validation/precedence](../../dev-tools/react/README.md#table-sizing)
and [live reduced motion](../../dev-tools/react/README.md#reduced-motion).
Trading keeps normal 400px cards, fullscreen 32px insets/bounds and default
350ms FLIP transitions. Reduced motion completes transitions without waiting
for animation frames/timers; it does not stop query updates.

#### Repeated row highlights

The #209 quality follow-up restarts successive up/down/string highlights
without changing stable row identity, DOM nodes, child state or focus.
`useRowAnimation` exposes readonly per-row `revisions` alongside its existing
direction map. Trading's one tracker supplies both `rowAnimations` and
`rowAnimationRevisions` to the normal and fullscreen DataTables. Standalone
`animateOnChange` uses the same mechanism automatically; the hook stays
headless and provider-free tables need no query or Trading data model.

The renderer alternates equivalent keyframe names on changed committed tokens,
not token parity: batched updates may skip numbers. The 500ms ease-in-out
profile, success/danger/primary color mixes and expiry after the latest change
are preserved. No forced layout read, DOM replacement, perpetual animation or
animation-end dependency is introduced. Removing rows, unmounting or enabling
reduced motion clears pending state/timers while current values keep updating.
CSS keyframe names are internal; documented theme variables and direction
classes remain available.

`rowAnimationRestart.test.tsx` covers repeated directions, shared owners,
unchanged/independent rows, skipped tokens, StrictMode, retained input state and
focus, expiry and cleanup. The built-package Trading composition test also
checks successive same-direction changes in both actual presentations without
another subscription or read. `row-animation.spec.ts` observes native browser
Animation objects over five updates separated by a deliberate 200ms cadence,
including a point after the original one-shot animation would have finished.
The pre-fix up/down browser runs retained their classes but had no animation
on later updates; the corrected tests require a new live animation for each
update, unchanged DOM/input/focus, eventual expiry and no reduced-motion
animation. Full-rule generic axe reports accompany the default-theme cases.
These are automated observations, not human screen-reader results.

The inherited en-US Node / sv-SE browser hydration fixture executes its complete
generated SSR module graph. P6's existing top-level `React.lazy` initializer
retains a separate client-only ModalLayer chunk even when the Modal export is
unused; P5's one-file SSR setup therefore stopped before running Node. The P6
fixture writes every emitted chunk/asset to an owned temporary directory and
executes the single entry normally, then removes that directory. It does not
flatten or eagerly load the client layer, skip hydration, replace the original
root, or relax warning/error, locale, keyboard or alignment assertions.

The distinct active P6 size baseline has **no P5 allowance**. The exact reviewed
P5 approval from `e1954c0d40fecf640445ff213b356ed01e6505f9` is retained separately
in `baseline-metrics-p5-quality-v3.json`, protected by a byte-identity test.
Its original 75725-pass/75726-fail boundary and forgery/non-compounding tests
continue against that historical fixture. Explicit P6 tests enforce the
unchanged 2% cap for every metric and reject carrying over the approval.
Original P1-P6 schema-2 records, all five visual images, contrast fingerprints,
coverage floors and the open human checklist are unchanged.

P6 additionally enforces the existing critical thresholds on
`src/components/**`: **90% statements, lines and functions / 85% branches**,
alongside the unchanged client/react thresholds. Coverage includes all package
product source, with no exclusions added for the modal, portal theme or other
presentation code. Current module measurements and final counts are not frozen
by this policy statement.

The following is the **required evidence inventory**. Measured P6 outcomes
follow below; exact-head CI is recorded in the owning draft PR and issue #164.
Do not substitute P5's historical counts or assume that an added assertion
has run successfully.

| Evidence | Contract it must establish |
| --- | --- |
| Package `DataTable`, sizing and accessibility regressions | Native sort buttons, one Enter/Space request per action, `scope`/`aria-sort`, table/action names and busy state; named tabbable viewport with native scrolling even without interactive cells; numeric/unit/token heights and invalid-value rejection; supplied/query state contracts preserved. |
| Package Modal tests | Provider-free controlled naming/description, initial/fallback/return focus, Tab/Shift+Tab containment, topmost Escape/outside dismissal, pointer shielding, shared scroll cleanup, nested/independent owners, StrictMode and unmount. |
| Package motion tests | Live preference changes, automatic timer cancellation/latest baselines, controlled animation suppression and no animation-dependent data processing. |
| Built-package Trading integration | Shared BaseDialog interoperability and field labels; fullscreen/code nesting and cleanup; app-owned Radix tabs; delayed display/copy, cancellation, retry and lazy-read boundaries preserved. |
| Installed public contracts | Dual-format declarations and NodeNext/bundler positive/negative types; `ModalProps`, typed refs/callbacks, `TableHeight`, `ariaLabel`, `useReducedMotion`; no public ambient `any` or source aliases. |
| Installed dependency/import/SSR checks | `/client` installs with `--omit=peer` without React/runtime types; `/react` stays free of Radix, components, React DOM and CSS. Modal open/closed SSR omits portal content without a fake DOM; motion SSR is false. |
| Installed literal README recipes | All original ten recipes, including the first five, retained literally; added numeric/token sizing, scoped provider-free table/modal and headless motion examples compile from the actual tarball. Negative-assertion counts are measured from the programs, not hard-coded. |
| Real-browser presentation checks | Chromium/Firefox/WebKit keyboard/DOM behavior, nested/independent dismissal and focus/scroll cleanup, local and portal themes, hostile general host styles, narrow sizing and reduced-motion changes. Record browser-specific outcomes, not assumed parity. |
| Existing visual and product gates | The same five original exact-zero-diff PNG comparisons, original behavior scenarios, financial CRUD/reconnect expectations, coverage floors, artifact policy and actual-backend gates. No expectation refresh or reduced assertion substitutes for a passing run. |

Keep evidence categories distinct:

- **axe/rule scans** detect only the automated rules exercised in the rendered
  state. Active-modal scans explicitly scope to the active dialog; dashboard
  scans cover the whole document with all rules enabled. Retain the scope
  attachment with each raw report and its failures. A scoped dialog result
  is not a whole-page pass, and a clean scan is not a WCAG conformance claim.
- **DOM/ARIA assertions** inspect roles, names, relationships, hidden background
  and attributes. They do not prove what any screen reader actually announces.
- **Accessibility-tree inspection** records browser-native snapshots in
  Chromium, Firefox and WebKit, separately from DOM `ariaSnapshot` output;
  Chromium also has a full CDP tree attachment. These are browser-exposed
  trees, not human/platform AT results. Preserve engine differences rather
  than inventing normalized names or claiming identical spoken output.
- **Real-browser keyboard checks** exercise actual focus and input behavior in
  the recorded engines. They do not establish universal browser/version support.
- **Human AT review** requires an actual screen reader and a human recording
  announcements/usability. No such P6 review is available; acceptance is pending.

P1-P5 historical results, the original five visual PNG images and their
zero-diff requirement stay intact. P6 artifact/coverage measurements must be
recorded after the same gates without lowering inherited thresholds or
excluding moved code. Current development readiness, once measured, is not
merge, release or publication permission.

The expanded predecessor comparison uses exact
`569b1d26e558b838d3a572bf7e2e5fbd6280eeaf`: 171 archived source blobs were
verified, the predecessor was built, and its compiled tarball was consumed
without source links. Paired Linux Chromium/Firefox/WebKit full-rule audits
retain 47 strict state/browser pairs and two separately recorded natural
post-click observations, producing 98 raw reports. All 19 distinct failing
element/state entries in the strict matrix are unchanged from the predecessor.
Foreground/background colors, fonts, rendered state and browser versions are
recorded rather than inferred from matching PNG images.

| Measured Trading text/control | Contrast ratio (required: 4.5) |
| --- | --- |
| Sell badge / footer | 3.81 / 3.98 |
| Add, Create Order, Save | 2.97 |
| Selected Buy / Sell | 2.53 / 3.76 |
| Buy/Sell order hint | 3.66 |
| Delete / Cancel Order confirmation | 3.90 |
| Drasi UI link / active code tabs / copied feedback | 4.13 / 4.16 / 4.34 |
| Stable primary-button hover | 4.03 |
| Matched WebKit transition samples | 3.97 / 3.28 |

`app/.test-runtime/p6-predecessor-569b1d2/colour-approval-matrix.json` is the
expanded inventory; the earlier `comparison.json` is only its two-target
dashboard subset. Provenance, raw reports, screenshots and traces are retained
alongside it. Clipboard success was exercised in Chromium only. The React
example's added `height={400}` line is disclosed. WebKit transition comparisons
seek the existing native animation to the same time with identical keyframes,
easing, hover and focus; no color declarations are rewritten. Unmatched
natural transition phases are not called equivalent.

These measurements establish preexistence, **not acceptance**. All current
contrast violations and incomplete checks remain visible and unwaived.
Untested states are not classified. No color or image expectation change is
authorized by this inventory; that decision and human AT review remain
separate. It is not a zero-violation Trading audit.

The coordinator's disposition preserves the user's original exact-design
requirement: keep these colors and test **no new or worsened Trading
accessibility violations**, not a zero-contrast result. The full-rule audit
still runs `color-contrast`; `trading-axe-baseline.json` matches only 19
documented fingerprints in 86 exact element/state/browser contexts. Matching
also requires the recorded selector, semantic identity, impact, foreground,
effective background, font metrics and rendered typography; lower contrast,
new rules/elements, duplicate bad elements and unrecorded contexts fail.
Known violations are retained in the report rather than deleted or downgraded.
Missing violations are allowed as improvements, not fabricated as observations.
Generic component/default-theme audits still require **zero violations**.

The authoritative regression environment is the pinned Linux browser image.
Computed font-family serialization can differ on another OS; an unmeasured
platform is not silently normalized into the approved boundary. Stable-state
Trading audits move the pointer away and await existing finite CSS motion;
no assertion deadline or business/visual clock policy is increased. Two
separately configured WebKit cases exercise the exactly measured native hover
and transition phases. Other engines do not get empty or expected-fail tests
for those unmeasured phases.

`test/fixtures/p6-predecessor-axe-evidence.json.gz` durably retains all 98 raw
reports, violations, incomplete results and source/state provenance. The
provenance test verifies file hashes and every regression context against
those measurements; positive/negative policy tests reject broader matching.
CI retains this archive, compact evidence and actual new axe/non-regression
reports. A passing regression check must be described as **no new/worsened
Trading a11y regressions under preserved legacy colors**, never contrast-clean,
full WCAG compliance, a waiver or human AT acceptance.

The Linux helper and React workflow retain the installed public-contract
proofs, compiler-resolution traces and generated programs alongside browser
reports and coverage. Dependency installation directories are excluded from
these evidence copies; the checked manifests/locks and proof records remain.

The focus-visibility implementation adds two locked runtime packages:
`scroll-into-view-if-needed` 3.1.0 and `compute-scroll-into-view` 3.1.1. This is a
maintained, overlay-bounded geometry operation, not a replacement for Radix's
focus/scroll-lock/dismissal ownership. Its nearest, if-needed reveal is instant
under both default and reduced motion. Revalidation must check viewport
visibility and unchanged background scroll, including long content and
transformed/fullscreen layouts. Byte accounting includes every emitted
entry/shared/lazy chunk; the helper must not be hidden from the artifact
comparison by measuring only the small public Modal wrapper.

Native paging checks keep PageUp/PageDown down until actual movement is
observed, then release in `finally` and await the native `scrollend` event.
This avoids treating a compositor plateau as a completed gesture or a
zero-duration synthetic key press cancelling WebKit's paging;
the test never sets `scrollTop` to manufacture movement or increases the
existing five-second assertion bound.

#### Historical P6 measured evidence

This subsection records original P6
`959594b1ad1d8b0a891b9526d2376f30fe66182f`, its earlier server 0.2.1 / ABI 0.11
runtime and original measurement convention. It is preserved history, not a
substitute for rebuilding and testing the newly integrated parent.

Local verification uses actual React 18.3.1 with Node 22.20.0 in the pinned
Linux image and Node 24.19.0 in a separate clean consumer. Package tests are
**413**, Trading tests **219** (including 102 strict contrast-policy cases),
and Node tooling checks **8**. The installed checker runs nine public type
programs with **630 measured negative assertions**, 13 literal README recipes
(all original ten retained), nine parser guards and eight ESM/CJS import/SSR
modes. Client-only installation still omits React and its runtime types;
all entries, shared/lazy chunks and dual declarations are inspected.

Before the explicit preserved-design policy, the full packed Linux run was
146/152: its six failing Trading audit scenarios exposed the retained contrast
findings, not ignored failures. All five original image comparisons and other
browser cases passed. The subsequent strict Trading-only regression run passes
29/29 actual Chromium/Firefox/WebKit cases, including all measured control
families and the two WebKit hover/transition cases. The final complete
source-free Linux run passes **175/175** browser cases, including every
original behavior scenario and all five unchanged, zero-diff PNG images.
All raw Trading violations and incomplete checks remain reported under the
strict non-regression policy; this is not a contrast-clean result. Exact-head
CI is recorded separately in the owning draft PR.

The source-free tarball frontend also passes the actual checked-out backend
gate: fresh setup, existing no-write reload, financial singleton
2000 / cost 1800 / count 2, live 2050, reload 2050, offline/reconnect 2150,
CRUD/live/deletes and no recovery navigation or historical-row selection.
The backend UI/server were built in the owning checkout; raw Rust totals are
779 passed / 32 ignored, tooling 42 passed, strict Clippy/fmt passed, and
structured audit output reports zero vulnerabilities with 15 existing warnings
(10 unmaintained, four unsound, one yanked). This does not clear unused legacy
workspace advisories or the separately retained external/configured caveats.
Both owned live runs cleaned their processes/containers; only their exact
temporary runtime directories were removed after safe evidence retention.

Coverage below is statements / lines / functions / branches:

| Scope | P6 measured coverage |
| --- | --- |
| Client | 97.73 / 99.10 / 99.34 / 95.27 |
| Hooks | 98.88 / 100 / 100 / 94.28 |
| Components | 97.86 / 100 / 100 / 95.74 |
| DataTable | 100 / 100 / 100 / 96.31 |
| Modal public boundary | 100 / 100 / 100 / 100 |
| Modal client layer | 93.10 / 100 / 100 / 90 |
| Portal theme, sizing, reduced-motion helper | 100 / 100 / 100 / 100 |
| TradingQueryTable | 97.36 / 100 / 100 / 93.33 |
| CodeViewerDialog and BaseDialog | 100 / 100 / 100 / 100 |

Four targeted regression probes failed on P5 production code before
implementation: a CSS length was an ineffective class, sorting lacked a native
button, nested Escape collapsed both layers, and the form had no modal focus/
role behavior. Those observable cases now pass. Additional browser findings
were fixed at their causes: lost portal font weight, pixel-versus-unitless
line height, responsive SVG shrink behavior and offscreen wrapped focus.
Original image expectations were never refreshed to bless those changes.

#### Historical P6 measured artifact advance

The same 2% future-growth rule remains; this is an explained feature baseline,
not a percentage increase or coverage exception. P1-P5 history is retained in
`baseline-metrics.json`. The pinned Linux measurement includes all emitted
entry/shared/lazy files, not only the initial app chunk.

| Bytes | P5 | P6 feature measurement |
| --- | ---: | ---: |
| Tarball | 169457 | 204840 |
| Package ESM | 78523 | 89246 |
| Package CJS | 85845 | 98118 |
| Declarations, original `.d.ts`-family counter | 37834 | 41244 |
| Package CSS | 10043 | 8616 |
| Trading JS | 251684 | 304459 |
| Trading JS gzip | 74133 | 92744 |
| Trading CSS | 21639 | 25266 |
| Trading CSS gzip | 5212 | 5900 |

The 52775-byte app JS increase (18611 gzip) includes maintained Dialog/Tabs
primitives, bounded scroll geometry and P6 adapters instead of an untested
focus system. Public props/contracts and complete shipped reference explain
declaration/tarball growth. Tutorial CSS moves to Trading; namespaced defaults,
focus outlines and theme/motion contracts explain the stylesheet changes.
The client-only and hook import boundaries remain independent. Browser-only
fixture bundles and recorded audit archives are not production app assets.

#### Historical P6 ABI 0.13 parent integration and accounting

This section records the earlier `1284e9f` / ABI 0.13 proof, not validation
of the newer source/runtime selection below. The owning P6 branch merged exact P5
`bb0bec99f06e12f3fbe9cd2bf0eee653763f851e`, preserving original P6
`959594b1ad1d8b0a891b9526d2376f30fe66182f` ancestry and the existing PR base
ref. P6 presentation/runtime source, all eleven Trading queries and transforms,
the five original images, the exact 19-fingerprint/86-context contrast policy
and its 98-report/201-JSON archive remain unchanged. Human review stays open;
neither integration nor the accounting correction supplies new AT evidence.
No P7 code or examples are imported.

All five incoming original schema-2 histories remain byte-identical.
`baseline-metrics-p6-v2.json` additionally preserves the exact original P6
record (SHA-256 `8ff7a240f83d55c926517afcd9865c9181eb40451e3fb476e211aab086834389`).
The verified original P6 tarball SHA-256 is
`148f5df78906b18fc6d9d328faad2efc869d6cfd66cac6e38111013beb7f25ca`.
It contains **41,244 ESM plus 41,261 CommonJS declaration bytes: 82,505 total**.
Counting both existing formats changes accounting, not product bytes.
Corrected historical P1-P5 declaration totals remain
37,492 / 42,280 / 56,256 / 69,767 / 75,685; P6's actual declaration feature
cost relative to corrected P5 is 6,820 bytes. The original runtime/CSS/app
feature budgets, all coverage floors and the same 2% future-growth rule remain.
The current helper still counts every entry/shared/nested/lazy JS/CJS file,
all declaration formats, every packaged stylesheet and recursive app assets.

That historical proof used the owning checkout's newly built default server/UI:
server 0.2.3, registry library 0.9.1, host/plugin/FFI crates 0.11.0, index 0.6.1,
GQL 0.3.6 and unchanged core/AST/Cypher correction
`1284e9f648634c1faa73fd897a21c2712bb0cbbe`. The six incoming immutable plugin
locks use official merged release
`3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`; plugin SDK crate 0.11.1 and host
crate 0.11.0 share actual native ABI 0.13. Old ABI 0.11 and newer ABI 0.14 are
not fallbacks. Signatures, hash/target/main-identity checks, foreign/dirty-sibling
guards and `prepare-build.sh` remain intact. Current DTO and 17-line SSE 0.3.6
records stay separate from their historical counterparts.

Pre-freeze merged checks pass: **416 package tests, 226 Trading tests,
18 Node policy/provenance checks and 53 repository tooling tests**. The owning
server/UI was rebuilt before the locked Rust suite: **809 passed, 32 existing
ignored**, with strict Clippy/fmt and audit zero vulnerabilities / the same
15 warnings. The complete original **175/175** packed Linux browser matrix
and all five original exact PNG images pass under the corrected accounting.
An additional three-engine regression covers fully opaque content while its
own overlay is still fading; the audit now observes both within the unchanged
five-second bound and observes native paints after a completed transition
before capturing computed colors. It does not accept a transient color, recapture fingerprints
or alter runtime animation. The rejected intermediate WebKit raw audit/trace
is retained, rather than treated as a passing run.

Clock preparation also avoids a pre-navigation install/pause protocol race:
on the blank page only, installation starts before the original anchor and
`pauseAt` then establishes exactly the same `FIXED_TIME` before app code loads.
No application time is advanced during this preparation. The visual clock
still starts paused, lifecycle cases still explicitly resume it, and relative
timer/recovery assertions and five-second bounds are unchanged. A scheduling-gap
regression verifies the exact anchor across navigation, relative timers and
subsequent resume; original image comparisons remain authoritative.

The integrated Linux artifact measures **206,779 tarball / 89,246 ESM /
98,118 CJS / 82,505 dual declarations / 8,616 package CSS / 304,459 app JS
(92,744 gzip) / 25,266 app CSS (5,900 gzip)** bytes. Runtime, declaration and
CSS bytes match the remeasured original P6 artifact. The 1,939-byte tarball
increase is documentation (0.95%), within the original 204,840-byte budget's
unchanged 2% future-growth rule; the budget is not raised.

The merged owning-head validation and CI outcomes are recorded on #209 and the
Part B evidence in #164. Old runtime/binary passes, or borrowing a lower layer's
binary, are not substituted for the current own-source and independently packed
frontend gates.

The [retained external caveats](#product-gates-versus-retained-caveats) remain:
historical YAML-agent HTTP 400, configured plugin skips, the missing optional
`build-dynamic` target and unused-core advisories are not passing checks or
reasons to change models, credentials or pins.

#### P6 newer-main development-source propagation

The current merge retains all P6 quality inputs while inheriting exact #208
`67c7a2fdea39f8fe13fd7f5394e17dd2543cb8ca`: clean temporary engine source
`211d0f2a79aa2ad0f7cb841937f52013fe95ded6`, registry library 0.9.2,
host/plugin/FFI 0.11.2, index 0.6.3 and the six official `70ca432c` signed
native ABI 0.14 locks. Only engine 0.5.9 / AST 0.3.5 / Cypher 0.3.6 use paths;
the sibling's equal-version SDKs remain unused. See
[runtime provenance](../../docs/main-runtime-integration.md). This source
choice is unreleased development, not the library codec fix, legacy-data repair
or core merge/release authorization.

The original 181 cases, nine table/SSR cases and 30 row/boundary cases remain
the original **220-case** gate, with all five exact PNG images. This includes
the private generation scalar, opaque tokens, last-active phase/first-pulse
sentinel, paired primary/mirror native samples at the existing second frame,
500ms pulses, 200ms test cadence and complete-module-graph SSR fixture.
Runtime styling, business data, readiness bounds, paint observations and
application-visible clock advances are unchanged.

Two fixture corrections preserve those product requirements. Native row
observations distinguish exactly one named 500ms CSSAnimation from the existing
optional 150ms background-color CSSTransition; unexpected objects, names,
targets or timings still fail. A real pointer-hover coexistence control and
focused negative assertions protect this distinction.

Pinned Playwright 1.56.1 replays the install/pause protocol gap into monotonic
time after navigation even when wall time is correct. Clock preparation now
replays into a fresh `about:blank`, aligns public performance time to its 16ms
frame boundary, then restores the original wall anchor with `setSystemTime`.
No application code runs during alignment; normal navigation and the original
5000ms advance, ticker speed, deadlines and images are unchanged. Four real
setup-gap/frame-count probes in each engine add **12 cases**, bringing the
complete gate to **232**. They require the same wall anchor before/after
navigation and exactly 312 callbacks, with first/last-frame assertions.
Earlier 215/220 and isolated ticker-image failures remain retained evidence,
not passes or diagnosed host flakes; final outcomes belong to the exact-head
PR record.

All original feature budgets, complete artifact accounting and coverage floors
remain. P6's active baseline has no allowance; the exact P5-only 110-byte record
stays historical with its boundary and reuse-rejection tests. Fresh proof must
use this checkout's rebuilt UI/default binary and actual source/pin/hash
provenance, including fresh/no-write setup, CRUD and unfiltered singleton
2000/cost 1800/count 2 -> 2050 live/reload -> 2150 offline/reconnect.
Prior package/browser/runtime passes are not relabeled as current execution;
owning-head results and fresh CI checkout-tree equality are recorded on #209.

Parent #208 was accepted on first-attempt full CI
[35768828704](https://github.com/drasi-project/drasi-server/actions/runs/35768828704):
35/35, original images and its own rebuilt live gate. Its two local full
34/35 runs missed the WebKit concurrent-tab Connected assertion's original
five-second bound; one isolated pass did not diagnose or fix the cause.
Those failures remain a timing caveat, not an all-local-green claim or waiver
for this layer. No readiness limit was broadened.

Human AT remains open; full-rule Trading reports retain the exact 19 findings
and 86 contexts, raw violations and incomplete checks. Generic/default audits
still require zero violations. Current top-only YAML workflow skips, if present,
are not validation or a new HTTP 400/504 result. Host audit warnings, limited
plugin/legacy-smoke scope and publisher-visibility caveats remain explicit.

#### Manual screen-reader checklist (pending)

**Status: pending; no actual human screen-reader test is claimed.** A reviewer
can use NVDA with Firefox or Chrome on Windows, and VoiceOver with Safari on
macOS, or record the actual chosen supported combination explicitly. Browser
automation and an accessibility-tree snapshot must not be relabelled as either
of those human runs.

For each combination record: review date, exact revision, OS/version,
browser/version, AT/version, preference/viewport/zoom, steps, actual spoken
output and focus behavior, and pass/fail/blocked/not-run for every item.
Record defects and limitations rather than replacing outcomes with a generic
"accessible" label.

##### Isolated component preview

No preview service is started automatically. A reviewer can run the following
from this checkout to open the existing **test-only, provider-free** fixture.
It does not connect to Drasi, PostgreSQL, Trading's API or external endpoints;
the opt-in server mode refuses the Trading root and API routes. Use a free
loopback port, keep the command in the foreground and stop it with Ctrl+C.

```bash
npm --prefix dev-tools/react ci --ignore-scripts
npm --prefix dev-tools/react run build
npm --prefix examples/trading/app ci --ignore-scripts
npm --prefix examples/trading/app run --ignore-scripts build
cd examples/trading/app
npm exec -- vite build --config test/browser/consumer/vite.config.ts
P1_WEB_PORT=18333 P6_COMPONENTS_ONLY=1 node --import tsx test/browser/server.ts
```

Open `http://127.0.0.1:18333/__components/` for the controls or
`http://127.0.0.1:18333/__components/?case=modal` for modal ownership. Port
collisions fail rather than reusing another service. This supplies an isolated
route for genuine human review; it is not evidence that anyone performed it.
Trading-specific forms/code-viewer review remains a separate checklist item
in a reviewer-owned Trading environment using its documented startup path.
Do not toggle global accessibility/VoiceOver settings or interact with someone
else's desktop to manufacture a result.

1. Navigate the dashboard and a named table with the screen reader. Confirm
   column headings and the table name; activate a sort button by Enter and
   Space. Check one sort change, sensible `aria-sort` announcement and stable
   focus. Check names/disabled/busy state for row actions. Focus the named table
   viewport and check native Arrow/PageUp/PageDown scrolling in the AT's
   appropriate interaction/pass-through mode, including a non-sortable table
   with no row actions.
2. Open a Trading form, the fullscreen table and CodeViewer by keyboard.
   Confirm each dialog name (and short description when supplied), initial
   focus and associated input labels. A visible close/cancel control must be
   discoverable without pointer input.
3. Tab/Shift+Tab through each modal. Confirm containment, visible focus and
   background isolation using both ordinary focus navigation and the AT's
   browse/navigation commands. Check scrolling within long content. At a small
   viewport height, wrap from first to last control and back; the focused
   control must stay visible. Trading BaseDialog uses a naturally sized card
   and scrollable outer overlay, so exercise that layout as well as the
   default package modal. Start from a nonzero background-page scroll position
   and record it before and after these focus moves; revealing a control must
   not scroll that page.
   Repeat with default and reduced motion; the focus reveal itself is instant.
4. In CodeViewer, check the named tablist, selected tab and linked visible
   panel. Exercise ArrowLeft/ArrowRight/Home/End automatic selection and
   Enter/Space. Tab to Copy outside the tablist; verify it copies the currently
   displayed content when clipboard permission is available. Record denial
   as a limitation, not a successful copy.
5. Open CodeViewer over fullscreen, dismiss the top layer by Escape and by an
   outside primary pointer press, and verify the lower layer stays open.
   Where a dismissal option is disabled, it must not close another layer.
   Confirm background controls do not activate through the overlay.
6. Close by the visible control, Escape and outside press; check focus returns
   to an eligible opener/explicit target. Repeat with a removed, disabled,
   hidden or inert opener (change it in the browser's element inspector) and
   the configured fallback. With a surviving dialog, focus must stay there; after final
   close/unmount, verify page scrolling and focus are restored.
7. Allow live updates while sorting, using forms and inspecting a query.
   Check that focus is not stolen and last-good/error/retry notices remain
   usable. Table cell changes are not automatic live regions: record what is
   actually announced and any application announcement need, not an invented
   continuous-update announcement guarantee.
8. Repeat expansion/collapse and row updates with reduced motion active, then
   change preference during a transition. Check immediate stable layout,
   no continuing flash/transition and uninterrupted data updates.
9. Repeat relevant steps at the existing 390x844 narrow viewport and with the
   reviewer's recorded zoom. Check dialog/table scrolling, reachable close
   controls, focus visibility, readable labels and local/portal theme contrast.

Do not mark human acceptance complete until those versioned outcomes are
available. Automated passing counts alone cannot close this checklist.

### P5 composition separation

P5 ([#164 Part A](https://github.com/drasi-project/drasi-server/issues/164))
starts at the exact P4 head `20561c13dd74929855dfbe605bb1fac23e5a49f4`
in predecessor #207. It separates provider-free `DataTable`, headless query
state and the small composed `QueryTable`. Trading's `TradingQueryTable`
owns the existing fullscreen transition and shares sorting/animation across
two instances of the same presentation. `QueryInspector` and
`CodeViewerDialog` are app-owned; the package no longer ships tutorial code.
The package stylesheet and all original visual PNG images remain unchanged.

`DataTable.test.tsx` uses frozen, non-Trading rows with no provider. It covers
typed computed cells, actions, headers/custom rows, stable row state,
animation, null/number/string/hidden-field ordering, controlled precedence,
uncontrolled defaults/clearing and one notification per action in StrictMode.
`QueryTableComposition.test.tsx` uses the real provider/client rather than
mocking hooks. Loading/empty/retained/error slots expose full query state;
query-local retry preserves another table's subscription and shared retry
returns to the provider. No protocol, raw-key/projection or financial behavior
is reimplemented in the presentation layer.

The portable `composition-regressions.test.tsx` was first run against the
exact P4 checkout, before implementation. All three assertions failed:
there was no provider-free `DataTable` export, a plain table made three
non-cancelled definition/full-view reads rather than the two required
resource-validation reads, and three sort actions notified six times under
StrictMode. The identical checks pass on P5. Required initialization and
subscription resource validation remains intact; removing those GETs would
not be an acceptable way to make the inspector assertion pass.

Trading's `integration/tableComposition.test.tsx` tests the built package.
It covers closed-inspector read isolation, delayed success updating an open
viewer and its actual copied text, safe failure/retry, close cancellation and
fresh reopen, instance-scoped server-UI links, fullscreen shared sorting,
live animations/actions, collapse buttons/backdrop/Escape and unmount cleanup.
Definition-read retry does not bounce a healthy stream; a shared error uses
the connection owner's retry. Existing CRUD/provisioning/result tests remain.

The original three-browser behavior scenarios and five visual comparisons
remain the gate. The tutorial screenshot now waits for its actual lazily
loaded `OWNS_STOCK` query text before comparing the same image; it does not
capture a loading placeholder or refresh a baseline. The existing lifecycle
versus frozen visual-clock policy, five-second readiness assertions, forced
503, concurrent/partial setup, financial values and no-navigation assertions
are unchanged.

P6 still owns focus containment/restoration, topmost/nested overlay ownership,
shared scroll locking, complete keyboard/tab semantics, local/portal theme
tokens, reduced motion and the height API. Single-overlay compatibility tests
are not evidence that those unfinished contracts are complete. No standalone
example or Storybook site is added in P5.

#### Historical P5 measured artifact change

This records the original P5 head
`569b1d26e558b838d3a572bf7e2e5fbd6280eeaf`, its prior runtime, and schema-2
single-format declaration measurement. The exact original record is preserved
in `baseline-metrics-p5-v2.json`; current accounting and runtime integration are
described below. These original backend/skip counts are historical, not a
substitute for rebuilding and validating the current owning checkout.

The clean Linux/amd64 Node 22.20.0 run passes **354 package tests, 95 Trading
tests, 6 metric-policy tests and 26 browser scenarios**, including all five
original exact-zero-diff visual PNG images. Installed React 18.3.1 contracts
pass on actual Node 22.20.0 and 24.19.0: **9 public type programs with 528
negative assertions, 8 import/SSR modes, 8 README parser guards and 10 literal
README snippets** in ESM/CJS/bundler modes. Client/auth programs remain
React-free. The five predecessor README snippets are preserved.

Coverage is measured without excluding moved code:

| Scope | Statements | Lines | Functions | Branches |
| --- | ---: | ---: | ---: | ---: |
| Package, all files | 97.85% | 99.05% | 98.29% | 95.08% |
| Client, unchanged | 97.73% | 99.10% | 99.34% | 95.27% |
| React hooks | 98.79% | 100% | 100% | 94.08% |
| `DataTable` | 100% | 100% | 100% | 96.85% |
| `QueryTable` and state adapter | 100% | 100% | 100% | 100% |
| `useTableSort` | 100% | 100% | 100% | 100% |
| Trading, all files | 89.28% | 90.14% | 87.93% | 81.84% |
| App-owned table wrapper | 92.40% | 94.11% | 100% | 88.09% |
| App-owned code viewer | 97.87% | 97.72% | 91.66% | 96.66% |
| App-owned inspector | 96.15% | 97.72% | 100% | 82.92% |

The enforced critical client/react floors remain 90% statements/lines/functions
and 85% branches. Existing client normalization, reducer, subscription,
configuration and provider evidence is retained; this layer does not rewrite
their protocol or lifecycles.

| Measured bytes | P4 | P5 |
| --- | ---: | ---: |
| Package tarball, including docs/maps | 187699 | 169457 |
| Package ESM, all entries/chunks | 98585 | 78523 |
| Package CJS, all entries/chunks | 106000 | 85845 |
| Package declarations, `.d.ts` family | 34875 | 37834 |
| Package CSS | 10043 | 10043 |
| Trading JavaScript | 250892 | 251684 |
| Trading JavaScript gzip | 73795 | 74133 |
| Trading generated CSS | 21034 | 21639 |
| Trading generated CSS gzip | 5163 | 5212 |

The first full browser run passed all behavior/visual checks and correctly
rejected the declaration-growth budget. The measured advance retains P1-P4
history and the same **2% future-growth policy**. The new provider-free/state
slot/error/sort/animation declarations add 2959 bytes; moving tutorial and
fullscreen code out of the package reduces every complete runtime-format
total, not just its entry stubs. The app still owns that functionality, plus
composition plumbing and lazy retry (792 more JavaScript bytes). Its unchanged
Tailwind content scan now also sees the moved modules, including `.table` and
`.transition` utility discovery; generated CSS grows by 605 bytes. Neither
package CSS nor any original image was edited, and all five comparisons remain
exact. No size threshold, coverage floor or assertion was disabled to pass.

Unchanged-backend checks pass: exact engine verification, registry plugin
origin, 42 tooling tests, real UI/server builds, 779 Rust tests (32 ignored),
`make fmt-check` and strict locked all-target Clippy. The locked-server audit
reports zero vulnerabilities and 15 existing warnings, not clearance of the
separate unused legacy core workspace.

The supported native plugin invocation is `make download-test-plugins`
followed by `./tests/plugin_smoke_test.sh --skip-build`: **8 pass / 28 configured
skips**, with the pinned registry artifacts verified. This is not all-plugin
coverage. The additional legacy `make test-smoke` convenience target still
exits 2 because its `build-dynamic` target is absent; that pre-existing blocker
was not reported as a passing gate or repaired by original P5. The approved
incoming parent later repaired that convenience path; the paragraph above is
historical evidence, not a current-runtime blocker.

#### Historical P5 ABI 0.13 parent integration and accounting

This section records the earlier `1284e9f` / library 0.9.1 / ABI 0.13
integration. Its runtime, counts and captured measurements are historical;
the current `211d0f2a` / ABI 0.14 propagation below needs its own proof.

P5 normally merges exact updated P4
`2ccf8624533193861e6518cb0c39785f561fa270` into original P5
`569b1d26e558b838d3a572bf7e2e5fbd6280eeaf`. Both histories and the existing PR
base ref are retained. No P6/P7 product code, query/financial changes, source
aliases, image refresh or scope-expanding dependency updates are introduced.
The same provider-free table, small live composition, scoped retries,
controlled/uncontrolled sort and Trading-owned lazy inspector/fullscreen
composition remain the contract.

The original P1/P2/P3/P4 schema-2 files remain byte-identical to the incoming
parent, and `baseline-metrics-p5-v2.json` adds the byte-identical original P5
record. Policy tests protect those bytes and retain every earlier budget case.
The original P5 tarball SHA-256 is
`3cb4774d8c34e5aee2464b66026210a37ef83abd2f65f84ff82c09dfb59f8aca`.
Remeasuring that same artifact with the incoming `measurePackageModules`
convention counts **37,834 ESM + 37,851 CommonJS = 75,685 declaration bytes**.
That is an accounting correction, not new runtime bytes or a budget relaxation.
The corrected P1-P4 totals remain **37,492 / 42,280 / 56,256 / 69,767**.
P5's actual declaration feature cost relative to P4 is **5,918 bytes** when
both formats are counted; the original 2,959-byte single-format difference
remains in its historical record.

The current metric scope includes every package JS/CJS/declaration entry and
shared/nested chunk, every shipped stylesheet, and recursive Trading JS/CSS
assets. Source maps remain part of tarball size, not executable-byte totals.
P5 retains its feature/runtime budget and all coverage floors, with the same
**2% future-growth rule**; it does not replace the incoming accounting with
the old single-declaration convention.

Current-runtime gates must build this checkout's default server and real UI
before Rust or live tests. The approved runtime is server 0.2.3, library 0.9.1,
host/plugin/FFI crates 0.11.0, index 0.6.1 and GQL 0.3.6, with the same reviewed
core 0.5.8 / AST 0.3.5 / Cypher 0.3.6 source correction. All six signed locks
come from official merged-main
`3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`: plugin SDK crate 0.11.1 and host
0.11.0 have actual native ABI 0.13. Historical ABI 0.11 captures stay separate;
reproducing them requires their old whole harness/policy, not a pin override
that bypasses the current guards.

The current DTO and seventeen raw SSE records inherited from P4 remain under
`dev-tools/react/test/fixtures/server-v1-0.2.3`, separate from old recordings.
P5's own-source and independent packed-frontend live runs use only its rebuilt
default binary and actual backend source via `P1_SOURCE_ROOT`. Required
2000/cost1800/count2, 2050 live/reload, 2150 offline/reconnect, CRUD/live/delete
and no-navigation assertions remain unchanged. There is no new authoritative
cursor, timestamp ordering, replay rollback or atomicity claim.

The integrated clean Linux/amd64 gate passes **357 package tests, 102 Trading
tests, 15 metric-policy tests and 26 browser scenarios**, including all five
unchanged exact-zero-diff visual PNG images. The existing public-contract and
literal README checks also pass in independent Node 22.20.0 and 24.19.0
consumers with React 18.3.1: 9 type programs / 528 negative assertions,
10 README snippets in all three TypeScript modes, 8 import/SSR modes and
8 parser guards. Client/auth remain React-free; hook imports remain UI-free.

Measured integrated bytes are **171,115 tarball / 78,523 ESM / 85,845 CJS /
75,685 dual declarations / 10,043 package CSS / 251,684 Trading JS
(74,133 gzip) / 21,639 Trading CSS (5,212 gzip)**. Runtime, declaration and CSS
bytes match the remeasured original P5 artifact. Only the tarball grows:
**1,658 bytes (0.98%)** for current-runtime and integration documentation.
The original P5 byte budget is not raised; the same 2% policy passes.
Coverage percentages and critical client/react floors remain unchanged.

The owning checkout's updated default binary and real UI were built before
Rust validation. All **38** Cargo summaries total **809 passed / 32 ignored /
0 failed**; 53 tooling tests, strict locked all-target Clippy, fmt and the
locked-host audit also pass. The audit retains 15 existing warnings. These
results do not clear unused legacy-core, embedded-plugin, npm or publisher-
visibility advisories, and they make no P6 human accessibility claim.
Final committed-head own-source/packed live and CI evidence is recorded on
the owning #208 and #164 Part A without rewriting later-layer evidence.

#### P5 newer-main development-source propagation

The current parent is `6bb523b65ed8ca6a090dcc92d4ff79eb7c12731d`, with exact
source and registry requirements in [the runtime matrix](../../docs/main-runtime-integration.md).
Only engine 0.5.9 / AST 0.3.5 / Cypher 0.3.6 use clean temporary source
`211d0f2a79aa2ad0f7cb841937f52013fe95ded6`; library 0.9.2, SDK/host/FFI 0.11.2,
index 0.6.3, functions 0.5.9, middleware 0.5.10 and GQL 0.3.6 remain registry
dependencies. All six `70ca432c` main-signed locks use native ABI 0.14.
Equal-version sibling SDKs do not authorize local plugin builds.

Build the owning checkout's real UI/default binary before Rust tests or live
runs, and record the binary actually used after any relink. Native work runs
before browser timing gates. Independent packed consumers use only
`P1_SOURCE_ROOT` for this own backend, never a frontend alias or lower binary.
The original singleton/CRUD/no-write-reload/2050-live/2150-reconnect assertions,
35 browser cases, five exact images, names/maps/DCE and quantitative gates
remain unchanged. Current evidence belongs to #208/#164 Part A; parent or
historical runs are not relabeled as this branch's validation.

The narrow historical 110-byte P5 Trading gzip approval is retained exactly,
not borrowed for package size, other metrics or later baselines. The package
tarball ceiling remains 172846.14 bytes. `211d0f2a` is not a published fix,
library-codec adoption, legacy-record repair or license to broaden recovery.
Earlier core/backport records, audit warnings and human AT/contrast limits
remain explicit.

#### P5 table quality corrections and approved gzip allowance

The quality pass preserves the exact parent endpoint-identity and committed-
query-key fixes from `2b9890b1adb056bdb419cf94fdd6a701cef53479`. It corrects
mixed-value sort cycles, ambient-locale hydration differences and explicit
left body alignment. Numbers (including infinities and explicitly ordered NaN)
precede nonnumeric text representations; text uses fixed en-US variant
collation with `numeric: false`, followed by nullish values. Descending reverses
that ordering while ties stay stable. Omitted body alignment still inherits.
This is not a universal cross-ICU Unicode-ordering or P6 accessibility claim.

`tableOrdering.test.tsx` covers permutations, transitivity, non-finite numbers,
signed zero, stable ties and actual Trading name ordering. The test-only
`table-presentation.spec.ts` bundles installed public entrypoints in memory,
renders on a real en-US Node server, and hydrates in sv-SE Chromium, Firefox
and WebKit. It rejects recoverable errors, console warnings/errors and root
replacement; it also checks inherited host alignment and row identity.
All three browser regressions failed before the fix, then passed in all three
engines. The resulting 35 browser cases include the original 26 and all five
unchanged zero-diff visual PNG images.

Ordinary package experiments and their failures were retained rather than
discarded: the unoptimized consolidated artifact was 180932 bytes, the bounded
whitespace/syntax experiment was 175542, and the chosen whitespace-only
artifact was 168385 before this approval note. All retained the complete
56-file/18-map inventory, original map source content, notices and public/debug
names. A single alignment-code consolidation failed both package and app gzip
limits and was reverted. No archive ordering, hidden exclusions, identifier
mangling, removed documentation or historical baseline reset was used.

The chosen output passed functional/public/type/SSR/browser/image and own-
source/independent-packed live checks, but **failed the original Trading gzip
cap**: 75725 bytes exceeded `74133 * 1.02 = 75615.66` by 109.34 bytes.
On 2026-09-21 the actual user approved exactly **one fixed 110-byte allowance**
for this recorded P5 metric. The cap is now **`74133 * 1.02 + 110 = 75725.66`**:
**75725 passes; 75726 fails**. The original 74133-byte baseline and 2% rule are
not reset, and the allowance is never multiplied or added to later observations.

`approvedP5TradingJsGzipAllowance` records this approval separately. Policy
guards bind it to P5 Part A, original P5 head and the fingerprint of all
original P5 size counters. A different size baseline, layer, metric or amount
cannot inherit it. A later layer using its own distinct baseline must remove
this P5-only approval record rather than applying or compounding it.
Boundary tests retain every other 2% limit, coverage floor and byte-identical
historical record. This approval is not a blanket budget, behavior, image,
color/contrast or human assistive-technology waiver.

### Part B Trading result consumers

Trading now supplies required raw keys and validating projections for all 11
queries. `integration/tradingOptions.test.ts` retains the financial expression,
sign/cap/order and deployment assertions, and adds raw identity, projection
shape, portfolio conversion, pure sorting and explicit legacy routing checks.
The previous warning-on-unroutable assertion is strengthened to require a safe
`UNROUTABLE_RESULT` and no row logging. No query text, source/join ordering,
creation order or provisioning code changes are part of this migration.

`integration/resultConsumers.test.tsx` uses the compiled package and real
Trading provider/components, not mocked hooks. It exercises a portfolio
id-only delete with a transform that rejects incomplete projected rows, a
symbol-only stock delete, and separate render identities for same-symbol
positions. Market-mover initial sorting, dialogs/actions, fullscreen/tutorial
behavior and all existing business expectations remain in their original
integration/browser gates; no golden images or metrics baselines are updated.

The synthetic transport now emits the actual untemplated SSE envelope names
`queryId/results/timestamp` and `ADD/UPDATE/DELETE`, retaining before/after and
all expected business values. It remains synthetic evidence, not a recording
or substitute for the mandatory actual-server singleton aggregate, reload,
offline/reconnect and CRUD checks. The original recorded #119 defects and
measured tables below are historical evidence, not new Part B pass claims.

Known result consistency limits: REST and SSE expose no shared cursor.
Overlapping nonempty deltas reject a pending snapshot and trigger bounded read
reconciliation; successful no-known-overlap reads are best effort, not atomic,
gap-free or exactly-once. Raw keys precede transforms, deletes never project,
and query-local retry is distinct from shared connection retry. Package tests
own the exhaustive reconciliation/state-machine proof; Trading's focused
tests prove its application options and consumer behavior.

The package's portable `test/ResultRegression.test.jsx` also runs unchanged
against exact archived P3 `a0569c2` and current Part B. All three checks fail on
P3 with observed wrong results: a projection that omits its raw key yields
`null`; a key-changing official update leaves old and new rows; and a snapshot
at value 2 followed by an older buffered value 1 rolls back to 1 without a
refresh. The same checks pass on Part B: projected value 1 then a sparse delete,
only the new key 2, and bounded reconciliation to value 3. These checks use
real providers and clients, without a framework/provider/transport-mock bypass.
They are focused predecessor regression evidence, not a substitute for the
packed-consumer, browser or real Trading-server gates.

### P3 contract additions

Package `test/resources.test.ts` consumes the versioned, unmodified server
response bodies in `test/fixtures/server-v1/contract.json`, captured from the
actual frozen backend by the live harness. The fixture records server revision,
binary SHA256, Cargo.lock hash, engine revision, SDK and SSE plugin/ABI versions.
Synthetic full-view fixtures now include the real required read fields and
scoped links; this changes no creation body, query text, source/join order,
snapshot row or business expectation. The live harness records two extra
read-only query full views alongside its existing authoritative observations.

`test/transport.test.ts` covers static/rotating auth, native fetch receiver
binding, credentials/headers on every endpoint, custom stream options,
401/403, explicit and late cancellation, request/body/auth deadlines,
redirect refusal and identical resource IDs across instances. Native SSE
header/status limitations and authenticated Node REST-only reads are explicit.
`test/DrasiLifecycle.test.tsx` uses real providers/clients, not hook mocks:
equivalent inline values preserve the single stream; material references,
credentials, policy and callable changes dispose the old scope. StrictMode,
unmount, stale snapshot/events/retry callbacks, query-local automatic retry and
controlled app-owned binding are covered. Legacy wire alternatives are
characterized at their typed object boundary, not relabeled as Part B's
official canonical adapters.

The existing clean tarball consumer invokes the public contract suite under
`dev-tools/react/test/public-contract`. It tests ESM/CommonJS and conditional
declarations for all four exports, positive/negative generic API examples,
React 18 SSR and import-time DOM/network traps. An isolated peer-omitted
client-only install has neither React runtime nor React types, and its
declaration/runtime graphs are checked. Hook imports cannot reach composed
components/tutorial code/CSS. Marked runnable README snippets are compiled
against the installed tarball. The package and Trading still build without
source aliases or lifecycle rebuilding in the consumer.
Eight parser checks require all five marked README examples and reject duplicate
names, path traversal, wrong fence languages, unterminated fences and compiler
diagnostic suppression. The actual installed README is hashed and compiled in
ESM, CommonJS and bundler modes; the client/auth snippets also compile with
React and React types absent. Parallel hand-written examples alone are not
treated as documentation proof.

From the repository root:

```sh
npm --prefix dev-tools/react ci
npm --prefix dev-tools/react run typecheck
npm --prefix dev-tools/react run test:coverage
npm --prefix dev-tools/react run build
npm --prefix examples/trading/app ci --ignore-scripts
npm --prefix examples/trading/app run typecheck
npm --prefix examples/trading/app run test:coverage
npm --prefix examples/trading/app run --ignore-scripts build
cd examples/trading/app
npx --no-install playwright install chromium firefox webkit
npm run test:browser -- --project=chromium --project=firefox --project=webkit
```

`npm test` now runs assertions and exits nonzero on failure. It never installs or
builds the package implicitly: use the commands above first. React 18.3.1 is the
Trading baseline. Node 22 is the CI baseline. No React 19 support is established
by these tests.

For linked-package Vitest development, an invalid-hook-call error confined to
dialogs can indicate that an externalized primitive resolved the package's
development React copy instead of the app's existing deduplicated React.
Trading combines `resolve.mainFields: ['module', 'main']` with targeted
`test.server.deps.inline` entries for Radix, `react-remove-scroll` and its
singleton/callback/sidecar helpers, routing their imports through the existing
`react`/`react-dom` deduplication. Retain both parts of this test-resolution
configuration. It does not change package types or primitive architecture,
alias the package to source, add a second React runtime, or bypass the
installed-tarball gate.

The first client mount of Modal loads its maintained primitive asynchronously;
a closed mount warms that layer without taking focus or scroll ownership.
Tests must await the ready dialog role (for example, `findByRole`) before
keyboard interaction rather than assume synchronous first-mount rendering.
Keep the existing readiness deadlines; no timeout increase is required by this
boundary. A delayed-module regression should delay only the load, then use the
real primitive to prove close/unmount cannot acquire ownership later.

## Reproducible visual and packed-consumer gate

Visual comparisons run in the pinned Linux/amd64 environment, **not against
screenshots produced by a different host OS**:

```sh
cd examples/trading/app
npm run test:browser:linux
# Deliberate baseline creation/update, followed by image-diff review:
npm run test:browser:linux -- --project=visual --update-snapshots
```

This requires Docker and uses
`mcr.microsoft.com/playwright:v1.56.1-noble@sha256:f1e7e01021efd65dd1a2c56064be399f3e4de00fd021ac561325f2bfbb2b837a`
(Node 22.20.0; Playwright 1.56.1 / Chromium 141.0.7390.37).
It checks out no other repository and writes no production files. It builds,
type-checks, tests and packs the package, copies Trading to an empty consumer,
and runs its checks using only the tarball. The tarball's sources are absent.
All unrelated locked consumer dependency versions and integrity hashes are
checked before the clean `npm ci --ignore-scripts`.

The GitHub Actions browser step explicitly sets `HOME=/root`, matching the
pinned container's effective user. Actions otherwise supplies a
`/github/home` owned by `pwuser`, which Firefox refuses to use as root.
The override is scoped to the browser step; it does not change shared
directory ownership, disable browser safeguards or skip Firefox coverage.

The container job records its actual `$RUNNER_TEMP` for artifact upload. The
host-side `runner.temp` context is not translated inside action inputs; using it
there previously dropped Trading traces/logs even though workspace coverage was
uploaded. Failed browser cases now retain their real diagnostics.

The disposable consumer first substitutes the tarball using
`npm install --package-lock-only --ignore-scripts`. This is intentional:
some npm versions run a directory dependency's `prepare` while resolving a
`file:` dependency, even when the outer install used `--ignore-scripts`.
The repository's actual Trading manifest remains a local built-artifact
dependency. Only the disposable consumer points at a tarball.

For a manually created tarball:

```sh
# From the repository root; choose a new, empty destination.
(cd dev-tools/react && npm pack --pack-destination /tmp)
node examples/trading/app/test/tools/packed-consumer.mjs \
  /tmp/drasi-react-0.1.0.tgz /tmp/trading-packed-consumer
cd /tmp/trading-packed-consumer/examples/trading/app
npm run typecheck
npm run test:coverage
npm run --ignore-scripts build
```

Synthetic data and projections live in `app/test/fixtures/synthetic/`. Six
known stocks include positive, negative and zero changes, and volume exactly at
the 10,000,000 boundary. Initial portfolio totals are $2,000 value / $1,800 cost.
The browser clock is fixed at `2026-01-15T12:00:00Z`; tests explicitly advance
timers/frames, and do not run the random price generator. Screenshot comparison
allows **zero differing pixels**. Update expectations only after explaining the
change, checking the behavior inventory and reviewing the image diff.

The fixture HTTP server binds loopback port **15273**, overridable with
`P1_WEB_PORT`. An occupied port is a failure, not permission to reuse/kill the
service already there. Playwright owns and stops its server. Docker runs own
short-lived containers with `--rm`; no shared containers, networks or volumes
are removed. Each Linux run retains a named `.test-runtime/linux-browser-*`
directory containing coverage, tarball, built consumer assets, HTML report,
failure screenshots/video/traces and any generated expectations. Remove only a
specific completed run's directory when no longer needed.

## Coverage and size evidence

The checked-in `app/test/fixtures/baseline-metrics.json` records measured
**Linux/amd64, Node 22.20.0, Vitest 3.2.7 V8 with AST-aware remapping**
coverage, not the final #161/#163 targets. All
package and app `src` files are included, including currently uncovered
entrypoints and unused components.

| Vitest/V8 scope | Statements | Lines | Branches | Functions |
| --- | --- | --- | --- | --- |
| Package (18 tests) | 62.52% | 64.93% | 47.47% | 64.84% |
| Trading (22 tests) | 83.79% | 84.83% | 74.52% | 82.01% |

The app runner also includes nine native-version setup guards. These are
additional checks, not new Trading behavior coverage or changed product floors.

Schema version 2 remeasures the same source and unchanged test scenarios using
Vitest's existing `experimentalAstAwareRemapping` option. Legacy V8 remapping
produced either 241/372 or 242/373 covered package branches on identical runs:
one always-covered synthetic range in `DrasiContext.tsx` appeared depending on
coverage-file merge order. That made the original 64.87% branch floor flaky,
despite the same 131 uncovered ranges. AST-aware remapping instead counts
syntactic statements/branches/functions consistently: six package runs and
three Trading runs produced identical counters. It also exposes more real
branches (339/714 package, 389/522 Trading), so these percentages are **not
comparable** to the original legacy-V8 measurements. No product code or tests
were removed, no source was excluded, and no runtime/tool version was changed.

The corrected P1 artifact baseline is 110,691 bytes packed, 66,865 bytes package ESM, 68,860
bytes CJS, 37,492 bytes declarations and 10,043 bytes package CSS. Clean Trading
JS is 223,574 bytes (65,775 gzip), CSS 21,034 bytes (5,163 gzip).
Schema version 3 counts every emitted package JS/CJS entry and nested chunk,
both declaration formats, every packaged stylesheet, and recursive Trading
JS/CSS assets. The unchanged tarball contains 18,746-byte `index.d.ts` and
18,746-byte `index.d.cts` files; version 2 counted only the former. Its exact
measurements are retained in `baseline-metrics-v2.json`. P2's original version-2
record is separately retained in `baseline-metrics-p2-v2.json`: its two
21,140-byte declaration files total 42,280 bytes, not 21,140. Neither correction
changes the shipped tarballs: P1 SHA-256
`f85eae9c85c50b1af98474d1b0524dc12b5ed9e780cf1c5500b4ef7fbad733f0` and P2
SHA-256 `ee28948165c33c762d3d7fb3ff6e086bda553fe60fd5c12b0499bc7918745140`.
This is a measurement correction, not artifact growth or a relaxed budget. Source maps and other
non-runtime files remain covered by the complete tarball byte metric.
`check-baseline.mjs` rejects any coverage drop or artifact growth above 2%;
explain and review baseline updates instead of silently accepting them.
The Linux runner and CI both run this check and retain
`test-results/measured-baseline.json`. V8 counters can differ across Node
versions, so compare the pinned environment rather than mixing host coverage.

P3 introduces multiple entrypoints and shared chunks. Package ESM/CJS metrics
therefore sum **every shipped file of that format**, including entrypoint
wrappers, rather than measuring only the now-small root barrel. Declaration
bytes now sum all `.d.ts`, `.d.mts` and `.d.cts` files: the original P3
measurement omitted its separately shipped CommonJS declarations. P3's exact
schema-2 record is retained in `baseline-metrics-p3-v2.json`. Its 28,121 ESM
declaration bytes plus 28,135 CommonJS declaration bytes total 56,256, with no
product-byte growth. Both P3 module-accounting cases and all incoming P1/P2
cases remain, including nested assets, CSS, duplicate paths and the original
four threshold checks. Source maps remain included in tarball bytes. The same
2% policy and coverage floors are unchanged.

Browser coverage is scenario-based; these percentages are Vitest/V8 only and
must not be presented as browser or real-server coverage.

### Measured P2 contract cost

The same pinned Linux gate measured P2 after all 26 browser scenarios and the
five unchanged, zero-differing-pixel PNG files passed. Only the **artifact size
baseline** was advanced. `artifactChange.p1Sizes` compares the corrected P1
accounting against P2; the two historical version-2 records remain unchanged.
The 2% growth policy, every P1 coverage floor, all included source files and
visual expectations are unchanged. Frontend dependencies are unchanged; the
approved backend runtime update is recorded separately below.

| Artifact | P1 bytes | P2 bytes | Reason for growth |
| --- | ---: | ---: | --- |
| Packed tarball | 110,691 | 125,325 | Runtime, declarations, source maps and the reference/error/ownership documentation |
| Package ESM / CJS | 66,865 / 68,860 | 72,992 / 75,038 | Resource DTO guards, instance paths, typed errors, bounded transport/snapshot handling and controlled binding |
| Declarations (both formats) | 37,492 | 42,280 | Explicit references, read DTOs, error codes/identity, timeouts and lifecycle binding; the former single-format counts were 18,746 / 21,140 |
| Trading JS / gzip | 223,574 / 65,775 | 233,695 / 69,038 | App-owned idempotent setup, conflict checking, cancellation, Web Locks and retry UI |
| Package CSS / Trading CSS | 10,043 / 21,034 | 10,043 / 21,034 | Unchanged |

The app's net runtime addition is 10,121 bytes (3,263 gzip), not a new dependency
or duplicated React/SSE implementation. Setup contains the single app mutation
boundary; the package has no POST/PUT/PATCH/DELETE path. Do not update screenshots
or lower coverage to accommodate future drift.

P2's measured whole-package coverage is 77.09% statements / 78.80% lines /
64.65% branches / 78.06% functions; Trading is 87.31% / 87.96% / 78.06% /
85.59%, respectively. The new Trading provisioner is 98.81% statements /
99.28% lines / 92.98% branches / 100% functions. These improve every P1 floor;
they are not a claim that #163's final transport/result-contract coverage targets
are finished.

### Historical P3 transport/type contract cost

This is the original schema-2 measurement before the main-runtime integration.
The table retains its original single-format declaration numbers; the current
schema-3 declaration budget counts both formats as described above.

The first complete P3 Linux/amd64 Node 22.20.0 run passed the package/type/SSR
and source-free consumer gates, all 26 browser scenarios and all five original
PNG comparisons. The old 2% **size** gate then correctly rejected the feature
growth. The following measured sizes advance that artifact baseline with this
explicit explanation; P1 and P2 bytes remain in `artifactChange.p1Sizes` and
`p2Sizes`. No coverage floor, 2% policy, query/financial assertion, CSS or
visual baseline was reduced/changed to achieve a pass.

| Artifact | P2 bytes | P3 measured baseline bytes | Reason |
| --- | ---: | ---: | --- |
| Packed tarball | 125,325 | 155,916 | Runtime/type/source-map additions plus full transport/auth/SSR/migration docs and shipped CHANGELOG |
| All package ESM / CJS | 72,992 / 75,038 | 84,754 / 91,229 | Complete DTO/JSON/link guards, shared auth/cancellation, scoped lifecycle and all entrypoint/shared-chunk wrappers |
| All `.d.ts` declarations | 21,140 | 28,121 | Named read/transport/hook/column contracts, physical type boundaries and subpath exports |
| Trading JS / gzip | 233,695 / 69,038 | 240,826 / 70,541 | New client contract and typed app-boundary handling; not a new dependency or React copy |
| Package CSS / Trading CSS | 10,043 / 21,034 | 10,043 / 21,034 | Unchanged |

P3's exact run measurements remain in its retained artifacts;
subsequent P3 documentation/guard refinements remained subject to the same
2% cap above that recorded measurement, not an unbounded budget increase.
The root barrel alone is only hundreds of bytes after splitting, so measuring
only it would be misleading; all shipped modules are counted.

The pinned run measured whole-package S/L/B/F
**87.38 / 89.03 / 83.22 / 89.42%**, Trading
**87.40 / 88.24 / 78.10 / 86.45%**, and the unchanged provisioner
**98.81 / 99.28 / 92.98 / 100%**. Client modules measured
**97.08 / 98.87 / 94.21 / 99.15%**; `DrasiContext.tsx` measured
**95.62 / 100 / 85.32 / 100%**. These exceed the current critical-code
90/90/85/90 targets without excluding product code. They do not complete
Part B's missing identity/normalization/consistency/state scenarios.

For before/after evidence, the equivalent-inline-configuration provider test
was replayed against an isolated archive of exact P2 `3e9833c`: it fails because
rerendering replaces the client and closes the existing stream. The same real
provider test passes on P3. No predecessor/other worktree was modified.
Positive/negative packed type cases, malformed captured DTO mutations and
auth/cross-instance cases complement the inherited business assertions.

### P2 current development-source update

The normal merge of P1 `74bebe2e585946ac52fce9b37473790b7c377bea` retains
P2's reference-only, GET-only package and app-owned bounded/shared setup.
Package and Trading runtime sources, query definitions, negative ownership
cases, financial assertions, original images and complete artifact budgets
are unchanged. The inherited source/runtime guards now require core 0.5.9,
index 0.6.3 and registry SDK 0.11.2, rejecting the earlier versions.

This selection needs its own rebuilt server/UI, signed ABI 0.14 plugins and
actual browser/Trading evidence. The `211d0f2a` pin is temporary development
source, not a released fix; the older `1284e9f` reports below are historical.
Current commands, exact-head provenance and CI results are recorded on #205.
Neither unchanged frontend bytes nor earlier lower-layer passes substitute
for the current mixed graph's live proof.

Own local verification passes 83 package tests, 66 Trading tests (57 existing
plus nine runtime-version guards), nine artifact-policy tests and 54 tooling
tests. The complete source-free Linux gate passes all 26 browser scenarios and
five original exact PNG files with unchanged artifact bytes and coverage.
The own rebuilt UI/default server passes 809 locked Rust tests / 32 existing
ignores; full plugin-dependent testing passes 840 / one ignored doctest, with
the separate legacy smoke still eight passes / 28 unconfigured skips.
Formatting, strict Clippy and the selected host audit pass with 15 existing
warnings, without suppressions or dependency changes beyond the inherited graph.

The actual new-runtime Trading gate passes fresh setup, no-write reload and
all CRUD/live/delete/reconnect assertions, including the unfiltered singleton
2000/cost 1800/count 2 -> 2050 live/reload -> 2150 after reconnect. Its 17 raw
SSE 0.3.7 events retain the observed `queryId`/`results`/`timestamp` envelope
and `ADD`/`DELETE`/`UPDATE`/`aggregation` shapes; aggregation has `before`/`after`
without `data`. This is observed compatibility, not a stronger protocol claim.

An initial local gate run concurrent with native builds failed the WebKit
two-tab readiness assertion and differed by 454 pixels in the animated desktop
ticker. Both diagnostics are retained. The full unchanged gate then passed
without concurrent Rust work; no clock, readiness, timeout, assertion, image,
query or artifact-budget adjustment was made. Fresh final-head CI remains
separate evidence on #205.

### Historical P2 ABI 0.13 main-runtime update

The earlier normal merge of P1 `e361d1c3369e83213f0bb164ab2be66569ea7e18`
changed no P2 package or Trading runtime source. Its own rebuilt server/UI and
signed ABI 0.13 plugins passed the real Trading scenario with the singleton totals
above; that raw capture has 17 SSE events with the observed envelope/result
shapes documented below. Earlier raw captures remain historical, not a substitute
for this runtime proof.

That layer passed 83 package tests, 64 Trading tests (57 existing plus
seven runtime-version guards), nine artifact-policy tests and 53 tooling tests.
The source-free Linux gate passed all 26 browser scenarios and the original five
exact PNG files. Locked Rust tests reported 809 passes / 32 existing ignores;
`make test-all` reported 840 passes / one existing ignored doctest, with the
separate limited plugin smoke reporting eight passes / 28 unconfigured skips.
Strict Clippy/fmt and the selected host audit passed, retaining its 15 existing
warnings. Its CI, binary/lock hashes and independent packed live proof remain
in #205's historical evidence, not relabeled as proof for the newer runtime.

### P3 current development-source integration

The current P3 parent is
`d784446d7e2ca369c2ae0a3e3c560455d54369a6`. Its source/setup/runtime selection
is the same temporary `211d0f2a` / registry library 0.9.2 / SDK 0.11.2 /
native ABI 0.14 graph described below. P3 retains all public entrypoints,
React-free client runtime/type graphs, guarded reads, auth/cancellation and
provider ownership/identity behavior. In particular, explicit endpoint
path/query slashes remain significant while equivalent server bases do not
churn the connection; all 17 endpoint regressions are unchanged.

Older core 0.5.8 / `1284e9f` / ABI 0.13 results and fixtures stay historical,
not current-runtime evidence. This graph must build its own embedded UI/default
server before binary-launching tests and run the same raw financial/CRUD/live
gates. Run timing-sensitive packed browser/visual checks after heavy native
work, not concurrently. The same 2% artifact cap, source coverage floors,
clock/readiness assertions, all 11 query definitions and five original images
remain authoritative. Current proof and final-head CI belong to #206.

### Historical P3 ABI 0.13 main-runtime integration

The earlier P3 integration normally merged exact P2
`f8e06c3415d5190f71b2f9ad3e294b7fe2d8c467`, preserving its original
`a0569c2ae7b17f51996f431cf2264c636a19f3d1` ancestry, public entrypoints,
React-free client graph, guarded reads, auth/cancellation and configuration
lifecycle. No P4-P7 product code is imported. The incoming v1 query DTO is
byte-identical: `enableArchive` and `memoryBudgetMiB` configure the server or
instance, not extra top-level fields of the query read DTO. Per-query
`storageBackend` remains validated, opaque JSON rather than a new client
creation API.

The original P1/P2 schema-2 records and P3's original schema-2 record remain
unchanged. Current accounting retains every P3 entry/shared chunk and the
incoming recursive asset/CSS checks, counts both declaration formats, and
keeps the same coverage floors and 2% growth rule. New source/lock/binary/live
evidence belongs to #206; old runtime captures are not relabeled as 0.2.3
or ABI 0.13 results.

That own rebuilt merge source `5988b1d82cbb4d24dca2d4cb2763c0572d564bf4`
passed the unchanged actual Trading flow on server 0.2.3, with singleton
2000/cost 1800/count 2 -> 2050 live/reload -> 2150 offline/reconnect,
CRUD/live/deletes and no recovery navigation. Its binary SHA-256 is
`6327ce9a6e938451f50174d634efdf795f011044b5b8eeeafbd8204b74fb3b28`;
lock SHA-256 is
`63565b6a959f0c51a0cec916381866511be8da75f81d21dccc31ba7b2748b414`.
Actual full-view query/reaction/snapshot records are retained separately in
`dev-tools/react/test/fixtures/server-v1-0.2.3/contract.json` and run through
the same read-only guards as the original 0.2.1 records.

Those checks reported 221 package tests, 64 Trading tests, 12 artifact-policy
cases, 53 tooling tests, and 809 locked Rust passes / 32 existing ignores.
Strict Clippy/fmt and the selected audit pass with zero vulnerabilities and
15 retained warnings. The first local full Linux run had a 375-pixel mobile
ticker-only difference; an unchanged targeted visual rerun and a subsequent
full 26-scenario rerun passed all five original images. No product, screenshot,
timeout, readiness or clock assertion was changed to obtain those passes.
Those packed/CI results remain in #206's historical evidence and do not
validate the newer `211d0f2a` / ABI 0.14 selection.

### Historical P4 result/state contract cost

P4 starts at exact P3 `a0569c2ae7b17f51996f431cf2264c636a19f3d1`.
This section preserves the original `20561c1` evidence on server 0.2.1 /
SSE 0.3.4 / ABI 0.11. The table's declaration column uses its original
single-format accounting; it is not the current schema-3 declaration total.
The first frozen Linux/amd64 Node 22.20.0 run passed **329 package tests,
82 Trading tests, six unchanged metric-policy tests, all 26 browser scenarios
and all five original zero-differing-pixel PNG comparisons**. The old size
gate then correctly rejected the feature cost. Only the measured artifact
baseline advances here: P1/P2/P3 committed measurements and P3's final exact-head
CI observations remain in `artifactChange`; the **2% future-growth policy**
and original whole-package/application coverage floors are unchanged.
Vitest now additionally enforces the client and React subtrees at
90% statements/lines/functions and 85% branches. No product exclusions,
denominator changes, dependency upgrades or visual relaxations are used.

| Artifact | P3 final CI bytes | P4 measured bytes | Feature cost |
| --- | ---: | ---: | --- |
| Packed tarball | 155,715 | 187,699 | +31,984: normalized runtime, declarations/maps, wire/identity/state/migration documentation |
| All package ESM / CJS | 84,877 / 91,352 | 98,585 / 106,000 | +13,708 / +14,648: strict normalizer, named compatibility adapter, raw reducer and bounded subscription reconciliation |
| All `.d.ts` declarations | 28,121 | 34,875 | +6,754: snapshot/delta/change, adapters/context, raw-key/projected-row options, states and scoped retry |
| Trading JS / gzip | 240,901 / 70,550 | 250,892 / 73,795 | +9,991 / +3,245: result contract plus app-owned row guards and legacy selection; no additional React copy/socket |
| Package CSS / Trading CSS | 10,043 / 21,034 | 10,043 / 21,034 | Unchanged; Trading CSS gzip remains 5,163 |

Metrics sum every entrypoint and shared chunk, not just the root barrel.
Node 24.19.0's separate clean tarball consumer measured the same byte counts.
Normal Trading presentation is unchanged, including desktop/mobile dashboards,
mobile position dialog, fullscreen watchlist and portfolio query-code images.
Recovery-only messages preserve last-good rows and name the correct retry scope.
All 11 Cypher definitions, creation/source/join order, app provisioner, financial
transforms/filters/default sorting and backend/plugin pins remain unchanged.

| P4 Vitest/V8 scope | Statements | Lines | Branches | Functions |
| --- | ---: | ---: | ---: | ---: |
| Whole package | 89.54% | 90.37% | 86.69% | 90.90% |
| Client modules | 97.73% | 99.10% | 95.27% | 99.34% |
| Result normalizer/adapters | 99.34% | 100% | 99.48% | 100% |
| Raw identity reducer | 100% | 100% | 100% | 100% |
| Subscription reconciliation | 96.74% | 100% | 88.29% | 93.75% |
| Hook context/state | 98.83% | 100% | 93.10% | 100% |
| Trading app | 88.21% | 89.06% | 80.48% | 86.66% |
| Unchanged Trading provisioner | 98.81% | 99.28% | 92.98% | 100% |

These scopes do not claim completion of #164's UI/accessibility work or
#165's independent examples. Named cases, not coverage alone, establish
query-ID isolation, key-changing updates, sparse deletes, reactive filtering,
late/cancelled work, StrictMode, bounded overflow and terminal/transient retry.
The portable P3-fail/P4-pass cases above preserve their actual wrong-result
evidence. Tests also deliberately show the protocol limit: an undetectably
delayed older event can temporarily replace a newer no-known-overlap snapshot
until refresh. No clock/signature ordering or exactly-once guarantee is claimed.

Clean Node 22.20.0 and 24.19.0 consumers pass nine public-contract programs
with **363 consumed negative assertions**, all five literal installed README
examples in ESM/CommonJS/bundler modes (client/auth additionally without React
or its types), eight extraction guards and eight runtime/import/SSR runs.
The package remains private and unpublished.

Both checkout and source-free packed frontend runs pass the unchanged actual
server test: authoritative singleton **2000 / cost 1800 / count 2**, live **2050**,
reload **2050**, offline/reconnect **2150**, with CRUD/live/deletes, no recovery
navigation and no historical candidate rows. The real UI and locked server were
built; native locked Rust tests passed **779 / 32 ignored**, strict clippy/fmt
and 42 tooling tests passed, and the selected-server audit reported **0
vulnerabilities / 15 retained warnings**. The unused legacy core-workspace
advisories remain separate. Owned live services/ports were stopped; shared
services were not touched. These are development proofs, not automatic
merge/release authorization; the unchanged external YAML-agent model/HTTP-400
blocker and configured skips must still be reported against the final PR head.

### Historical P4 ABI 0.13 integration and accounting

The normal merge retains P4's raw-key-before-projection semantics, sparse
deletes, before/after identity changes, explicit adapters and query-scoped
recovery. It does not replay ambiguous snapshot cuts, infer ordering from
clocks/signatures, cap result sets, or promise an authoritative cursor.
The unchanged predecessor red/green tests remain part of the package gate.

`baseline-metrics-p4-v2.json` is the byte-identical original P4 record, alongside
the incoming original P1/P2/P3 schema-2 files. The retained original P4 tarball
has SHA-256
`de1d883e324f9b0597b3e1620df43fa9bb49efd998a24f50b9b4ad1beb075c9a`.
It contains 34,875 `.d.ts` bytes and 34,892 `.d.cts` bytes: **69,767 total**.
Current accounting includes both graphs, all entry/shared/nested runtime
chunks, all packaged CSS and recursive Trading assets. The P4 runtime/artifact
budget is retained rather than replaced with the smaller P3 feature budget.
Corrected P1/P2/P3 declaration totals remain 37,492 / 42,280 / 56,256.
This changes neither product bytes nor the 2% policy or coverage floors.

At that ABI 0.13 integration, rebuilt-source and packed/browser/live results
belonged to #207. Those measurements and old runtime recordings are not
relabeled as current evidence. Its ABI 0.13 guards were not weakened to run an
old ABI 0.11 cache or historical image. Reproduce old results only with the
complete matching historical harness/policy described below.

The own default `target/debug/drasi-server` was rebuilt before Rust integration
tests. Normal merge `3b39126ae36c9a0da96a4c0e29ad812b181636f6` has exact
parents original P4 `20561c13` and approved P3 `39f82c9`. That
server 0.2.3 / library 0.9.1 / host SDK 0.11.0 / index 0.6.1 binary has SHA-256
`eec11ff22242d354b05ef5fb65f1f5240c235a9906298c342d77276ab8feef2d`,
with lock SHA-256
`63565b6a959f0c51a0cec916381866511be8da75f81d21dccc31ba7b2748b414`.
The unchanged own-source live scenario passed with fresh signed SSE 0.3.6 /
plugin SDK 0.11.1 / ABI 0.13 plugins: singleton 2000/cost 1800/count 2,
live/reload 2050 and offline/reconnect 2150, CRUD/deletes and no recovery
navigation or historical candidate rows. This is not a borrowed lower binary.

All 17 raw CDP records from that run remain separate in
`dev-tools/react/test/fixtures/server-v1-0.2.3/sse-0.3.6.ndjson`, with adjacent
source/manifest/lock/binary/plugin provenance. Tests retain all old recording
assertions and exercise those records through the unchanged normalizer
and default SSE transport. In particular, recorded lowercase aggregation carries
before/after without `data`; numeric signatures still exceed JavaScript's safe
integer range and are never row identity. No authoritative snapshot cursor,
clock-based ordering or broader plugin compatibility is inferred.

The integrated local gates pass 332 package tests, 89 Trading tests, all
13 artifact-policy tests (all originals retained), and 53 tooling tests.
Own rebuilt locked Rust passes 809 tests with 32 existing ignores; fmt,
strict Clippy, typo validation and the selected audit pass (zero vulnerabilities,
15 retained warnings). Clean installed Node 22.20.0 and 24.19.0 consumers retain
nine public programs / 363 negative assertions, five literal README examples,
eight extraction guards and eight import/SSR checks, including React-free
client/auth and hook presentation isolation. The full packed Linux run passes
all 26 scenarios and all five unchanged exact images on its first run.
The independent source-free frontend also passes the current own-source live
scenario; it points `P1_SOURCE_ROOT` only at this backend checkout.

Integrated measurements are **189,130 tarball / 98,585 all ESM / 106,000 all
CJS / 69,767 all declarations / 10,043 package CSS** bytes. Packed Trading
remains **250,892 JS / 73,795 JS gzip / 21,034 CSS / 5,163 CSS gzip** bytes.
Only the tarball changes from original P4: **+1,431 bytes (0.76%)** for current
runtime/compatibility documentation. Executable, CSS and already-shipped
declaration bytes are unchanged; the declaration metric now counts both graphs.
The existing P4 byte budget is not raised, and the same 2% policy passes.
Current-head CI and final provenance remain on #207; external workflow/model,
configured-skip, embedded-plugin/legacy-workspace, npm/publication and human
assistive-technology caveats are not converted into passed product claims.

### P4 current development-source integration

P4's normal merge of exact P3 `887ff332df02b6d42a56a97a853a3522ec7e42db`
retains its original work and `2b9890b` concurrent-render correction. The
captured raw `getKey` is still published only at commit before layout
notifications. All four concurrent/StrictMode/committed-projection/no-churn
regressions, 17 endpoint tests and packed SSR console/browser-global traps
remain authoritative. No new shared cache, protocol, UI, query, recovery
promise or historical-data filtering is introduced.

The current source/runtime/pin policy below applies: clean temporary `211d0f2a`
engine/parser source, registry SDK even when equal-version sibling SDKs exist,
and the six official signed ABI 0.14 locks. Own embedded UI/default-server
build precedes serial Rust tests; record the actual used binary hash after
any test relink. Browser/visual work runs only after heavy native builds.
Both installed Node versions and the source-free Trading frontend need fresh
proof against this own runtime, not a lower owner's binary or old ABI cache.

P3/P4 historical fixtures, all artifact histories, coverage floors, complete
declaration/asset accounting and the existing 2% budget stay intact. Current
head/source/binary/live/CI results belong to #207. The temporary source is not
a released fix, library-codec adoption, legacy-data repair, broader recovery
guarantee, publication or completed human assistive-technology acceptance.

## Mandatory real-server gate

The primary gate builds the **checked-out server and its Cargo.lock**, not an
arbitrary executable with the same version number. Docker provides a disposable
PostgreSQL database; the original `mock-generator/trading_api.py` is run with
Flask on a dedicated loopback port. The actual built Trading app creates its
queries/reaction and consumes real REST/SSE data. No synthetic endpoints are
enabled in this browser run.

### Integrated source and shared setup

Prepare/verify the exact `.drasi-core-revision` **before** any locked Rust build:
`211d0f2a79aa2ad0f7cb841937f52013fe95ded6`, the user-approved existing source
from drasi-project/drasi-core#810. This is a **temporary development source
pin**, not a released fix or permission to merge/publish it. The default
manifest selects only sibling engine 0.5.9 / AST 0.3.5 / Cypher 0.3.6.
Library 0.9.2, SDK/host/FFI 0.11.2, index 0.6.3, functions 0.5.9, middleware
0.5.10 and GQL 0.3.6 remain registry-sourced with server 0.2.3. Equal-version
SDKs in the sibling workspace are not selected or permission to build plugins
there. The older `1284e9f` / core 0.5.8 / ABI 0.13 reports are historical.
See
[B1's full provenance and consumption boundary](../../docs/engine-prerequisite.md)
and [the approved main-runtime matrix](../../docs/main-runtime-integration.md).

Relative to released core 0.5.9, the selected source includes three aggregate
production paths and two additive merged-main outbox paths, not only a
three-file overlay. Registry library 0.9.2 calls `append`, not the new trim
methods. Engine-only selection does not consume drasi-project/drasi-core#909's
library codec change: compact MessagePack remains selected. These tests use
fresh owned state and make no old-record repair, record-dropping, output-only
clearing, persistence migration or general recovery claim.

`source_provenance.py` invokes the shared source verifier and resolved-SDK
policy, checks the caller's commit/lock and exact engine/parser origins, and
rejects a different caller plugin lock. Native installation reuses
`scripts/install_plugins.py`, rather than a second installer. Actual loaded
plugin status/hash/version/ABI metadata is validated by that same shared
policy and retained in `loaded-plugins.json`. The native CLI's reported server
and SDK versions must also match the verified lock/resolved SDK; a stale
`target/debug/drasi-server` fails before any test services start. The success
banner uses that verified binary version, not the historical image version.

For a source-free packed Trading consumer, set `P1_SOURCE_ROOT` to the original
server checkout. It supplies **backend build provenance and setup helpers
only**; no package source alias or Tailwind scan is introduced. The frontend
still consumes the tarball. CI prepares the pinned sibling before its build,
passes that checkout explicitly, and triggers on Cargo, Make, core pin, shared
helper/pins, server/UI, package and Trading changes for dependent PR bases.

Prerequisites: a POSIX host, Docker, the repository's Rust toolchain, Node 22,
Python 3.13, and access to GHCR/Sigstore for signed plugin
verification.

Linux checkout builds also require the system libjq and Oniguruma development
libraries. The Ubuntu CI job installs `libjq-dev` and `libonig-dev` and exports
`JQ_LIB_DIR=/usr/lib/x86_64-linux-gnu`, matching the existing getting-started
workflow. This explicit path is needed because the distro's libjq development
package does not supply `libjq.pc`; installing only the `jq` executable is not
sufficient for `jq-sys`.

From the repository root, after the fast package/app setup above:

```sh
bash scripts/prepare-build.sh
npm --prefix ui ci
npm --prefix ui run build
cargo build --locked
python3 -m venv examples/trading/app/.test-runtime/venv
examples/trading/app/.test-runtime/venv/bin/python -m pip install \
  -r examples/trading/app/test/live/requirements.txt

P1_SOURCE_ROOT="$PWD" \
P1_SERVER_BIN="$PWD/target/debug/drasi-server" \
P1_SERVER_CARGO_LOCK="$PWD/Cargo.lock" \
P1_SERVER_REVISION="$(git rev-parse HEAD)" \
P1_PYTHON="$PWD/examples/trading/app/.test-runtime/venv/bin/python" \
npm --prefix examples/trading/app run test:live
```

Rebuild the default server binary before Rust integration tests as well: some
tests invoke `target/debug/drasi-server` directly. A build in another target
directory or a handed-off B1 binary does not update that executable.

The Linux amd64/arm64 and macOS arm64 lockfiles pin HTTP source **0.2.12**,
PostgreSQL source **0.2.11**, SSE reaction **0.3.7**, and PostgreSQL/scriptfile
bootstrappers **0.2.14** by immutable OCI manifest digest and binary SHA256.
They come from the merged official main release
`70ca432c0f12623ab9b371b2d515180ccc80c2dd`, publication run
[35281678998](https://github.com/drasi-project/drasi-core/actions/runs/35281678998).
Both plugin and host SDK crates are **0.11.2**, with independently versioned
C ABI **0.14.0**. ABI 0.11/0.13 caches are incompatible, not fallbacks.
The shared installer runs the existing
`plugin install --from-config --locked` with `verifyPlugins: true` and
independently checks every downloaded binary hash. Missing tools, wrong
hashes, unsuccessful signatures and unavailable platform pins fail the gate.

The harness loads the existing database schema, replaces only its own sample
data with the six fixed stocks, two positions, two watchlist entries and two
completed orders, then recreates only its own replication slots. This prevents
old seed inserts from racing the deliberately small bootstrap snapshot. It
uses fixed bootstrap prices, explicit subsequent price updates and bounded
observable readiness, not the random price generator. Secrets are generated
per run, passed through environment variables, and not written into the
server configuration.

Every run owns fresh containers, a uniquely named Docker network, loopback
ports, plugin/state/WAL directories and child processes. Published Docker ports
are allocated by Docker; native ports are allocated independently of normal
demo ports. The browser runner remaps network addresses only. For the native
SSE reaction, its bind host/port are also redirected to an isolated loopback
listener; query definitions, query results and SSE payloads are untouched.
A transparent streaming proxy forwards headers and bytes unchanged, then
closes the connection to simulate an outage while the real database/source
continue changing. It never manufactures deltas.

Readiness limits are 60 seconds per service and 300 seconds for signed plugin
installation. The browser scenario is bounded to 120 seconds (180-second outer
watchdog). The test waits for an observed server-side update before reconnect,
requires the correct aggregate snapshot and financial total, and forbids page
navigation during recovery. Cleanup targets only stored child PIDs, container
IDs (including their anonymous volumes) and the run's network. Drasi shutdown
uses SIGINT. Completed `.test-runtime/live-*` directories intentionally retain
diagnostics; remove a specific finished directory when no longer needed.

Failure artifacts include service/install logs, runtime/lock/binary provenance,
`source-provenance.json` with engine Git revision and resolved dependency sources,
`loaded-plugins.json` with actual ABI/hash/status,
raw `server-rest.json`, unmodified SSE `data` strings in `server-sse.ndjson`,
the application's mutation transcript and Playwright traces/screenshots/video.
These actual server records must never be replaced by the synthetic fixture
projections. In particular, preserve SSE row signatures as raw text when
recording: some exceed JavaScript's safe integer range.

The successful isolated B1 candidate runs and earlier integrated P1 run are
historical evidence, not a substitute for running this gate on the current
branch's own rebuilt binary. The default gate must still prove singleton
2000/cost 1800/count 2, live 2050, existing-resource reload 2050, and
offline/reconnect 2150 without navigation or historical-row selection.

Historical SSE 0.3.6 observations include `message` events with
`queryId`/`results`/`timestamp`, `ADD`, `DELETE`, `UPDATE` and lowercase
`aggregation` results. Aggregation carries `before`/`after` without `data`;
update also carries `data`. ABI compatibility alone does not establish wire
compatibility; current SSE 0.3.7 must pass the actual gate independently.
Retain each run's raw observations separately; do not normalize them into
synthetic fixtures or infer guarantees for unexercised protocol paths.

### Older image comparison, not a passing fallback

The separately pinned official Docker image in `test/live/runtime-pins.json`
is historical. For a faithful comparison, use a separate owned checkout of P1
commit `a8dd2f68dab9fb7ccbd982dfb6a3f309e36f0059`, including its matching
platform locks and shared ABI-verification policy, then run
`P1_RUNTIME=image npm --prefix examples/trading/app run test:live` there.
Overriding only `P1_PLUGIN_LOCK` in the current checkout is insufficient: the
current shared ABI 0.14 verifier correctly rejects that image's ABI 0.11 before
the financial scenario. Do not weaken the current policy to run the old image.
Neither the image pin nor the original failure recordings have been rewritten.
That image is version 0.2.1 but comes from commit
`f78d9f11993eb96a2707617b727949dab2f33ab7` with core **0.5.7**, not the #119
checkout. Neither a mutable `v0.2.1` tag nor the GitHub release executable is an
equivalent pin.

The older image reproduced a real baseline problem: after observing the
processed $2,150 portfolio total on the server, its aggregate REST snapshot
contained three rows with totals **$2,000, $2,150, $2,000**. Reconnect restored
the individual positions correctly, but the UI summary displayed **$2,000**.
The singleton-snapshot and $2,150 UI assertions stay mandatory and fail; the
harness does not filter historical rows, guess the newest aggregate or refresh
the page to hide the discrepancy.

### Historical #119 reproduction

The bounded comparison rebuilt the unchanged #119 server at
`a2b648062a4c55e036d68b6f26bf73b4e773bcf1` using `cargo build --locked` after
building its UI. Native macOS arm64 plugins were independently signature-,
ABI-, schema- and hash-verified. The full Trading scenario **failed with exit
code 1**; no skipped/expected-failure marker or success fallback is used.
Initial/reload assertions are soft only so the test can collect later
diagnostics: they still fail the gate. The final $2,150 assertion is unchanged.

| Evidence | Exact comparison |
| --- | --- |
| Server / library / core | 0.2.1 / 0.8.9 / **0.5.8** |
| SDK / C ABI | 0.10.0 / 0.11.0 |
| Server executable SHA256 | `f38e92ba7245686779a5501538567b48888df51f705a857f983efc6028922e9b` |
| Cargo.lock SHA256 | `391bd37ead8424f3430db92f0cc522092083d4011b19fd00bc75f2450d355cad` |
| Plugin source revision | `e05938237fd8c2c8a46bb40580c6971e53088fce` |
| Plugin digests / hashes | [`plugins-darwin-arm64.lock`](app/test/live/plugins-darwin-arm64.lock); Linux equivalents are alongside it |

The original reproduction used #119's registry engine, not today's pinned
default source. Its exact historical inputs and the existing query were:

```cypher
    MATCH (p:portfolio)-[:OWNS_STOCK]->(s:stocks)-[:HAS_PRICE]->(sp:stock_prices)
    WITH sum(sp.price * p.quantity) AS totalValue,
         sum(toFloat(p.purchase_price) * p.quantity) AS totalCost,
         count(p) AS positionCount
    RETURN totalValue,
           totalCost,
           (totalValue - totalCost) AS totalProfitLoss,
           CASE WHEN totalCost > 0
                THEN ((totalValue - totalCost) / totalCost * 100)
                ELSE 0
           END AS totalProfitLossPercent,
           positionCount
```

Its exact request string, source subscriptions and joins are preserved in
[`app-mutations.json`](app/test/fixtures/recorded/a2b6480-core-0.5.8/app-mutations.json).
With AAPL 10 shares at $110 and MSFT 5 shares at $180, the positions total
$2,000 and cost $1,800. The REST summary already contains two rows, including a
one-position $1,100/$800 intermediate result. Updating AAPL to $115 delivers a
correct **$2,050** summary via real SSE. Reload reuses the existing resources
but displays **$2,000**. While disconnected, change AAPL to $125 and delete
MSFT from the *watchlist* (not the portfolio). The test waits until REST
explicitly contains the processed $2,150 aggregate before reconnecting.

The settled summary response still contains totals **[2000, 2150, 2000]**;
all three rows claim two positions and $1,800 cost. After reconnect, portfolio
rows show $1,250 and $900, and the watchlist correctly reflects its deletion,
but the summary displays **$2,000**, not **$2,150**. There is no page navigation
during recovery. This is not an arbitrary sleep or an unprocessed input.

Sanitized, unmodified service recordings (only disposable seed data) are under
[`app/test/fixtures/recorded/a2b6480-core-0.5.8/`](app/test/fixtures/recorded/a2b6480-core-0.5.8/):
`server-rest.json` records raw response bodies and observation times;
`server-sse.ndjson` preserves original SSE data strings;
`ui-observations.json` records initial/live/reload/reconnect text;
`runtime.json` records exact provenance. They document a **defect**, not golden
expected results. They are never substituted for live responses.

This historical defect is tracked by the existing
[drasi-project/drasi-core#680](https://github.com/drasi-project/drasi-core/issues/680).
The earlier compatible prerequisite used
[drasi-project/drasi-core#934](https://github.com/drasi-project/drasi-core/pull/934);
its reports remain historical. The current #204 development prerequisite uses
the exact `211d0f2a` source from drasi-project/drasi-core#810 with the newer
registry graph. No released fix or data migration is claimed. The current gate must prove the
correct singleton 2000 -> live/reload 2050 -> offline/reconnect 2150 result on
this branch's own rebuilt server; earlier handed-off binary results are not a
substitute.

### Product gates versus retained caveats

Current-head evidence is recorded on #200/#201, including the normal merge
predecessor, server/core/manifest/lock/binary/plugin provenance, actual live
results and cleanup. `make test-all` retains the original smoke script: its
8 passes / 28 configuration-dependent skips are reported separately, not
presented as coverage of every plugin. No new skips or expected failures are
added to the mandatory Trading gate.

Classify YAML from the actual current workflow and run. Newer main gates the
agent to the top stack layer, so this layer normally skips it: that is not
executed validation, a pass, or the older HTTP 400/504 outcome. Historical
infrastructure failures remain recorded; no model/credential changes or
approval bypass is made here. The selected host audit and its 15 existing
warnings are distinct from unused-workspace findings and dependencies embedded
in signed plugin binaries; no broader audit pass is inferred. See
[the scoped server security disposition](../../docs/server-security-dependencies.md).
These caveats remain visible when discussing readiness for dependent
development; they do not authorize merge, automatic merge or publication.
