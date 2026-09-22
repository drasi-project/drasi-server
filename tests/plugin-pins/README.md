# Reviewed registry plugin pins

> **Current pins:** mock 0.2.11, log 0.2.8, HTTP reaction 0.3.4, with shared
> scriptfile 0.2.14, from merged main release
> `70ca432c0f12623ab9b371b2d515180ccc80c2dd`. Plugin and host SDK crate
> 0.11.2 use native ABI 0.14.0. See
> [current source/signature/availability proof](../../docs/main-runtime-integration.md).
> The older matrix and timestamps below are preserved **historical evidence**,
> not descriptions of the current lockfiles. No unmerged-branch artifact is pinned.

## Current installation and source requirements

These locks and the shared locks in
[`examples/trading/app/test/live`](../../examples/trading/app/test/live)
come from the published, merged-main release
[`70ca432c0f12623ab9b371b2d515180ccc80c2dd`](https://github.com/drasi-project/drasi-core/tree/70ca432c0f12623ab9b371b2d515180ccc80c2dd).
They cover Linux amd64, Linux arm64, and macOS arm64. Each entry pins an
immutable OCI/platform manifest digest, the binary SHA-256, and the verified
GitHub Actions issuer and `publish-plugins.yml@refs/heads/main` identity.

`python3 scripts/install_plugins.py --group getting-started --server-bin PATH --plugins-dir DIRECTORY`
selects only `source/postgres:0.2.11`, `bootstrap/postgres:0.2.14`, and
`reaction/log:0.2.8`. It validates both shared lock sets before selecting those
three kinds; the other Trading/test pins are inputs to this validation, not
additional Getting Started plugins. The shared helper and pins are reused
without changing Trading startup or its queries.

The helper checks the reachable Cargo graph with `cargo metadata --locked`:
the host, plugin SDK, and FFI crates must resolve from crates.io at **0.11.2**,
and `drasi-lib` must resolve from crates.io at **0.9.2**. The published plugins
use SDK crate **0.11.2**, core **0.5.9**, and library **0.9.2**; both the plugins
and host use native compatibility metadata **0.14.0**. Crate versions and native
ABI metadata are separate. The preceding ABI **0.13.0** pins cannot be reused
with this host.
Unlike the registry-only predecessor, this engine-prerequisite checkout also
requires the exact `211d0f2a79aa2ad0f7cb841937f52013fe95ded6` sibling source
before Cargo runs. Make entry points prepare it automatically; use
`make prepare-core` before direct Cargo commands. Preparing this temporary
engine/AST/Cypher source pin does not select its unused SDK/library, even though
their version numbers match the registry crates. The signed plugins remain the
official release artifacts, not builds from this unmerged engine branch.

Installation uses the server's existing `plugin install --from-config --locked`
command with signature verification enabled and automatic installation off.
Conflicting existing locks/binaries, missing files, unsupported platforms, or
failed verification stop the command. Existing unrelated lock entries are
preserved. There is no latest-version, unsigned, or alternate-version fallback.
Run the shared origin, pin, installer, and startup-failure tests with:

```bash
python3 -m unittest discover -s tests -p 'test_plugin_*.py'
```

`make test-tooling` additionally covers the engine preparation and Make entry
points retained by this prerequisite. Signature, hash, and ABI checks do not
audit dependencies embedded in precompiled plugins; Cargo Audit covers the
server's selected Rust graph only.

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
