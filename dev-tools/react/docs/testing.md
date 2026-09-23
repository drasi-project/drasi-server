# Testing and compatibility

[Package overview](../README.md)

## What each test layer proves

Coverage is a guard, not a reason to add trivial tests. Choose a behavioral
check that fails for the defect or missing consumer flow you are changing.
The source paths below are relative to `dev-tools/react` in the repository;
test source and runnable examples are not part of the installed tarball.

| Public surface | Meaningful checks | Boundary |
| --- | --- | --- |
| `DrasiClient`, transport and adapters | `test/DrasiClient.test.ts`, `transport.test.ts`, `resources.test.ts`, `results.test.ts`, `subscription.test.ts`: GET-only ownership, typed faults, cancellation, raw-key updates/deletes, snapshot overlap and bounded retry. Versioned real wire fixtures are kept separately. | Injected endpoints prove client behavior, not every server/plugin version. |
| Providers and query hooks | `DrasiContext.test.tsx`, `DrasiLifecycle.test.tsx`, `QueryState.test.tsx`, `concurrentOptions.test.tsx`: real provider/client/hook pipeline, scoped errors/retry, retained data, committed callbacks, StrictMode and cleanup. | They do not promise a shared query cache or authoritative REST/SSE cursor. |
| Tables, sorting and composition | `DataTable.test.tsx`, `QueryTable.test.tsx`, `QueryTableComposition.test.tsx`, `NonTradingContract.test.tsx`, `tableOrdering.test.tsx`: stable rows, slots/actions, controlled sorting, non-Trading types and shared presentations without a second subscription. | Native browser layout/keyboard behavior is checked separately. |
| Modal, themes, motion and animation | `Modal.test.tsx`, `ModalLoading.test.tsx`, `portalTheme.test.tsx`, `rowAnimationRestart.test.tsx`, `reducedMotion.test.tsx`: focus/dismissal ownership, scoped tokens, cleanup and restart identity. | The inherited browser gate also checks native animation objects, public-clock replay, portals and five original images. |
| Installed consumer API and docs | `test/public-contract/`: ESM/CJS, both declarations, strict SSR console/global traps, names/maps, tree shaking, React-free client, hooks without CSS, all 16 literal recipes and real single-React installs. | Recipes compile from installed documents; compilation alone is not live execution. |
| Runnable examples | `examples/test/showcase.spec.ts` checks clearly simulated states and row-action Modal/focus composition with no API calls. `examples/test/live.spec.ts` runs the table, hooks and `QueryTable` on actual pre-created resources, including update/delete/empty/recovery and isolated client-projection failure/retry. | The projection-error control is deliberately client-side, not a server or protocol outage. |

## Commands from the repository root

