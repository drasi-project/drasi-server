# Reviewed registry plugin pins

> **Current pins:** mock 0.2.12, log 0.2.9, HTTP reaction 0.3.5, with shared
> scriptfile 0.2.15, from merged main release
> `22125bf1d66062533b832a166fe4a51079a23d6e`. Plugin and host SDK crate
> 0.11.3 use native ABI 0.14.0. See
> [current source/signature/availability proof](../../docs/main-runtime-integration.md).
> The older matrix and timestamps below are preserved **historical evidence**,
> not descriptions of the current lockfiles. No unmerged-branch artifact is pinned.

## Current installation and source requirements

These locks and the shared locks in
[`examples/trading/app/test/live`](../../examples/trading/app/test/live)
come from the published, merged-main release
[`22125bf1d66062533b832a166fe4a51079a23d6e`](https://github.com/drasi-project/drasi-core/tree/22125bf1d66062533b832a166fe4a51079a23d6e).
They cover Linux amd64, Linux arm64, and macOS arm64. Each entry pins an
immutable OCI/platform manifest digest, the binary SHA-256, and the verified
GitHub Actions issuer and `publish-plugins.yml@refs/heads/main` identity.

`python3 scripts/install_plugins.py --group getting-started --server-bin PATH --plugins-dir DIRECTORY`
selects only `source/postgres:0.2.12`, `bootstrap/postgres:0.2.15`, and
`reaction/log:0.2.9`. It validates both shared lock sets before selecting those
three kinds; the other Trading/test pins are inputs to this validation, not
additional Getting Started plugins. The shared helper and pins are reused
without changing Trading startup or its queries.

The helper checks the reachable Cargo graph with `cargo metadata --locked`:
the host, plugin SDK, and FFI crates must resolve from crates.io at **0.11.3**,
and `drasi-lib` must resolve from crates.io at **0.9.3**. The published plugins
use SDK crate **0.11.3**, core **0.5.10**, and library **0.9.3**; both the plugins
and host use native compatibility metadata **0.14.0**. Crate versions and native
ABI metadata are separate. An unchanged ABI number does not establish wire
compatibility for older source plugins: source-event sequences are now required
numeric values. All eight kinds use the new release; there is no older SSE or
source-plugin fallback.
This registry-based setup does not require or build a sibling core checkout.

The Darwin SSE **0.3.8** artifact initially lacked a signature. Its signature
was repaired by the official publisher at
[`32fc9052f917666af14663b6c21f6e44d32778ae`](https://github.com/drasi-project/drasi-core/tree/32fc9052f917666af14663b6c21f6e44d32778ae),
without changing the original manifest or binary digest. The trusted workflow
identity is unchanged; the repair's signing revision is distinct from the
binary's original release revision.

Installation uses the server's existing `plugin install --from-config --locked`
command with signature verification enabled and automatic installation off.
Conflicting existing locks/binaries, missing files, unsupported platforms, or
failed verification stop the command. Existing unrelated lock entries are
preserved. There is no latest-version, unsigned, or alternate-version fallback.
Run the shared origin, pin, installer, and startup-failure tests with:

```bash
python3 -m unittest discover -s tests -p 'test_plugin_*.py'
```

`make test-tooling` additionally covers no-sibling registry preparation, strict
released package checksums and explicit local SDK selection through Make.
Signature, hash, and ABI checks do not audit dependencies embedded in the
precompiled plugins. Cargo Audit covers the server's selected Rust graph,
not those embedded dependencies. The selected registry release includes the
aggregate-grouping corrections and named output writer; see
[persistent-query upgrade guidance](../../README.md#upgrading-persistent-query-state)
before reusing existing state.

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
