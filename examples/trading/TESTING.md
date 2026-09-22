# Trading regression baseline

This is the P1 foundation for [#200](https://github.com/drasi-project/drasi-server/issues/200).
Its original behavior baseline is [#119](https://github.com/drasi-project/drasi-server/pull/119)
at `a2b648062a4c55e036d68b6f26bf73b4e773bcf1`; its current predecessor is the
separately approved [B1 prerequisite #204](https://github.com/drasi-project/drasi-server/pull/204)
at `7b9601784f37ca3680359b7fc29a52f9f286d15f`. It protects the existing Trading
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
now incorporates exact P1 `e361d1c3369e83213f0bb164ab2be66569ea7e18`, including
the approved main-runtime and official signed-plugin changes below, while
retaining original P2 `3e9833ccb2a4c5fb00a4cf2a2bf1ab9e8c14ae97` ancestry.
The package still
connects to explicit existing references with GETs only; Trading owns automatic
setup in `src/drasi/ensureTradingResources.ts` and lifecycle orchestration in
`TradingProvider.tsx`. This does not change the business/visual baseline below.

P3 ([#163 Part A](https://github.com/drasi-project/drasi-server/issues/163))
originally built on P2 `3e9833ccb2a4c5fb00a4cf2a2bf1ab9e8c14ae97`.
Its normal parent integration retains original P3
`a0569c2ae7b17f51996f431cf2264c636a19f3d1` and incorporates exact updated
P2 `f8e06c3415d5190f71b2f9ad3e294b7fe2d8c467`, without importing P4-P7.
It completes the public client/transport/type/entrypoint and material
configuration contract. P3's historical passes do not imply acceptance of the
later Part B identity, canonical-delta or snapshot/reconnect/stale contracts.
The Part B consumer changes and focused proof obligations are described below;
the measured historical evidence remains unchanged.

P4 normally integrates exact updated P3
`39f82c9e7124c512eaad2047bda3222732387297`, retaining original P4
`20561c13dd74929855dfbe605bb1fac23e5a49f4` ancestry and the complete result,
identity, adapter and bounded-recovery contract. This brings the approved
server 0.2.3 / SSE 0.3.6 / ABI 0.13 runtime into the owning P4 branch, not
P5-P7 product features. Current evidence is recorded separately from the
original ABI 0.11 captures and historical measurements below.

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

#### P5 approved parent integration and accounting

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

The app runner also includes seven native-version setup guards. These are
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
### P2 approved main-runtime update

The normal merge of P1 `e361d1c3369e83213f0bb164ab2be66569ea7e18` changes no
P2 package or Trading runtime source. Its own rebuilt server/UI and new signed
ABI 0.13 plugins pass the same real Trading scenario with the singleton totals
above; the new raw capture has 17 SSE events with the observed envelope/result
shapes documented below. Earlier raw captures remain historical, not a substitute
for this runtime proof.

The updated layer passes 83 package tests, 64 Trading tests (57 existing plus
seven runtime-version guards), nine artifact-policy tests and 53 tooling tests.
The source-free Linux gate passes all 26 browser scenarios and the original five
exact PNG files. Locked Rust tests report 809 passes / 32 existing ignores;
`make test-all` reports 840 passes / one existing ignored doctest, with the
separate limited plugin smoke reporting eight passes / 28 unconfigured skips.
Strict Clippy/fmt and the selected host audit pass, retaining its 15 existing
warnings. Final-head CI, binary/lock hashes and independent packed live proof
are recorded on #205, rather than relabeling older results as current evidence.

### P3 approved main-runtime integration

P3 normally merges exact updated P2
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

The own rebuilt merge source `5988b1d82cbb4d24dca2d4cb2763c0572d564bf4`
passes the unchanged actual Trading flow on server 0.2.3, with singleton
2000/cost 1800/count 2 -> 2050 live/reload -> 2150 offline/reconnect,
CRUD/live/deletes and no recovery navigation. Its binary SHA-256 is
`6327ce9a6e938451f50174d634efdf795f011044b5b8eeeafbd8204b74fb3b28`;
lock SHA-256 is
`63565b6a959f0c51a0cec916381866511be8da75f81d21dccc31ba7b2748b414`.
Actual full-view query/reaction/snapshot records are retained separately in
`dev-tools/react/test/fixtures/server-v1-0.2.3/contract.json` and run through
the same read-only guards as the original 0.2.1 records.

Fresh checks report 221 package tests, 64 Trading tests, 12 artifact-policy
cases, 53 tooling tests, and 809 locked Rust passes / 32 existing ignores.
Strict Clippy/fmt and the selected audit pass with zero vulnerabilities and
15 retained warnings. The first local full Linux run had a 375-pixel mobile
ticker-only difference; an unchanged targeted visual rerun and a subsequent
full 26-scenario rerun passed all five original images. No product, screenshot,
timeout, readiness or clock assertion was changed to obtain those passes.
Final packed/current-head CI evidence is recorded on #206 rather than inferred
from any lower owner's binary or an old plugin cache.

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

### P4 approved parent integration and accounting

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

Current rebuilt-source and packed/browser/live results belong to #207. The
historical P4 measurements and old runtime recordings are not relabeled as
current evidence, and current ABI 0.13 guards must not be weakened to run an
old ABI 0.11 cache or historical image. Reproduce old results only with the
complete matching historical harness/policy described below.

The own default `target/debug/drasi-server` was rebuilt before Rust integration
tests. Normal merge `3b39126ae36c9a0da96a4c0e29ad812b181636f6` has exact
parents original P4 `20561c13` and approved P3 `39f82c9`. Its current
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
assertions and exercise these current records through the unchanged normalizer
and default SSE transport. In particular, current lowercase aggregation carries
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

## Mandatory real-server gate

The primary gate builds the **checked-out server and its Cargo.lock**, not an
arbitrary executable with the same version number. Docker provides a disposable
PostgreSQL database; the original `mock-generator/trading_api.py` is run with
Flask on a dedicated loopback port. The actual built Trading app creates its
queries/reaction and consumes real REST/SSE data. No synthetic endpoints are
enabled in this browser run.

### Integrated source and shared setup

Prepare/verify the exact `.drasi-core-revision` **before** any locked Rust build:
`1284e9f648634c1faa73fd897a21c2712bb0cbbe`. The default manifest selects only
the sibling engine 0.5.8 / AST 0.3.5 / Cypher 0.3.6 paths. Registry library
0.9.1, SDK/host/FFI 0.11.0, index 0.6.1 and GQL 0.3.6 stay selected with
server 0.2.3. This is the full compatible source backport, not a published
0.5.8 fix, the rejected
0.5.9 hook API, or the earlier disposable three-file overlay. See
[B1's full provenance and consumption boundary](../../docs/engine-prerequisite.md)
and [the approved main-runtime matrix](../../docs/main-runtime-integration.md).

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

The Linux amd64/arm64 and macOS arm64 lockfiles pin HTTP source **0.2.11**,
PostgreSQL source **0.2.10**, SSE reaction **0.3.6**, and PostgreSQL/scriptfile
bootstrappers **0.2.13** by immutable OCI manifest digest and binary SHA256.
They come from the merged official main release
`3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`
(drasi-project/drasi-core#789). Their SDK crate is **0.11.1**; it and the host
SDK crate **0.11.0** both use the independently versioned C ABI **0.13.0**.
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

Actual SSE 0.3.6 observations include `message` events with
`queryId`/`results`/`timestamp`, `ADD`, `DELETE`, `UPDATE` and lowercase
`aggregation` results. Aggregation carries `before`/`after` without `data`;
update also carries `data`. ABI compatibility alone does not establish wire
compatibility. Retain each run's raw observations separately; do not normalize
them into synthetic fixtures or infer guarantees for unexercised protocol paths.

### Older image comparison, not a passing fallback

The separately pinned official Docker image in `test/live/runtime-pins.json`
is historical. For a faithful comparison, use a separate owned checkout of P1
commit `a8dd2f68dab9fb7ccbd982dfb6a3f309e36f0059`, including its matching
platform locks and shared ABI-verification policy, then run
`P1_RUNTIME=image npm --prefix examples/trading/app run test:live` there.
Overriding only `P1_PLUGIN_LOCK` in the current checkout is insufficient: the
current shared ABI 0.13 verifier correctly rejects that image's ABI 0.11 before
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
The separately approved source prerequisite now comes from #204 and
[drasi-project/drasi-core#934](https://github.com/drasi-project/drasi-core/pull/934).
No released fix or data migration is claimed. The current gate must prove the
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

The unchanged YAML agent can fail HTTP 400 for an unsupported model before
validation. That is external infrastructure, not a passed check or a reason
to change models/credentials here. The selected server audit reports zero
vulnerabilities with 15 existing warnings; the unused legacy core workspace's
h2/Azure findings remain red and are not waived. See
[the scoped server security disposition](../../docs/server-security-dependencies.md).
These caveats remain visible when discussing readiness for dependent
development; they do not authorize merge, automatic merge or publication.