Build the package explicitly and use the
[documented installation path](getting-started.md#installation-and-entrypoints).
Direct example commands assume the Node 24.19.0/npm 11.17.0 copied-local setup.
On Node 22.20.0/npm 10.9.3, use the documented tarball consumer and run its
example commands from `dev-tools/react/examples` within that consumer.
These commands use existing locked tools; do not introduce source aliases or
let a consumer lifecycle script rebuild the library:

```sh
npm --prefix dev-tools/react run typecheck
npm --prefix dev-tools/react run test:coverage
node --test dev-tools/react/test/public-contract/*-check.mjs
npm --prefix dev-tools/react/examples run typecheck
npm --prefix dev-tools/react/examples test
npm --prefix dev-tools/react/examples run build
npm --prefix dev-tools/react/examples run test:browser
```

For the complete installed/browser gate, run
`npm --prefix examples/trading/app run test:browser:linux`.
It builds an ordinary tarball, creates a locked source-free consumer, compiles
the installed guides and runs the inherited browser/visual matrix plus the
canonical server-free showcase. It does not replace actual backend validation.

Follow the [repository example setup](https://github.com/drasi-project/drasi-server/blob/main/dev-tools/react/examples/README.md#run-the-real-example)
to build this checkout's UI/server and prepare the approved source/plugins,
then run `npm --prefix dev-tools/react/examples run test:live`. The normal and
source-free paths use the same example-owned startup. Trading's separate
[real financial/CRUD gate](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md)
remains mandatory. Finish heavy Rust compilation before timing-sensitive
browsers; do not hide failures by increasing bounds or refreshing image files.

Keep full raw failures, axe violations/incomplete results, source/binary/pin
provenance and exact test counts. Passing automation is not human screen-reader
acceptance. New example entries must have explicit measured asset scope;
retain every existing entry/shared/lazy asset and the headless zero-CSS check.

## Verified compatibility

[P7 integration evidence](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md#p7-current-development-source-integration)
records the normal P6 merge, retained original/quality ancestry and contracts,
checks and full artifact accounting. Claims exclude untested browser/AT versions;
historical P6/P7 passes are not current proof or human AT approval.

| Surface | Exercised configuration |
| --- | --- |
| React / React DOM | **18.3.1**, including real providers, StrictMode, unmount, equivalent/material rerenders and SSR. React 19 is not yet claimed. |
| Node / tooling | **22.20.0** pinned Linux gate; **24.19.0** native development gates. TypeScript **5.9.3** (package) / **5.9.2** (Trading), tsup **8.5.1**, Vite **5.4.19**, Vitest **3.2.7**, committed lockfiles. |
| Browsers | Playwright **1.56.1** Chromium, Firefox and WebKit in the pinned Linux/amd64 image from [Trading TESTING.md](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md). This is not an all-browser/all-version claim. |
| Server | Current development runtime: **0.2.3**, registry library **0.9.2**, index **0.6.3**, engine **0.5.9** from the user-approved temporary source `211d0f2a79aa2ad0f7cb841937f52013fe95ded6` (drasi-project/drasi-core#810), AST **0.3.5**, Cypher/GQL **0.3.6**. Only engine/AST/Cypher are path-selected; equal-version sibling SDKs are not consumed. |
| Plugin / ABI | Current signed SSE **0.3.7**, host/plugin/FFI crates **0.11.2**, native ABI **0.14.0**, from official merged release `70ca432c0f12623ab9b371b2d515180ccc80c2dd`. All six immutable platform/digest/hash/signature locks are inherited from the approved parent. No prior ABI cache fallback or trust relaxation. |
| Historical runtime evidence | Original server **0.2.1** / library **0.8.9** / SSE **0.3.4** / ABI **0.11.0**, and server **0.2.3** / library **0.9.1** / SSE **0.3.6** / ABI **0.13.0** records remain separately versioned. They are not current ABI 0.14 validation or a fallback for it. |

Protocol capabilities, not a guessed version string, determine acceptance.
Missing full-view fields, unsupported language/status/shape, wrong resource
identity or incompatible reaction membership fail with typed errors. No older
server fallback, query-language substitution or newer SDK/ABI upgrade is
attempted. See [engine prerequisites](https://github.com/drasi-project/drasi-server/blob/main/docs/engine-prerequisite.md) and
the [approved main-runtime integration](https://github.com/drasi-project/drasi-server/blob/main/docs/main-runtime-integration.md).
The client does not attest engine/plugin versions; a semantically wrong result
with a valid shape cannot be detected by DTO validation. Operator setup and
real-server provenance/gates supply the version evidence.
`211d0f2a` is unreleased development source: three aggregate and two additive
outbox paths, not stored-data repair. Registry library 0.9.2 still uses compact
records and `append`, not the new trim methods or drasi-project/drasi-core#909's
codec fix. No migration, dropping, broader recovery or publication is implied.

Historical `test/fixtures/server-v1/sse-0.3.4.ndjson` copies all **20 raw lines**
verbatim from Trading's `test/fixtures/recorded/a2b6480-core-0.5.8/server-sse.ndjson`.
Adjacent `contract.json` retains actual REST bodies and separate capture,
source-tag and serializer provenance. The tag
[`drasi-reaction-sse-v0.3.4`](https://github.com/drasi-project/drasi-core/tree/drasi-reaction-sse-v0.3.4)
resolves to **`ff2fde26d0f33adcec17b19db7d2533f80b0baab`**:
`lib/src/channels/events.rs::ResultDiff` defines noop, nullable aggregation-before
and before/after updates; `components/reactions/sse/src/sse.rs` emits
`queryId`/`results`/`timestamp`, **not internal `QueryResult.sequence`**.
The capture does not exercise every source-defined variant; an unused local
SSE 0.3.5 checkout is not evidence for this protocol.

Historical ABI 0.13 evidence remains separate:
`test/fixtures/server-v1-0.2.3/sse-0.3.6.ndjson` preserves all **17 raw CDP
records**; adjacent `sse-0.3.6.provenance.json` retains P4's merge revision
and source/manifest/lock/binary/signature hashes.
Both recordings still exercise the adapter; 0.3.6 also tests default transport
and query-ID routing: `ADD`/`DELETE` data, `UPDATE` before/after/data, lowercase
aggregation before/after **without data**, and unsafe numeric signatures.
Neither captures a shared cursor. These records and Part A's DTO fixture are
unchanged, not relabeled as ABI 0.14 proof. Current SSE 0.3.7 needs its own live
gate; native ABI compatibility alone does not establish wire semantics.

## Development and Trading verification

The source/type graphs are physically separated under `src/client`,
`src/react` and `src/components`; `src/types.ts` is only a root compatibility
barrel, not an import used by the client. From the repository root, use the
existing commands (install the locked package dependencies first as above):

| Command | Scope |
| --- | --- |
| `npm --prefix dev-tools/react run build` | ESM, CJS and both declaration formats. |
| `npm --prefix dev-tools/react run typecheck` | Package source types. |
| `npm --prefix dev-tools/react test` | Package Vitest behavior/regression tests. |
| `npm --prefix dev-tools/react run test:coverage` | Product-source coverage and subtree floors. |
| `npm --prefix dev-tools/react run dev` | Package build watcher, not an example web server. |
| `node --test dev-tools/react/test/public-contract/readme-examples-check.mjs` | README extraction/parser guards only; not installed recipe compilation. |

The tarball includes README, these five guides, CHANGELOG, LICENSE, NOTICE,
dist and CSS. Runnable example/test source remains repository-owned.
For contributions, use this repository's
[issue tracker](https://github.com/drasi-project/drasi-server/issues), not a
separate published-package repository. Keep changes at their owning boundary:

- [Package tests](https://github.com/drasi-project/drasi-server/tree/main/dev-tools/react/test) cover clients, transport/DTO validation, actual hooks
  and provider lifecycles, tables, modal/theme/motion and regression behavior.
- [Installed public contracts](https://github.com/drasi-project/drasi-server/tree/main/dev-tools/react/test/public-contract) are invoked by the
  documented [Trading packed-consumer gate](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md#reproducible-visual-and-packed-consumer-gate).
  They compile **literal** marked recipes from the installed tarball against
  its real exports and inspect runtime/declaration/SSR dependency graphs.
  Source aliases or locally rewritten copies are not a substitute.
- Preserve all 16 existing `@drasi-docs` recipes. Add each runnable TS/TSX fence
  with a first-line marker, register its name in
  `test/public-contract/readme-examples.mjs`, register every shipped guide in
  `test/public-contract/documents.mjs`, and retain/extend
  `readme-examples-check.mjs` guards. Do not suppress diagnostics or relax
  installed negative assertions to make a recipe pass.
- Real-server protocol fixtures in [test/fixtures/server-v1](https://github.com/drasi-project/drasi-server/tree/main/dev-tools/react/test/fixtures/server-v1)
  retain capture provenance; synthetic examples do not expand that evidence.
  Trading-specific contribution guidance is [app-owned](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/CONTRIBUTING.md).

`test/ResultRegression.test.jsx` is a portable behavioral proof run unchanged
on archived P3 **`a0569c2`** and the P4 implementation. All three cases fail on P3 with
observed wrong results and pass on P4:

| Regression | Observed P3 result | P4 result |
| --- | --- | --- |
| Projection omits the raw identity field | `null` instead of the projected row. | Projected value `1`, then removal by a sparse identity-only delete. |
| Official before/after update changes the key | Both old and new identities remain. | Only the new identity with value `2`. |
| Snapshot value `2` overlaps an older pending delta value `1` | Replay rolls the result back to `1`, without refreshing. | Reject the ambiguous candidate and accept value `3` through a bounded REST refresh. |

These tests use the real provider, hooks, client and transport pipeline with
injected REST/EventSource endpoints, not mocks that bypass those product
implementations. They prove these targeted regressions, not stronger handoff
semantics; the [undetectable delayed-event limit](reference.md#snapshotstream-consistency-and-limits) still applies.

Vitest coverage includes all `src/**/*.{ts,tsx}` product code. It enforces
**90% statements, lines and functions and 85% branches**, separately for
`src/client/**`, `src/react/**` and, starting in P6, `src/components/**`.
No product-source exclusions are used to meet these floors. They do not
replace the existing package/consumer gates or change artifact-size baselines.

Trading continues to consume built local exports. Its unchanged query
definitions, financial transforms, provisioning and tutorial snippets stay
app-owned. [Trading TESTING.md](https://github.com/drasi-project/drasi-server/blob/main/examples/trading/TESTING.md) documents
the locked clean-tarball gate, all three browsers, five original exact PNG images,
coverage/size budgets and authoritative real-server financial/CRUD/reconnect
assertions. Synthetic tests are not proof that a real plugin emits a shape.
Artifact accounting includes all entrypoints/shared chunks, both declaration
formats and all shipped CSS; historical single-format measurements remain
preserved separately. The 2% future-growth policy and coverage floors do not
change with this accounting correction.

Keep `"private": true`. P7 / #165 completes consumer documentation and adds
the linked example workspace without a new API or package release. Publishing,
repository transfer and release credentials/workflows remain separately
authorized; no Storybook site or new documentation toolchain is required.
These contracts and recorded P6 evidence do not mean all current checks have
passed or establish human acceptance or merge readiness.
