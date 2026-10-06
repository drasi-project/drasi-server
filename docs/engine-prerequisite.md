# Compatible engine source prerequisite

> **The temporary source prerequisite is retired.** Default builds now use
> published core 0.5.10, library 0.9.3, SDK/FFI 0.11.3 and index 0.6.4.
> No sibling checkout, source pin file or preparation fetch is required.
> See [the released runtime and reconstruction warning](main-runtime-integration.md).
> The `1284e9f6` backport, later `211d0f2a` pin and all results below remain
> historical evidence, not new release execution. The protected `211d` snapshot
> must remain until every consumer migrates and separate cleanup is approved.

## Historical September 18 backport delivery (superseded)

This checkout temporarily consumes the reviewed aggregate-identity correction
for [drasi-project/drasi-core#680](https://github.com/drasi-project/drasi-core/issues/680)
from the compatible backport in
[drasi-project/drasi-core#933](https://github.com/drasi-project/drasi-core/issues/933) /
[drasi-project/drasi-core#934](https://github.com/drasi-project/drasi-core/pull/934).
Server integration is tracked by
[#202](https://github.com/drasi-project/drasi-server/issues/202), between
[#119](https://github.com/drasi-project/drasi-server/pull/119) and
[#201](https://github.com/drasi-project/drasi-server/pull/201), under
[#161](https://github.com/drasi-project/drasi-server/issues/161).

**This is an unpublished source integration, not a fixed release or blanket
security approval.** The separately approved
[#203 transport remediation](server-security-dependencies.md) clears the two
selected server advisories while preserving the Drasi/plugin matrix. Existing
warnings and the unused upstream workspace's failing audit remain visible;
neither a draft PR nor functional validation waives them. No merge, publication,
library/SDK upgrade or data migration is part of this prerequisite.

## Exact source and dependency boundary

`.drasi-core-revision` records the full pushed commit
`1284e9f648634c1faa73fd897a21c2712bb0cbbe`. It is based on released
`drasi-core-v0.5.8` commit `3d73500b1605428b441908d4eefa6375dde659b0`,
not current main. The backport applies the complete reviewed three-file runtime
delta from `e759606fa065bee0ef9e60017e263f0f85dd0e44` to
`540999d29375b509e88577eb4d24980618ef8ef8` from
[drasi-project/drasi-core#810](https://github.com/drasi-project/drasi-core/pull/810).
It preserves the released zero-argument
`ContinuousQuery::process_source_change_with_hook` callback.

| Component | Version | Selected source |
| --- | --- | --- |
| Engine `drasi-core` | 0.5.8 | `../drasi-core/core` at the recorded commit |
| `drasi-query-ast` | 0.3.5 | `../drasi-core/query-ast`, same checkout |
| `drasi-query-cypher` | 0.3.6 | `../drasi-core/query-cypher`, same checkout |
| `drasi-lib` | 0.8.9 | Unchanged crates.io package |
| Host SDK / plugin SDK / FFI primitives | 0.10.0 | Unchanged crates.io packages |
| `drasi-index-rocksdb` | 0.5.8 | Unchanged crates.io package |
| `drasi-query-gql` | 0.3.6 | Unchanged crates.io package, using the shared AST |
| Trading plugin C ABI | 0.11.0 | Unchanged signed artifacts; not the SDK crate version |

All three root `[patch.crates-io]` entries are necessary: the engine uses its
AST/Cypher path siblings, Cypher uses AST, and registry library/SDK/index/function
callers must share those same Cargo identities. A core-only override can leave
nominally incompatible copies. The compatibility workspace's unused library
0.9.0 and SDK/FFI 0.11.0 crates are **not** selected.

In the initial source-only integration commit
`ef2d9471f5a14884c3ab191a5f08f41e914d7c3d`, compared with #119's
`a2b648062a4c55e036d68b6f26bf73b4e773bcf1`, the lockfile removed only the three
patched packages' registry source/checksum fields.
All 646 package versions and dependency arrays, and all 643 other package
records were unchanged. That pre-security lock SHA-256 is
`b0b2b2a464b03888050b782eab0cf9f88413608bcb12a3c674cacefaadbf34c6`.
Resolved feature sets and dependency-kind/target edges were also compared with
an immutable baseline archive, not inferred from version counts.

The subsequently approved, separately identified #203 commit changes only
the required HTTP/TLS/test-client closure. The final lock has 623 packages and
SHA-256 `7405a70dfa40b5f3c9f007468acd00b9d63bef4d718d6315444829db19d8f6a4`;
[the complete seven-upgrade/23-removal and feature/edge delta](server-security-dependencies.md)
is documented separately. Do not describe the final combined lock as
source-only, or reuse pre-update binary results as final validation.

The earlier complete Git engine 0.5.9 candidate at
`211d0f2a79aa2ad0f7cb841937f52013fe95ded6` was rejected: it changed the public
hook to `FnOnce(&[QueryPartEvaluationContext])`, whereas registry library 0.8.9
supplies `move ||`. Build, tests and Clippy failed E0593. No caller shim or
library upgrade was applied; the failed attempt is preserved in #202's evidence.

## Local setup, containers and CI

```sh
make prepare-core
make build-release
cargo metadata --locked --format-version 1
cargo tree --locked --target all
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
make test-tooling
```

`scripts/prepare-core.sh` fetches only the recorded commit when the sibling is
absent. An existing checkout or managed-worktree symlink must resolve to its
Git root, have that exact HEAD, and be clean. Wrong revisions, dirty source,
unrelated directories and broken links fail without reset, replacement or
automatic deletion. An approved symlink must be created only at an absent
`../drasi-core`; never repoint an unrelated checkout or a project's main
working tree. `bash scripts/prepare-core.sh --check` verifies without fetching.

Devcontainer post-create opts into `--allow-sudo` only because its unprivileged
user cannot normally create a sibling under root-owned `/workspaces`.
If that sibling is absent and its parent is not writable, preparation uses
noninteractive `sudo mkdir` to reserve exactly that path and assigns only the
new directory to the invoking user. It never recursively changes `/workspaces`,
changes ownership of an existing checkout/link, or enables privileged behavior
for ordinary local/CI calls. Missing sudo authorization is an explicit error.

**Cargo path dependencies do not store a Git revision in `Cargo.lock`.**
Keep the revision file, source verification, manifest and lock together.
Package version 0.5.8 alone cannot distinguish the published baseline from
this backport. Root patches also do **not** propagate automatically to
downstream consumers of a published `drasi-server` library; those consumers
need their own reviewed dependency selection.

Existing Rust CI calls the same preparation helper while leaving the server
checkout in its normal location. The Docker builder fetches the pinned sibling
inside the image: a host sibling outside the build context is not silently
omitted. Server cross-build commands set `DRASI_CORE_WORKSPACE`; `Cross.toml`
mounts the complete sibling workspace root as well as Cross's automatic crate
mounts, so workspace-inherited manifest fields remain available. This does not
add a sibling dependency to the separate SSE CLI. Release workflow preparation
is wired for reproducible builds, but this work does not run or authorize a
release.

Trading's devcontainer and `start-demo.sh` share `scripts/prepare-trading.sh`.
Registry-SDK mode verifies the exact engine, builds the checked-out server and
real UI with locked dependencies, and installs all five pinned Trading plugins,
including SSE before the app creates its reaction. A pre-existing binary,
download of `latest`, or empty `ui/dist` is not source-build evidence. Published
images/binaries do not inherit these local patches.

The same source-workspace prerequisite installs `dev-tools/react` with its
committed npm lock and runs its existing build before `npm ci` installs the
app's file dependency. This prevents a clean checkout from invoking that
dependency's prepare script without `tsup`. It changes no package APIs,
exports, UI or lockfile resolutions. Source-free packed-consumer checks are a
separate path and must continue to disable lifecycle rebuilding from source.

## Plugins and genuine local SDK development

`scripts/plugin_origin.py` follows the actual locked Cargo resolution for the
server's SDK, host SDK, FFI and library identities. Directory existence and
commented manifest text are not selection criteria. Engine-only paths with
registry SDK crates retain registry plugins. Mixed or duplicate identities and
unsupported registry versions fail explicitly.

Trading reuses the original #201 platform locks byte-for-byte, at
`examples/trading/app/test/live/plugins-*.lock`: HTTP source 0.2.8, PostgreSQL
source 0.2.7, SSE reaction 0.3.4, and PostgreSQL/scriptfile bootstrappers 0.2.10.
The supported pin sets are Linux amd64/arm64 and macOS arm64. Preparation uses
`plugin install --from-config --locked` and independently verifies every binary
hash because the existing CLI can report per-plugin failures with a zero exit
code. Existing conflicting binaries or lock entries are not overwritten.
Server startup re-verifies signatures against OCI with `verifyPlugins: true`;
no registry-mode verification bypass is added. Python 3.11+ is needed for the
standard-library TOML/hash tooling.

For deliberately selected local SDK development, the SDK/host/FFI/library
manifests and versions must match the actual plugin-build workspace. Only then
do the existing local-plugin targets build matching plugins. That separate
development mode can use unsigned locally built binaries; it is not evidence
for this pinned registry-SDK integration. `make test-all` also selects its
existing local or registry installation path from resolved origins, not the
mere presence of the engine sibling.

The test-only registry path also uses immutable pins for mock source 0.2.7,
log reaction 0.2.5 and HTTP reaction 0.3.1 from source-manifest commit
`e05938237fd8c2c8a46bb40580c6971e53088fce`, reusing the original scriptfile
entry. The same installer and hash checks serve both groups; see
[test-plugin provenance](../tests/plugin-pins/README.md) for platform-specific
signature/load evidence and the historical native version-label limitation.

## Validation status and removal policy

Initial, pre-#203 macOS arm64 validation passed locked metadata/tree,
the server build, 775 Rust tests (32 existing ignored), strict all-target Clippy
and formatting. The five original Trading artifacts installed with trusted
signature results and exact binary hashes. An isolated server loaded them with
reported C ABI 0.11.0, exposed the SSE factory and served its embedded UI.
Fresh validation of the updated #203 graph and binary is recorded separately in
[the security dependency evidence](server-security-dependencies.md); it does not
reuse that pre-update executable. Tooling tests cover origin decisions, clean-plugin startup, locked-install
postconditions, post-create source dispatch and safe exact-revision preparation.

The P1 owner subsequently ran the **full unchanged** real
PostgreSQL/Flask/CDC/SSE/browser gate against committed security-updated source
`f9b573712fc349d5339e0021336a4deb5c6a56c7`: **PASS**, with fresh 11-query/reaction
setup, CRUD/deletes, singleton 2000/cost 1800 -> live/reload 2050 ->
offline/reconnect 2150, no manual refresh and unchanged pins/assertions.
Harness revision was `fc6671bfeb7bb0bf151023d657b8d7926a0cf084`; tested native
binary SHA-256 was
`f18c25d6b9e8cdbaa6b72e4700aee7e70cff15c30ebd0e56a6c1688eb333f358`.
Later setup-helper corrections do not alter those runtime inputs.

Actual Docker Linux arm64 source builds and ABI/UI probes, the pinned Cross
tool's locked Linux arm64 check, and the actual unprivileged Trading
devcontainer post-create route also pass. Devcontainer validation used the
declared image/features and source scripts with only network/port isolation;
the parent `/workspaces` stayed root-owned, the newly reserved sibling alone
became user-owned, and all npm/Cargo manifests and locks remained unchanged.
It verified the built release executable, real UI and five signed plugin
factories, not an arbitrary prebuilt binary. Owned validation resources were
removed afterward.

The final integrated #201 default-build run and coordinator promotion remain
separate gates. See #202 for exact latest-head CI outcomes and the unchanged
YAML agent's unsupported-model infrastructure failure; missing/skipped checks
and unrelated audit failures are not represented as passes.

The original server audit reported `RUSTSEC-2026-0258` for h2 0.3.27 and
0.4.14, and `RUSTSEC-2026-0285` for rustls 0.23.40, identically to #119.
The explicitly approved #203 update removes those selected findings without
suppression; 15 existing warnings remain. The upstream compatibility workspace
separately retains its baseline-identical h2/azure_core findings and is not
modified or declared audit-green by the server change.

Remove the three overrides and revision/setup integration only after a
compatible fixed release is explicitly selected and independently validated
with the same source-identity, library/SDK/ABI and real Trading gates. Review
the new lock graph rather than doing an unrelated resolver refresh. Do not
remove the origin-based distinction between engine-only and local SDK work.

This backport does not consume the newer library's codec correction from
[drasi-project/drasi-core#908](https://github.com/drasi-project/drasi-core/issues/908) /
[drasi-project/drasi-core#909](https://github.com/drasi-project/drasi-core/pull/909).
It does not repair malformed positional records or remove legacy
contributor-keyed output. Recovery requires separately approved **complete
reconstruction from authoritative sources**, with coordinated snapshots and
cursors; no output-only clearing, silent dropping, value guessing or real-data
migration is included.
