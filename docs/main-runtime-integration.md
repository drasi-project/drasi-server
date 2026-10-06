# Main runtime integration for the engine prerequisite

## Current released runtime

Default builds now use the published September 25 release from
[`22125bf1d66062533b832a166fe4a51079a23d6e`](https://github.com/drasi-project/drasi-core/commit/22125bf1d66062533b832a166fe4a51079a23d6e).
The temporary engine/AST/Cypher path overrides, revision file and mandatory
source fetching are removed. A clean server checkout builds without any sibling
repository, including in Docker, CI, devcontainers and default cross-builds.
Existing unrelated or dirty sibling directories are not selected or modified.

| Component | Released selection |
| --- | --- |
| Core / Cypher and GQL functions | 0.5.10 |
| Library | 0.9.3 |
| Host SDK / plugin SDK / FFI primitives | 0.11.3 |
| RocksDB index / middleware | 0.6.4 / 0.5.11 |
| AST / Cypher and GQL parsers | 0.3.5 / 0.3.6 |
| Noop and application bootstrap / application reaction | 0.2.15 / 0.3.13 |
| State store / WAL | 0.2.8 / 0.2.10 |

All these packages resolve from crates.io. The unchanged parser archives identify
source `8f0ed49802ab0f2d62ce834aafe7fe7e5861ee76`; the new family identifies
the release above. `scripts/plugin_origin.py` binds the actual locked graph to
the 17 reviewed name/version/source/checksum identities. It rejects partial
source overrides, duplicate identities, older releases, altered checksums and
unused patches rather than accepting arbitrary registry versions. This is
independent of the plugin binaries' own SDK version fields.

The published core includes the final reviewed aggregate grouping, sign/magnitude
numeric equality and hashing, compound keys, default/current fingerprints,
precision-safe Noop, lazy min/max and terminal suppression fixes. Library 0.9.3
contains the named MessagePack output writer. These claims are backed by
checksum-verified published archives and focused registry-only behavior tests,
not merely release tags or version strings.

### Build and plugin boundaries

`make prepare-build` remains a locked origin check, not a source downloader.
Trading still builds this server and real UI, prepares its npm file dependency
in order, and installs its five signed plugins before the app creates queries
and the SSE reaction. Explicit local SDK development is still available:
host/SDK/FFI/library manifest paths and versions must match the actual selected
local plugin-build workspace. Only that mode builds local unsigned plugins or
mounts a local workspace for Cross. Directory presence is not authorization.

The six platform lockfiles are inherited from the verified root release update.
They pin HTTP source 0.2.13, PostgreSQL/mock 0.2.12, PostgreSQL/scriptfile
bootstrap 0.2.15, SSE 0.3.8, log 0.2.9 and HTTP reaction 0.3.5. All use SDK crate
0.11.3 and native ABI 0.14.0. There is no older or unsigned fallback. The Darwin
SSE artifact's missing signature was repaired by the official publisher at
`32fc9052f917666af14663b6c21f6e44d32778ae` without changing its release binary
or manifest digest. The signing revision and binary source revision are distinct.
The exact issuer and `publish-plugins.yml@refs/heads/main` trust policy remain.
ABI compatibility alone does not prove arbitrary old source-event wire
compatibility; the new coherent plugin family supplies required sequence values.

The isolated YAML validation job runs its original three config-test commands
without a source-preparation step. Its reporting, failure aggregation, read-only
job boundary, action pins and stack-top gate are unchanged. Compilation uses the
same strict gh-aw 0.88.8 compiler; an ancestor's skipped job is not an executed
top-of-stack validation.

### Persistent-state upgrade: reconstruct all affected state

All affected numeric groups require reconstruction, **including ordinary integer
keys and numbers nested in lists or objects**, not only floating-point keys.
Grouping/default/current fingerprints, lazy min/max sets, query indexes and
output must be reconstructed together from available authoritative bootstrap or
retained replay. Back up state and verify the required source history first.
Clearing only output rows or hot-reusing old lazy/index state is not a migration.

Source ranks now participate in the query configuration hash, causing a one-time
old-hash mismatch and rebootstrap. That mechanism and the new named output writer
do **not** repair malformed old positional MessagePack records: Strict failures
remain visible. No real user-data deletion, automatic repair or expanded recovery
guarantee is part of this server update.

Current build/runtime evidence is recorded on #204/#202 against actual released
inputs. Earlier source-pin, ABI and raw financial/browser recordings below remain
historical; they are not relabeled as new execution. The protected `211d` source
snapshot is retained for still-unmigrated consumers and requires separate cleanup
approval. Host audit warnings, embedded-plugin audit coverage, publisher and human
assistive-technology limitations remain explicit.

## Historical September 22 temporary source selection (superseded)

The approved normal merge of #119
`739613b927b4c00e92444bebf52c9bd71a90f547` brings main
`77dcf5df807286dde22e50f93efc78ddf252ef35` above prior #204
`ba53d5044aa65e45257a7524c6c67890c5eaf128`, preserving both histories.
The user chose the existing exact
[`211d0f2a79aa2ad0f7cb841937f52013fe95ded6`](https://github.com/drasi-project/drasi-core/commit/211d0f2a79aa2ad0f7cb841937f52013fe95ded6)
source from [drasi-project/drasi-core#810](https://github.com/drasi-project/drasi-core/pull/810).
This is a **temporary source pin for current-main development**, not a released
aggregate fix or a released-only dependency closure. It authorizes no core
change, merge, publication, or retargeting of the older compatible backport.

| Component | Current selection |
| --- | --- |
| Engine / AST / Cypher | 0.5.9 / 0.3.5 / 0.3.6, only the three sibling path patches at exact `211d0f2a` |
| Library | 0.9.2, registry |
| Host SDK / plugin SDK / FFI primitives | 0.11.2, registry; native ABI 0.14.0 |
| RocksDB / Cypher and GQL functions / middleware | 0.6.3 / 0.5.9 / 0.5.10, registry |
| GQL parser | 0.3.6, registry, sharing the patched AST |
| rustls / webpki / AWS-LC / AWS-LC sys / h2 | 0.23.45 / 0.103.15 / 1.18.1 / 0.45.0 / 0.4.16, inherited root security closure |

The source pin is recorded in `.drasi-core-revision`; Cargo path lock entries
alone do not record a Git revision. Existing preparation reads that file for
Make, local builds, CI, Docker, devcontainers and cross-builds. It obtains only
an absent sibling and rejects wrong or dirty existing source without changing
it. An existing shared worktree link must not be repointed by preparation:
the one coordinated transition requires explicit owner quiescence and guarded
replacement of the link alone. Neither core worktree is modified.

Relative to the published core 0.5.9 source at
`70ca432c0f12623ab9b371b2d515180ccc80c2dd`, this pin includes the three reviewed
aggregate production paths **and two additive outbox production paths** from
merged drasi-project/drasi-core#927, plus four test-only paths. It is not a
three-file release backport. Root/selected manifests, AST/Cypher sources and
result-aware hook APIs match that published release. Registry library 0.9.2
uses `append`, not the newer trim methods. Source compatibility still requires
the mixed graph's own build and runtime evidence; the older backport's passes
are not substituted.

All six inherited plugin lockfiles stay exactly as reviewed in #119. Their
eight kinds use official merged release source
[`70ca432c0f12623ab9b371b2d515180ccc80c2dd`](https://github.com/drasi-project/drasi-core/commit/70ca432c0f12623ab9b371b2d515180ccc80c2dd):
HTTP source 0.2.12, PostgreSQL/mock sources 0.2.11, PostgreSQL/scriptfile
bootstrap 0.2.14, SSE 0.3.7, log 0.2.8, and HTTP reaction 0.3.4.
All use SDK crate 0.11.2 and native ABI 0.14.0. Prior ABI 0.13 caches are not
compatible and cannot be reused as a fallback. Plugins are not built from the
temporary engine branch, even though that workspace's unused SDK has the same
crate version as the registry SDK.

Installation retains immutable digests, binary hashes and the exact issuer
`https://token.actions.githubusercontent.com` / subject
`https://github.com/drasi-project/drasi-core/.github/workflows/publish-plugins.yml@refs/heads/main`.
[Publication run 35281678998](https://github.com/drasi-project/drasi-core/actions/runs/35281678998)
published the three platforms successfully but failed its visibility step.
It is not described as an overall successful workflow. Root's anonymous
availability, full-signature and binary checks are prior artifact evidence;
this mixed graph requires its own native/runtime checks.

**Persistence boundary:** engine-only selection does not consume
drasi-project/drasi-core#909's library codec change. The registry library still
uses compact `rmp_serde::to_vec`. No reconstruction of legacy data, record
dropping, output clearing, migration, broader recovery guarantee, or new
published-release claim follows. Existing server audit warnings, unused-core
workspace advisories, embedded-plugin coverage and human accessibility
limitations remain unwaived. Earlier validation below is labeled historical;
current evidence is recorded on #204/#202 against its actual inputs.

### Validation of the current selection

Locked metadata and the full target tree select one identity for each patched
package. Compared with the exact incoming 621-package registry lock, only the
three selected source/checksum pairs are removed; all versions, other records,
dependency arrays, resolved features and dependency-kind/target edges match.
Unrelated Windows edge rebindings from the targeted Cargo update were rejected,
not adopted. Equal-version unused SDKs still select registry mode, and all three
public local-plugin build targets reject this configuration before building.

The own UI/default server build, strict Clippy, formatting and 46 tooling tests
pass. Ordinary locked Rust tests report 809 passes and 32 existing ignores;
`make test-all` reports 840 passes and one ignored doctest. Its separate legacy
smoke reports eight passes and 28 skips, not 36 tested plugin kinds. Eight exact
core aggregate/retained-source regressions pass (753 other tests filtered out).
Cargo Audit reports zero vulnerabilities and the same 15 warnings.

All eight official plugins pass fresh isolated-HOME installation and actual
native ABI 0.14 startup, required-factory/hash/version checks and embedded UI
serving. Missing plugins and preserved ABI 0.11/0.13 artifacts fail startup
without fallback. Clean public source preparation fetches exact `211d0f2a`;
the real unprivileged devcontainer preflight also passes without changing
parent ownership. These checks do not substitute for a full devcontainer
post-create build or final-head container CI.

The own real PostgreSQL/original Flask/CDC/raw-SSE API trial creates all 11
unchanged queries and one reaction, and verifies every query remains running.
Watchlist, portfolio and order CRUD pass. Every captured authoritative summary
is a singleton: initial 2000/cost 1800/count 2, then 2050 live and after
existing-resource reuse, then 2150 offline and after reconnection. Full rows
are retained without filtering or choosing a historical row. The disposable
fixture supplies the original UI's complete order-duration fields; an earlier
incomplete payload correctly failed and is retained separately, not counted as
a passing run. This is an API/raw-wire trial, not an unchanged P1 browser gate
or persisted-data recovery claim. P1 owns its subsequent integrated browser
validation; final-head CI and any environmental limitations are recorded
separately on the PR.

## Historical September 20 main integration (superseded)

This is the user-approved, history-preserving update to draft
[#204](https://github.com/drasi-project/drasi-server/pull/204).
It merges #119 head `116cdbd5595b5adf27c2314c1d46cb8f076ad08e`
(which includes main `0d369a7d9739306b5e45376d65049ef1075a6df7`)
above the earlier prerequisite `6f888956cca131992ed7e656387f74cc3053652b`.
The explicit choice to adopt main's newer runtime and compatible signed plugins
supersedes the earlier frozen library/SDK/plugin matrix, not the query/Trading
behavior contract or the requirement for signature verification.

The earlier [engine](engine-prerequisite.md),
[security](server-security-dependencies.md) and
[plugin](../tests/plugin-pins/README.md) results remain historical evidence.
They are not relabeled as proof for the new runtime. No merge into main,
publication, native stack reordering or changes to the shared core checkout
are authorized by this update.

## Exact selected graph

| Component | Current selection | Provenance |
| --- | --- | --- |
| Server | 0.2.3 | Incoming main, including archive opt-in, RocksDB budgets and plugin-prefix handling |
| `drasi-lib` | 0.9.1, registry | Exact incoming main lock; crate source `d179d58721478fbb9d1086ba34058462bba129b1` |
| Host SDK / plugin SDK / FFI primitives | 0.11.0, registry | Exact incoming main lock; host/plugin SDK source `9ddde5b50e85904409a1eb0172d6f475a2d38964` |
| RocksDB index | 0.6.1, registry | Exact incoming main selection, including RocksDB 0.22 |
| Core / AST / Cypher | 0.5.8 / 0.3.5 / 0.3.6, sibling paths | Unchanged reviewed correction `1284e9f648634c1faa73fd897a21c2712bb0cbbe` |
| GQL parser | 0.3.6, registry | Same single patched AST identity |
| Wiremock / deadpool | 0.6.5 / 0.12.3 | Incoming main; Wiremock explicitly pinned rather than downgraded to the earlier 0.6.0 |
| h2 | 0.4.16 | Both parents' patched selection |
| rustls / webpki / AWS-LC crates | 0.23.45 / 0.103.14 / 1.18.0 + sys 0.44.0 | Retained reviewed #203 security requirements |

The 621-package lock is the incoming main lock with only the three engine/parser
registry source/checksum pairs removed and the four reviewed TLS package
versions/checksums retained. AWS-LC sys retains its required `pkg-config` build
edge. Every other incoming package record and dependency edge is unchanged.
This is a reviewed merge of dependency intent, not an unconstrained resolver
refresh or a wholesale choice of either parent's lock.

The new lock SHA-256 is
`63565b6a959f0c51a0cec916381866511be8da75f81d21dccc31ba7b2748b414`.
The three root patches and `.drasi-core-revision` are unchanged. No newer
unused library or SDK from that sibling workspace is consumed. The complete
main runtime builds against the pinned correction without a registry edit,
API shim or core backport change.

## Plugin crate version, native ABI and publisher are separate

The host's SDK **crate 0.11.0** declares native `FFI_SDK_VERSION = "0.13.0"`.
The loader compares native major/minor ABI and the target triple before init.
Old ABI 0.11 artifacts and newer ABI 0.14 artifacts are not compatible.
Matching a crate version or an OCI label alone is insufficient.

All current pins come from merged release
[`3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`](https://github.com/drasi-project/drasi-core/commit/3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a),
[drasi-project/drasi-core#789](https://github.com/drasi-project/drasi-core/pull/789).
The annotated `drasi-plugin-sdk-v0.11.1` tag points to that same commit, which
is contained in core main. These plugins use SDK **crate 0.11.1**, library
0.9.1 and core 0.5.8; their actual native ABI is **0.13.0**. The host is not
upgraded beyond main to match the plugin SDK's patch version.

| Role | Exact pinned version |
| --- | --- |
| HTTP source | 0.2.11 |
| PostgreSQL source | 0.2.10 |
| PostgreSQL / scriptfile bootstrap | 0.2.13 |
| SSE reaction | 0.3.6 |
| Mock source | 0.2.10 |
| Log reaction | 0.2.7 |
| HTTP reaction | 0.3.3 |

The six platform lockfiles contain immutable OCI digests and independent binary
SHA-256 values for macOS arm64 and Linux amd64/arm64. All 24 digests match the
successful platform publication jobs of
[Release-plz run 33420502105](https://github.com/drasi-project/drasi-core/actions/runs/33420502105):
macOS arm64 `99591664440`, Linux arm64 `99591664444`, Linux amd64
`99591664447`. The **overall run failed its package-visibility step**; it is
not represented as a green workflow. Availability is established separately by
fresh anonymous installs of all 24 binaries, not by the visibility job's status.

Each artifact has the exact verified issuer
`https://token.actions.githubusercontent.com` and subject
`https://github.com/drasi-project/drasi-core/.github/workflows/publish-plugins.yml@refs/heads/main`.
The existing runtime trust policy is unchanged; the helper's exact main-subject
check is retained. Inspection also found ABI-compatible binaries from unmerged
branches `a0901765...` and `c688c39f...`. Those are **rejected research**, not
adopted pins or mislabeled main releases. Mutable version tags had pointed to
some of those different builds, which is why digest and source verification
were required.

Anonymous installation removes registry credential environment variables.
The verified SDK path then uses `RegistryAuth::Anonymous` explicitly: it does
not read HOME/Docker credential stores, and OCI authentication/token caches are
per-client in memory. Cosign verification uses the supplied anonymous auth and
embedded public trust roots, not a global artifact cache. Fresh output
directories plus that code trace establish the documented default install
path without changing global credentials or settings.

## Startup and behavior boundaries

Supported root Cargo/Make entry points prepare the engine before Cargo resolves
the mandatory paths. `scripts/prepare-build.sh` is the shared preflight for
build/run/test/lint/setup targets and Trading: obtain only an absent sibling,
then inspect Cargo's actual SDK origins and verify the exact core pin in
registry mode. Genuine matching local SDK development keeps its separate
source mode. Failed preparation stops before compilation or installation.
For direct `cargo ...` commands, run `make prepare-core` first (or
`make prepare-build` for deliberate local SDK development).

Trading retains its existing 11 queries, joins, creation order, transformations,
CRUD, defaults and presentation. The same installer supplies Trading's five
plugins, auxiliary tests' mock/log/HTTP plus scriptfile, and Getting Started's
PostgreSQL source/bootstrap and log reaction. Getting Started now explicitly
installs its compatible locked set before starting, with signature verification
on and unpinned startup auto-install off; it cannot silently fetch newer ABI
0.14 artifacts.

`scripts/plugin_origin.py` requires the actual incoming registry host/plugin/FFI
0.11.0 and library 0.9.1 identities. Engine-only sibling paths still do not
authorize building that workspace's unused SDK plugins. Existing unknown
plugin files or conflicting pins are rejected, never overwritten or reused
as an old-ABI fallback. Genuine local SDK development remains a separately
checked source mode.

The new SSE implementation still has both configured per-operation templates
and its default query-ID-bearing result envelope. Its public event stream is
not inferred compatible from ABI alone: the unchanged P1 live harness must
verify the actual payloads and singleton authoritative summary transitions
2000/cost 1800/count 2 -> 2050 live/reload -> 2150 after reconnect. No downstream
P2-P7 adapter, fixture, clock, coverage threshold or screenshot refresh is
introduced here.

## Validation record and limits

The locked main-runtime native build, strict all-target Clippy and ordinary Rust
suite pass: **809 passed, 32 existing ignored, 38 summaries**. With the signed
test plugins installed, the full `make test-all` Rust portion reports
**840 passed, one existing doctest ignored**; the legacy broad smoke separately
reports **8 passes / 28 skips / zero failures**. The skips are not extra tested
plugin kinds. Fresh Cargo Audit reports zero vulnerabilities and the same
15 existing warnings without suppression.

All eight official plugins pass actual signed startup, native ABI, version/hash,
factory and UI checks on macOS arm64 and the new source-built Linux arm64 host.
All three platforms' pins pass signature/hash/target-metadata checks through
anonymous installation. Native Linux amd64 loading is not implied by the
artifact checks; remote Getting Started and subsequent integrated live results
have their own evidence.

The real P1 gate, exact final commit/check outcomes and any remaining limitations
are recorded on #202/#203 and #204. Older binary hashes and earlier passes are
not substituted for this new runtime. YAML-agent status is classified from the
actual new run, not automatically repeated as either the previous HTTP 400 or a
passing skipped validation. Unused legacy core-workspace advisories and
dependencies embedded in precompiled plugins remain outside the host audit.

No legacy-data reconstruction, output-only clearing, codec migration or published
fixed-release claim follows from this change. Root Cargo patches remain a
workspace consumption mechanism, not an automatic downstream published-library
override.
