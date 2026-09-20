# Test-only registry plugin pins

> **Current pins:** mock 0.2.10, log 0.2.7, HTTP reaction 0.3.3, with shared
> scriptfile 0.2.13, from merged main release
> `3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`. Plugin SDK crate 0.11.1 and
> host SDK crate 0.11.0 both use verified native ABI 0.13.0. See
> [current source/signature/availability proof](../../docs/main-runtime-integration.md).
> The older matrix and timestamps below are preserved **historical evidence**,
> not descriptions of the current lockfiles. No unmerged-branch artifact is pinned.

## Historical September 18 locks and evidence (superseded)

At predecessor `6f888956`, these locks added only the test kinds missing from the then-unchanged Trading runtime
locks: `source/mock` **0.2.7**, `reaction/log` **0.2.5**, and `reaction/http`
**0.3.1**. `scripts/install_plugins.py --group test` reuses the existing
`bootstrap/scriptfile` **0.2.10** entry from
`examples/trading/app/test/live/plugins-*.lock`; it does not duplicate or change
any of the five Trading pins.

The full immutable source-manifest snapshot is
[`e05938237fd8c2c8a46bb40580c6971e53088fce`](https://github.com/drasi-project/drasi-core/tree/e05938237fd8c2c8a46bb40580c6971e53088fce).
The three plugin manifests at that snapshot identify the versions above. Their
OCI manifests and metadata declare SDK crate **0.10.0**, core **0.5.7** and
library **0.8.9**. Each checked-in entry records the immutable OCI/platform
manifest digest, binary SHA-256, and verified GitHub Actions issuer/subject.
Mutable version tags are not used by installation.

| Platform | OCI/metadata digest, binary hash and trusted signature | Actual native load |
| --- | --- | --- |
| macOS arm64 | Verified for all four test plugins | Verified ABI 0.11.0, versions, hashes and required factories |
| Linux amd64 | Verified for all four test plugins | Not yet claimed; requires the matching Linux server |
| Linux arm64 | Verified for all four test plugins | Verified with the source-built Linux image: ABI 0.11.0, versions, hashes and factories |

The macOS binaries also report Git commit `e059382`, independently resolved to
the full snapshot above, and these build timestamps: mock
`2026-07-08T21:41:08Z`, log `2026-07-08T21:26:26Z`, HTTP
`2026-07-08T21:27:17Z`. The actual native ABI metadata is **0.11.0**, independently
versioned from SDK crate 0.10.0.

**Historical metadata limitation:** these three source files pass
`env!("CARGO_PKG_VERSION")` for both `core_version` and `lib_version` in their
`export_plugin!` calls. Their raw native core/library labels therefore repeat
the plugin version (0.2.7, 0.2.5 or 0.3.1), not the dependency versions declared
in the OCI metadata. Those raw values are recorded, not relabeled or treated as
crate provenance. No plugin or SDK code is changed to repair this pre-existing
reporting limitation.

`make download-test-plugins` uses the same locked installer and independent
hash postconditions as Trading. Conflicting existing pins/binaries, missing
files, unsupported platforms, or failed installation stop the command. There
is no latest-version, unsigned, or alternate-version fallback. Focused tests
also reject wrong actual load ABI, version, file hash, status, missing factories
and duplicate plugin identities.

Signature/integrity/ABI validation is not a security audit of dependencies
embedded in these precompiled plugins. The server's Cargo Audit covers its own
selected Rust graph only. See [the engine prerequisite](../../docs/engine-prerequisite.md)
and [#203](https://github.com/drasi-project/drasi-server/issues/203) for the
separate source integration and targeted server security updates.
