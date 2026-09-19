# Trading regression baseline

This is the P1 foundation for [#200](https://github.com/drasi-project/drasi-server/issues/200).
Its original behavior baseline is [#119](https://github.com/drasi-project/drasi-server/pull/119)
at `a2b648062a4c55e036d68b6f26bf73b4e773bcf1`; its current predecessor is the
separately approved [B1 prerequisite #204](https://github.com/drasi-project/drasi-server/pull/204)
at `6f888956cca131992ed7e656387f74cc3053652b`. It protects the existing Trading
application, not a second demo. P1 does not redesign its query, package API,
CSS or components. The reviewed engine/security/source-backed setup changes
come from B1, whose history and policies are retained.

**Default source integration is now required, not a diagnostic overlay.**
The gate uses the pinned compatible source described below; the original #119
failure recordings remain [historical defect evidence](#historical-119-reproduction),
never new golden expectations. Readiness for another development layer requires
the actual current-branch product gates. It is not a claim that all external
checks or the entire unused core workspace are green, nor merge authorization.

P2 ([#162](https://github.com/drasi-project/drasi-server/issues/162)) builds on
the exact P1 head `a8dd2f68dab9fb7ccbd982dfb6a3f309e36f0059`. The package now
connects to explicit existing references with GETs only; Trading owns automatic
setup in `src/drasi/ensureTradingResources.ts` and lifecycle orchestration in
`TradingProvider.tsx`. This does not change the business/visual baseline below.

P3 ([#163 Part A](https://github.com/drasi-project/drasi-server/issues/163))
builds directly on P2's exact `3e9833ccb2a4c5fb00a4cf2a2bf1ab9e8c14ae97`.
It completes the public client/transport/type/entrypoint and material
configuration contract. P3's historical passes do not imply acceptance of the
later Part B identity, canonical-delta or snapshot/reconnect/stale contracts.
The Part B consumer changes and focused proof obligations are described below;
the measured historical evidence remains unchanged.

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

The original P1 artifact baseline was 110,691 bytes packed, 66,865 bytes package ESM, 68,860
bytes CJS, 18,746 bytes declarations and 10,043 bytes package CSS. Clean Trading
JS is 223,574 bytes (65,775 gzip), CSS 21,034 bytes (5,163 gzip).
`check-baseline.mjs` rejects any coverage drop or artifact growth above 2%;
explain and review baseline updates instead of silently accepting them.
The Linux runner and CI both run this check and retain
`test-results/measured-baseline.json`. V8 counters can differ across Node
versions, so compare the pinned environment rather than mixing host coverage.

P3 introduces multiple entrypoints and shared chunks. Package ESM/CJS metrics
therefore sum **every shipped file of that format**, including entrypoint
wrappers, rather than measuring only the now-small root barrel. Declaration
bytes sum all `.d.ts` files; matching `.d.cts` files are verified separately,
not double-counted in that established metric. Source maps remain included in
tarball bytes. Two extra metric-policy cases prove shared chunks are counted
and missing runtime/declaration formats fail. The original four threshold
tests and unchanged 2% policy remain authoritative.

Browser coverage is scenario-based; these percentages are Vitest/V8 only and
must not be presented as browser or real-server coverage.

### Measured P2 contract cost

The same pinned Linux gate measured P2 after all 26 browser scenarios and the
five unchanged, zero-differing-pixel PNG files passed. Only the **artifact size
baseline** was advanced, with the original P1 bytes retained in
`artifactChange.p1Sizes`. The 2% growth policy, every P1 coverage floor, all
included source files, dependency locks and visual expectations are unchanged.

| Artifact | P1 bytes | P2 bytes | Reason for growth |
| --- | ---: | ---: | --- |
| Packed tarball | 110,691 | 125,325 | Runtime, declarations, source maps and the reference/error/ownership documentation |
| Package ESM / CJS | 66,865 / 68,860 | 72,992 / 75,038 | Resource DTO guards, instance paths, typed errors, bounded transport/snapshot handling and controlled binding |
| Declarations | 18,746 | 21,140 | Explicit references, read DTOs, error codes/identity, timeouts and lifecycle binding |
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

### Measured P3 transport/type contract cost

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

### Measured P4 result/state contract cost

P4 starts at exact P3 `a0569c2ae7b17f51996f431cf2264c636a19f3d1`.
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
0.8.9, SDK/host/FFI 0.10.0, index 0.5.8 and GQL 0.3.6 stay selected. This is
the full compatible source backport, not a published 0.5.8 fix, the rejected
0.5.9 hook API, or the earlier disposable three-file overlay. See
[B1's full provenance and consumption boundary](../../docs/engine-prerequisite.md).

`source_provenance.py` invokes the shared source verifier and resolved-SDK
policy, checks the caller's commit/lock and exact engine/parser origins, and
rejects a different caller plugin lock. Native installation reuses
`scripts/install_plugins.py`, rather than a second installer. Actual loaded
plugin status/hash/version/ABI metadata is validated by that same shared
policy and retained in `loaded-plugins.json`.

For a source-free packed Trading consumer, set `P1_SOURCE_ROOT` to the original
server checkout. It supplies **backend build provenance and setup helpers
only**; no package source alias or Tailwind scan is introduced. The frontend
still consumes the tarball. CI prepares the pinned sibling before its build,
passes that checkout explicitly, and triggers on Cargo, core pin, shared helper,
server/UI, package and Trading changes for dependent PR bases.

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
bash scripts/prepare-core.sh
python3 scripts/plugin_origin.py mode
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

The Linux amd64/arm64 and macOS arm64 lockfiles pin HTTP source **0.2.8**, PostgreSQL source **0.2.7**, SSE
reaction **0.3.4**, and PostgreSQL/scriptfile bootstrappers **0.2.10** by immutable
OCI manifest digest and binary SHA256. Their SDK crate is **0.10.0** and their
independently versioned C ABI is **0.11.0**. The shared installer runs the existing
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

### Older image comparison, not a passing fallback

`P1_RUNTIME=image npm --prefix examples/trading/app run test:live` runs the
separately pinned official Docker image in `test/live/runtime-pins.json`.
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
