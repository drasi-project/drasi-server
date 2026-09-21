# Targeted server HTTP/TLS dependency remediation

> The original #203 evidence below is historical and remains unchanged. The
> [approved main runtime integration](main-runtime-integration.md) retains its
> patched h2/rustls requirements while accepting incoming Wiremock 0.6.5 and
> main's newer Drasi runtime; it documents the current 621-package graph.

[#203](https://github.com/drasi-project/drasi-server/issues/203) authorizes this
focused exception to the otherwise source-only lockfile change in
[#202](https://github.com/drasi-project/drasi-server/issues/202). It is a
separate change from the compatible engine backport, not a general dependency
refresh or a library/SDK/plugin upgrade.

## Advisories and minimum selected versions

| Package | Before | After | Why it changes |
| --- | --- | --- | --- |
| `h2` | 0.4.14 | 0.4.16 | Minimum patched release for RUSTSEC-2026-0258 |
| `rustls` | 0.23.40 | 0.23.45 | Minimum patched release for RUSTSEC-2026-0285 |
| `rustls-webpki` | 0.103.13 | 0.103.14 | Required minimum of rustls 0.23.45 |
| `aws-lc-rs` | 1.17.0 | 1.18.0 | Required minimum of rustls/webpki |
| `aws-lc-sys` | 0.41.0 | 0.44.0 | Required minimum of aws-lc-rs 1.18.0 |
| `wiremock` (test-only) | 0.5.22 | 0.6.0 | Uses existing Hyper 1 instead of the obsolete Hyper 0.14 / h2 0.3 path |
| `deadpool` (test-only) | 0.9.5 | 0.10.0 | Required minimum of wiremock 0.6.0 |

The obsolete h2 **0.3.27** instance is removed, not hidden or overridden across
an incompatible semver boundary. It was reachable only through dev dependency
wiremock 0.5.22 -> hyper 0.14.32. Only h2 0.4.16 remains in the resolved graph,
including runtime and test consumers.

[RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258.html)
concerns unbounded empty HTTP/2 DATA frames.
[RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html)
concerns acceptance of TLS 1.3 handshake messages across encryption-level
boundaries. The TLS handshake transcript remains authenticated; this is not
described as an authentication bypass.

Direct rustls and wiremock requirements are pinned to the selected versions.
Targeted `cargo update -p ... --precise ...` operations selected the lockfile;
an unconstrained workspace update was not used. Initially selected newer
compatible crypto patches were narrowed to the required minima above.

## Complete graph delta

Against source-integration commit
`ef2d9471f5a14884c3ab191a5f08f41e914d7c3d`, the lock contains **623 packages**
instead of 646: seven package replacements and 23 removals. The removed-only
packages were all outside the server/xtask runtime-and-build reachable graph:

```text
async-channel 1.9.0       base64 0.13.1          concurrent-queue 2.5.0
event-listener 2.5.3      fastrand 1.9.0         futures-lite 1.13.0
getrandom 0.1.16          h2 0.3.27              http 0.2.12
http-body 0.4.6           http-types 2.12.0      hyper 0.14.32
infer 0.2.3              instant 0.1.13         parking 2.2.1
rand 0.7.3               rand_chacha 0.2.2      rand_core 0.5.1
rand_hc 0.2.0            retain_mut 0.1.9       serde_qs 0.8.5
waker-fn 1.2.0           wasi 0.9.0+wasi-snapshot-preview1
```

All 616 retained package version/source/checksum records are unchanged.
Dependency identifiers were normalized before comparison: removing a duplicate
version can remove a textual version qualifier without changing the referenced
package. Apart from references to the seven replacements, the only retained
lock edge removed is `url 2.5.8 -> serde_derive`, because the removed
`http-types` path enabled URL's `serde` feature.

Three retained feature sets shrink for the same test-only reason: `url` loses
`serde`, `socket2 0.5.10` loses `all` supplied by Hyper 0.14, and `futures-io`
loses its redundant `default` feature while retaining `std`. No socket2 or
windows-sys package/edge was rebound to a different version.

The final lock SHA-256 is
`7405a70dfa40b5f3c9f007468acd00b9d63bef4d718d6315444829db19d8f6a4`.
The three local engine/AST/Cypher identities and revision
`1284e9f648634c1faa73fd897a21c2712bb0cbbe` are unchanged, as are registry
`drasi-lib 0.8.9`, SDK/host SDK/FFI 0.10.0, index 0.5.8 and GQL 0.3.6.
All original Trading and approved auxiliary plugin pin files remain
byte-identical to the source-integration commit.

## Tests and remaining limits

The ordinary Rust suite builds with wiremock 0.6.0 without production API
changes. Enabling the ten existing HTTP-reaction E2E scenarios exposed two
pre-existing test assumptions: they looked for returned row fields at the
top level instead of the pinned HTTP reaction's `after` envelope. The retained
pre-update executable, with wiremock 0.5.22 and identical plugin pins, reproduced
the same two failures.

The tests now strictly validate query identity, ADD/UPDATE operation and an
object-shaped `after` row before applying every original field/type/value
assertion. Four additional controls reject missing/malformed rows and wrong
query identity and verify unchanged row contents. No reaction payload,
scenario, query or financial expectation is changed. All **14** targeted
cases pass, including the original ten real plugin/Wiremock scenarios.

Local macOS arm64 checks on the updated lock pass build, strict all-target
Clippy, formatting and 35 setup-policy tests. The actual pinned-registry
`make test-all` path passes **810 Rust tests**, with one existing doctest
ignored. Its existing broad plugin smoke script records **8 passes and 28
skips**, not 36 successful plugin cases. Separate isolated runs of the updated
server loaded the five Trading and four test plugins with verified signatures,
expected versions/hashes, ABI 0.11.0, required factories and the embedded UI.

Fresh Cargo Audit 0.22.2 against RustSec database commit
`2b34578f89884736e0fcbd42f7ba8d6b10b4a0ce` reports **zero vulnerabilities** and
exits successfully without new ignores or disabled checks. **15 existing
warnings remain:** ten unmaintained-package warnings, four unsoundness
warnings, and yanked chacha20 0.10.0. The unsoundness warnings cover anyhow
1.0.102, im 15.1.0, scc 2.4.0 and sized-chunks 0.6.5. These warnings are not
silently treated as resolved.

The P1 owner also ran the full unchanged real PostgreSQL/Flask/CDC/SSE/browser
gate on the final security-updated binary: **PASS**, one scenario in 13.4s,
all 11 queries/reaction, CRUD/deletes, singleton 2000/cost 1800 -> live/reload
2050 -> offline/reconnect 2150, with no manual refresh or assertion changes.
The tested native binary SHA-256 is
`f18c25d6b9e8cdbaa6b72e4700aee7e70cff15c30ebd0e56a6c1688eb333f358`,
from committed server source `f9b573712fc349d5339e0021336a4deb5c6a56c7`.
Later setup-helper corrections do not change its runtime inputs. This is not
the pre-update `d659...` executable.

Actual source-built Linux arm64 Docker and devcontainer binaries also pass
signed-plugin ABI/hash/factory/UI probes. Cross's real locked Linux arm64
check passes with the complete sibling workspace mounted. Exact-head remote
outcomes and the unchanged YAML agent's unsupported-model infrastructure failure
are tracked in #202/#203; they are not replaced by local evidence or called
passing skips. The final integrated #201 CI remains a separate gate.

The unused legacy workspace findings in
[drasi-project/drasi-core#933](https://github.com/drasi-project/drasi-core/issues/933) /
[drasi-project/drasi-core#934](https://github.com/drasi-project/drasi-core/pull/934)
remain separate and are not waived or modified here. Server Cargo Audit also
does **not** assess dependencies embedded in precompiled plugins. This change
does not establish blanket security approval, authorize a release/merge, or
repair legacy retained data.
