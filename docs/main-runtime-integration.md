# Main runtime integration for the engine prerequisite

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
