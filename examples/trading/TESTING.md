# Trading regression baseline

This is the P1 foundation for [#200](https://github.com/drasi-project/drasi-server/issues/200).
Its original behavior baseline is [#119](https://github.com/drasi-project/drasi-server/pull/119)
at `a2b648062a4c55e036d68b6f26bf73b4e773bcf1`; its current predecessor is the
separately approved [B1 prerequisite #204](https://github.com/drasi-project/drasi-server/pull/204)
at `e07cc709658617d741881a0568ab3a6b5e9ff846`. It protects the existing Trading
application, not a second demo. P1 does not redesign its query, package API,
CSS or components. The reviewed engine/security/source-backed setup changes
come from B1, whose history and policies are retained.

**Default builds now use verified published registry dependencies, without a
sibling repository or source overlay.** The gate uses the released graph
described below; the original #119
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
- **KB-02 — duplicate-symbol identity:** the portfolio displays rows by symbol,
  and edit/delete look up the first API position with that symbol. Order query
  options also prefer symbol to order ID. Fixtures intentionally use unique
  symbols; these tests do not establish duplicate-symbol correctness. Stable
  identity work in #163 must explicitly address this instead of copying the
  limitation into a new contract.
- **KB-03 — legacy unidentified routing:** the app's adapter recognizes some
  snake_case shapes while current queries return camelCase, and can fan price
  rows out to several queries. Synthetic keyed batches do not validate that
  heuristic against a real SSE plugin. Recorded/live results must stay separate.
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

The app runner also includes ten native-version setup guards. These are
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

## Mandatory real-server gate

The primary gate builds the **checked-out server and its Cargo.lock**, not an
arbitrary executable with the same version number. Docker provides a disposable
PostgreSQL database; the original `mock-generator/trading_api.py` is run with
Flask on a dedicated loopback port. The actual built Trading app creates its
queries/reaction and consumes real REST/SSE data. No synthetic endpoints are
enabled in this browser run.

### Published registry provenance and shared setup

The default has no `.drasi-core-revision`, `prepare-core.sh`, source patches
or mandatory sibling fetch. Server 0.2.3 uses published core/functions 0.5.10,
library 0.9.3, host/plugin/FFI SDK 0.11.3, index 0.6.4, middleware 0.5.11,
noop/application bootstrap 0.2.15, application reaction 0.3.13, state store
0.2.8 and WAL 0.2.10. AST 0.3.5 and Cypher/GQL 0.3.6 remain the published
parser versions. An absent, unrelated or dirty sibling is not selected or
modified. Deliberately selected matching local-SDK development remains a
separate mode; it cannot pass this published-runtime gate.

`source_provenance.py` reuses `scripts/plugin_origin.py`'s central
`REGISTRY_PACKAGES` policy for all 17 exact name/version/source/checksum
identities. It checks the caller's server commit and lock, rejects partial,
duplicate, old, local/Git or patched-default identities, and verifies each
official `.crate` archive's SHA256 against that policy. Resolved package source
contents are checked against the archive, not assumed to match from a version
number. Provenance records archive URLs, hashes, `.cargo_vcs_info.json`,
verified file counts and Rust source hashes. The released family identifies
`22125bf1d66062533b832a166fe4a51079a23d6e`; the unchanged parser archives
retain their actual older VCS identity
`8f0ed49802ab0f2d62ce834aafe7fe7e5861ee76`. No sibling `engineGit` or
`engineSourceVerifiedClean` claim is emitted for registry builds.

The official middleware archive includes distinct `README.md` and `readme.md`
entries. On filesystems where those exact paths demonstrably alias the same
file, Cargo retains the later entry. Verification records both archive hashes
and checks the retained bytes; on case-sensitive filesystems it checks both
files separately. Symlinks, hard-link ambiguity, changed Rust source and
unrelated path aliases are not exemptions.

The checksum-verified core archive contains the final drasi-project/drasi-core#810
aggregate/numeric grouping, compound-key, precision/default/lazy-state and
terminal corrections. Library 0.9.3 includes the named MessagePack writer.
See [the reviewed release and persistent-state boundary](../../docs/main-runtime-integration.md);
older `211d`/`1284`, ABI and raw snapshot reports remain historical and are not
relabeled as released execution.

The caller's plugin lock must still match the exact reviewed platform lock.
Native installation reuses
`scripts/install_plugins.py`, rather than a second installer. Actual loaded
plugin status/hash/version/ABI metadata is validated by that same shared
policy and retained in `loaded-plugins.json`. The native CLI's reported server
and SDK versions must also match the verified lock/resolved SDK; a stale
`target/debug/drasi-server` fails before any test services start. The success
banner uses that verified binary version, not the historical image version.

For a source-free packed Trading consumer, set `P1_SOURCE_ROOT` to the original
server checkout. It supplies **backend build provenance and setup helpers
only**; no package source alias or Tailwind scan is introduced. The frontend
still consumes the tarball. CI verifies the locked registry origin before its
build, passes that checkout explicitly, and triggers on Cargo, Make, shared
helper/pins, server/UI, package and Trading changes for dependent PR bases.

Prerequisites: a POSIX host, Docker, the repository's Rust toolchain, Node 22,
Python 3.13, and access to crates.io archives and GHCR/Sigstore for source and
signed-plugin verification. A cached archive is still hashed; a missing one
is fetched only from its exact official crates.io URL, never from a source
checkout. Missing or mismatched source evidence fails before services start.

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

The Linux amd64/arm64 and macOS arm64 lockfiles pin HTTP source **0.2.13**,
PostgreSQL source **0.2.12**, SSE reaction **0.3.8**, and PostgreSQL/scriptfile
bootstrappers **0.2.15** by immutable OCI manifest digest and binary SHA256.
They come from the merged official main release
`22125bf1d66062533b832a166fe4a51079a23d6e`.
Both plugin and host SDK crates are **0.11.3**, with independently versioned
C ABI **0.14.0**. The Darwin SSE signature was repaired at
`32fc9052f917666af14663b6c21f6e44d32778ae` under the unchanged trusted publisher;
its binary source and signing revision are distinct. Older versions do not
become valid just because they report ABI 0.14: the coherent released source
family supplies required event sequence values. There is no old SSE/source
or ABI 0.11/0.13 fallback.
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
`source-provenance.json` with verified registry archive/VCS/content identities,
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
compatibility; current SSE 0.3.8 must pass the actual gate independently.
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
The earlier compatible and temporary prerequisites used
[drasi-project/drasi-core#934](https://github.com/drasi-project/drasi-core/pull/934)
and `211d0f2a` from drasi-project/drasi-core#810; their reports remain historical.
The current #204 prerequisite selects the checksum-verified published release.
The current gate must independently prove the
correct singleton 2000 -> live/reload 2050 -> offline/reconnect 2150 result on
this branch's own rebuilt server; earlier handed-off binary results are not a
substitute.

### Persistent-state upgrade is not automatic repair

This test uses only fresh owned data. Upgrading existing affected numeric
groups requires complete reconstruction from authoritative bootstrap or
retained replay, including ordinary integer keys and numbers nested in lists
or objects. Grouping/default/current state, lazy min/max sets, query indexes
and outputs must be reconstructed together. Back up state and verify source
history first; clearing output alone or retaining old lazy/index state is not
a migration.

Source ranks now participate in the query configuration hash, which causes a
one-time old-hash mismatch and rebootstrap. That mechanism and the named
MessagePack writer do not repair malformed old positional records. Strict
errors remain visible. No user-data deletion, automatic repair or expanded
recovery guarantee is part of this P1 change.

### Product gates versus retained caveats

Current-head evidence is recorded on #200/#201, including the normal merge
predecessor, server/archive/manifest/lock/binary/plugin provenance, actual live
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
