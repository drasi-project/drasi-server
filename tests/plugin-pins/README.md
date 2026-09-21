# Reviewed registry plugin pins

These locks and the shared locks in
[`examples/trading/app/test/live`](../../examples/trading/app/test/live)
come from the published, merged-main release
[`3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a`](https://github.com/drasi-project/drasi-core/tree/3f043cd9e30072c1b47a29f9c5d3b11b1a356c9a).
They cover Linux amd64, Linux arm64, and macOS arm64. Each entry pins an
immutable OCI/platform manifest digest, the binary SHA-256, and the verified
GitHub Actions issuer and `publish-plugins.yml@refs/heads/main` identity.

`python3 scripts/install_plugins.py --group getting-started --server-bin PATH --plugins-dir DIRECTORY`
selects only `source/postgres:0.2.10`, `bootstrap/postgres:0.2.13`, and
`reaction/log:0.2.7`. It validates both shared lock sets before selecting those
three kinds; the other Trading/test pins are inputs to this validation, not
additional Getting Started plugins. The shared helper and pins are reused
without changing Trading startup or its queries.

The helper checks the reachable Cargo graph with `cargo metadata --locked`:
the host, plugin SDK, and FFI crates must resolve from crates.io at **0.11.0**,
and `drasi-lib` must resolve from crates.io at **0.9.1**. The published plugins
use SDK crate **0.11.1**; both it and the host SDK use native compatibility
metadata **0.13.0**. Crate versions and native ABI metadata are separate.
This registry-based setup does not require or build a sibling core checkout.

Installation uses the server's existing `plugin install --from-config --locked`
command with signature verification enabled and automatic installation off.
Conflicting existing locks/binaries, missing files, unsupported platforms, or
failed verification stop the command. Existing unrelated lock entries are
preserved. There is no latest-version, unsigned, or alternate-version fallback.
Run the origin, pin, installer, and startup-failure tests with:

```bash
python3 -m unittest discover -s tests -p 'test_plugin_*.py'
```

Signature, hash, and ABI checks do not audit dependencies embedded in the
precompiled plugins. Cargo Audit covers the server's selected Rust graph,
not those embedded dependencies, and this change does not include the later
engine aggregate correction.
