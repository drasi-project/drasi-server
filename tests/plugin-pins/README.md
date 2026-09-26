# Reviewed registry plugin pins

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
Run the origin, pin, installer, and startup-failure tests with:

```bash
python3 -m unittest discover -s tests -p 'test_plugin_*.py'
```

Signature, hash, and ABI checks do not audit dependencies embedded in the
precompiled plugins. Cargo Audit covers the server's selected Rust graph,
not those embedded dependencies. The selected registry release includes the
aggregate-grouping corrections and named output writer; see
[persistent-query upgrade guidance](../../README.md#upgrading-persistent-query-state)
before reusing existing state.
