# Server finding preservation ledger

This directory preserves the Drasi Server findings recovered during two
separate September 2026 audits. Issues #179-#192 are the pre-existing
ComputationGraph Server cohort. Issues #194-#196 are additional Server findings
recovered from the later WorkGraph and recovery histories. The latter are not
presented as newly discovered ComputationGraph findings, and neither cohort is
presented as an implementation change or a claim that every historical
observation still reproduces on current `main`.

The audited baseline is Drasi Server 0.2.3 at
[`0d369a7d9739306b5e45376d65049ef1075a6df7`](https://github.com/drasi-project/drasi-server/commit/0d369a7d9739306b5e45376d65049ef1075a6df7).
The source report hash was
`683a7406e1edd6778f892bd3e849d2440ac8da3fa023aac79c6ac93b37eebbda`.
The machine-readable ledger records all 132 indexed report rows and all seven
narrative qualifications so the Server audit can be reconciled without the
original local sessions.

## ComputationGraph Server cohort

| Issue | Scope | Evidence |
|---|---|---|
| [#179](https://github.com/drasi-project/drasi-server/issues/179) | Serialize configuration saves | Code-confirmed race; deterministic harness proposed |
| [#180](https://github.com/drasi-project/drasi-server/issues/180) | Preserve provider references for environment-derived instance IDs | Code-confirmed residual persistence path |
| [#181](https://github.com/drasi-project/drasi-server/issues/181) | Repair the independent SSE CLI lockfile | Reproduced; draft fix [#186](https://github.com/drasi-project/drasi-server/pull/186) |
| [#182](https://github.com/drasi-project/drasi-server/issues/182) | Reserve instance IDs before resource startup | Code-confirmed race; runtime leak not asserted |
| [#183](https://github.com/drasi-project/drasi-server/issues/183) | Complete all instance cleanup after failure | Code-confirmed ownership gap |
| [#184](https://github.com/drasi-project/drasi-server/issues/184) | Surface schema-validator compilation failures | Existing unit test confirms false-success result |
| [#185](https://github.com/drasi-project/drasi-server/issues/185) | Serve live registry-aware OpenAPI | Code-confirmed route/cache disconnect |
| [#187](https://github.com/drasi-project/drasi-server/issues/187) | Own and await the Axum task | Code-confirmed lifecycle gap |
| [#188](https://github.com/drasi-project/drasi-server/issues/188) | Define typed Secret support boundaries | Code-confirmed unsupported local resolver path |
| [#189](https://github.com/drasi-project/drasi-server/issues/189) | Define persistence fidelity for unresolved settings | Code-confirmed semantic round-trip concern |
| [#190](https://github.com/drasi-project/drasi-server/issues/190) | Reconcile plugin load-policy entry points | Code-confirmed policy differences |
| [#191](https://github.com/drasi-project/drasi-server/issues/191) | Align preflight with effective instance selection | Code-confirmed traversal mismatch |
| [#192](https://github.com/drasi-project/drasi-server/issues/192) | Publish the five-category plugin support matrix | Code-confirmed discovery asymmetry |
The assigned umbrella is
[#193](https://github.com/drasi-project/drasi-server/issues/193). All records
above are assigned to `agentofreality`. Existing issue history and assignees
were retained.

This ledger and its portable evidence are published in assigned draft
[#197](https://github.com/drasi-project/drasi-server/pull/197), which is linked
to the umbrella without claiming to resolve it.

## WorkGraph/recovery additions

| Issue | Scope | Evidence |
|---|---|---|
| [#194](https://github.com/drasi-project/drasi-server/issues/194) | Stop same-kind inline bootstrap inheritance from leaking source-only fields | Historically reproduced; current merge remains |
| [#195](https://github.com/drasi-project/drasi-server/issues/195) | Define complete/fail-closed plugin-aware validation | Reproduced on the current baseline |
| [#196](https://github.com/drasi-project/drasi-server/issues/196) | Resolve environment references before secret-store construction | Current code-confirmed; historical bounded patch retained |

These additions are coordinated through #193 for preservation only; they do not
rewrite the provenance or scope of the earlier ComputationGraph audit.

## Portable reduced evidence

- [`repros/same-kind-inline-bootstrap.yaml`](repros/same-kind-inline-bootstrap.yaml)
  and
  [`repros/top-level-bootstrap-reference.yaml`](repros/top-level-bootstrap-reference.yaml)
  isolate the failing merge and its safe control without application-specific
  names or credentials. They require synthetic strict-example descriptors as
  described in #194.
- [`repros/validate-missing-plugin.yaml`](repros/validate-missing-plugin.yaml)
  is a direct CLI fixture for #195.
- [`repros/secret-store-bootstrap-env.yaml`](repros/secret-store-bootstrap-env.yaml)
  is a synthetic descriptor fixture for #196.
- [`patches/secret-store-bootstrap-env-6648739.patch`](patches/secret-store-bootstrap-env-6648739.patch)
  is a zero-context patch generated from the exact two-file portion of
  historical commit `6648739` relevant to #196. Inspect it before use; checking
  it requires `git apply --check --unidiff-zero`. It is evidence, not a patch
  approved for direct application.

## Standalone WorkGraph/recovery case details

The issue bodies are the canonical discussion records. The following portable
snapshots retain the complete engineering cases for the three later additions
without requiring access to the original application, session history, or
private workspace.

### #194: same-kind inline bootstrap inheritance

**Affected code and version.** Server 0.2.3 at `0d369a7`, specifically
`SourceConfig` deserialization and `merge_bootstrap_provider_with_source` in
`src/api/models/source.rs`. The function copies every source config key missing
from an inline bootstrap provider whenever the two `kind` strings match.
Top-level reference resolution in `src/factories.rs` does not perform this
merge.

**Reproduction.** Register synthetic source and bootstrap descriptors under
`strict-example`. Let the source accept `endpoint`, `credential`, `durability`,
and `webhook`, while the bootstrap DTO accepts only `endpoint` and `credential`
and denies unknown fields. Load
[`same-kind-inline-bootstrap.yaml`](repros/same-kind-inline-bootstrap.yaml).
The parsed inline provider contains `durability` and `webhook`; strict provider
creation rejects them. The
[`top-level-bootstrap-reference.yaml`](repros/top-level-bootstrap-reference.yaml)
control supplies the same shared values without copying source-only fields.

The equivalent failure was exercised historically on Server `8cd501c`: static
validation accepted the inline document, but server construction failed before
network activity because inherited `durability` was unknown to the strict
bootstrap DTO. The top-level provider/reference form started successfully under
the same host and plugin set. Current 0.2.3 retains the generic merge, and its
existing `test_bootstrap_provider_inherits_generic_kind` confirms that all
source fields are intentionally copied. A current production plugin was not
used to rerun the historical failure.

**Expected, actual, and impact.** Inline shorthand should inherit only fields
that the bootstrap contract declares compatible, keep source/provider configs
separate, or fail during validation with the same diagnostic as construction.
Instead, unrelated source-only fields reach the bootstrap DTO. A new source
option can therefore break an otherwise unchanged inline bootstrap
configuration, and inline/reference forms that appear equivalent can have
different startup outcomes.

**Possible fix.** Make inheritance descriptor-declared, pass source config
separately without materializing inherited provider fields, or require an
explicit shared-schema capability. Do not hard-code connector fields, weaken
strict DTOs, or serialize resolved secrets.

**Acceptance criteria.**

- Cover overlapping and source-only fields at deserialization, plugin-aware
  validation, and provider creation with synthetic descriptors.
- Ensure source-only fields reach a strict bootstrap DTO only through an
  explicit descriptor contract.
- Preserve supported shared-field inheritance, explicit overrides,
  different-kind inline providers, and top-level references.
- Make `validate` and `run` agree when the compatible plugin/schema is present.
- Keep provider persistence lossless for unresolved references and secrets.

### #195: incomplete plugin-aware validation exits successfully

**Affected code and version.** Server 0.2.3 at `0d369a7`.
`validate_with_plugins` records missing descriptors, schema validation skips
those descriptors, and `validate_config` includes them only in
`warning_count`. Process failure depends on `error_count`, so missing or
unloadable required plugins do not affect exit status.

**Reproduction.** Create an empty directory and run:

```sh
mkdir -p ./empty-plugins
cargo run --locked --quiet --bin drasi-server -- \
  validate \
  --config docs/investigations/repros/validate-missing-plugin.yaml \
  --plugins-dir ./empty-plugins
printf 'exit=%s\n' "$?"
```

This was rerun on the current baseline. The command reported zero loaded
plugins, warned that `source/definitely-not-installed` was absent, printed the
same source as `[OK]` under config validation, summarized one warning, and
returned status 0. No plugin binary, network, database, secret, or external
service was involved.

**Expected, actual, and impact.** CI and users need an explicit way to require
complete plugin-aware validation. Current exit 0 can mean either that all
required schemas accepted the document or that one or more component schemas
were never checked. Deployment gates can therefore accept unknown options or
fail only later during startup. This is a completeness/policy issue, not proof
of a runtime security bypass.

**Possible fix.** Add a documented strict/complete mode or make missing required
plugins errors with a compatibility transition. Preserve permissive
structure-only validation only as an explicit, machine-distinguishable outcome.
Do not make automation parse prose or weaken signature, integrity, ABI, or
target checks.

**Acceptance criteria.**

- Cover no directory, empty directory, absent required kind, unloadable
  selected file, complete plugin set, and invalid component config.
- Make strict validation fail for every incomplete case and identify each
  unvalidated component without exposing its values.
- Never print component `[OK]` when its schema was skipped in strict mode.
- Keep any permissive mode visibly and programmatically incomplete.
- Coordinate with #184 for schema-compilation failures and #190 for loader trust
  policy without combining their implementations.

### #196: secret-store bootstrap environment resolution

**Affected code and version.** Server 0.2.3 at `0d369a7`.
`create_secret_store_from_registry` passes the opaque
`SecretStoreConfig.config` JSON directly to the descriptor. The provider-backed
SDK resolver is installed only after this provider is created, so it cannot
resolve the provider's own bootstrap configuration.

**Reproduction.** Register a synthetic `test-file` secret-store descriptor that
requires a `PathBuf` and records the JSON it receives. Load
[`secret-store-bootstrap-env.yaml`](repros/secret-store-bootstrap-env.yaml)
with its environment variables unset. Current code passes the literal
`${TEST_SECRET_STORE_PATH:-./data/test-secrets.json}` and the structured nested
environment object to the descriptor. Add a `{kind: Secret, name: recursive}`
case to prove that provider self-reference fails explicitly rather than
recursing or falling back.

This is current code-confirmed; a current real secret-store plugin was not
rerun. Historical commit `6648739` implemented the bounded environment-only
resolution and unit tests. Its relevant two-file diff is preserved here, while
its unrelated path dependencies and build changes are excluded.

**Expected, actual, and impact.** Static values should pass unchanged;
environment references should resolve recursively before provider creation;
missing variables should produce path-specific diagnostics; and Secret
self-reference should be denied. Currently the descriptor must reject the raw
placeholder, use it literally, or duplicate Server interpolation behavior.

**Possible fix.** Re-evaluate the historical helper against current
`ConfigValue`/SDK behavior and add a small environment-only pre-provider
resolver. Keep original unresolved configuration for persistence, never log
resolved values, and do not initialize a fallback provider after failure.

**Acceptance criteria.**

- Cover plain strings, `${VAR}`, `${VAR:-default}`, structured environment
  objects, nested arrays/objects, missing values, and Secret self-reference.
- Pass resolved bootstrap JSON to the descriptor while retaining unresolved
  configuration for persistence.
- Preserve the normal provider-backed resolver for all later plugin
  construction.
- Use synthetic providers only; no credentials are required by the regression.

## Historical Server source inventory

| Commit | Remote state | Reusable evidence | Disposition |
|---|---|---|---|
| [`8cd501c`](https://github.com/drasi-project/drasi-server/commit/8cd501c360e04c1cdb45ff3ae3215af3a8b50f37) | `github-integration-prototype` | Runtime middleware regression proving malformed edits reach reconciliation safely | The test is application-shaped but the invariant is reusable. No current Server defect was established, so the protocol-specific patch is not copied here. |
| [`6648739`](https://github.com/drasi-project/drasi-server/commit/66487399e523da873175ccab4cd250aeb2a560e8) | `workgraph-generic-recovery` history | Secret-store environment bootstrap helper/tests; sibling-Core build pin | Only the environment-resolution hunk is preserved as a candidate for #196. Path dependencies are branch setup, not a product fix. |
| [`831e137`](https://github.com/drasi-project/drasi-server/commit/831e1378d2f7c69c313910d565b6b594696fa813) | tip of `workgraph-generic-recovery` | Consolidated native setup and explicit validation/plugin limitations | Remains remotely readable. It should be reviewed as a separate documentation update rather than copied wholesale into a defect fix. |
| [`0347dee`](https://github.com/drasi-project/drasi-server/commit/0347dee15dc0ab0e66ffcca08d83599509aae6f1) | `agentofreality-parallel-computation-graph` | Generated SSE CLI lockfile | Extracted independently in draft PR #186 for #181. Do not import the feature ancestry. |

`37ccfc7` and `8cd501c` contained byte-identical final middleware test content;
the latter is the remotely retained authoritative Server object. The historical
test used an application protocol only to prove a generic middleware invariant:
parse malformed data in place with skip semantics, type-gate the transform, and
reconcile the previously derived output. It does not establish a current Server
bug and should not be filed as one.

## Recommended issue-specific PR boundaries

Each future fix should use its issue as the unit of work. Do not combine these
into a single implementation PR:

1. #194: source/bootstrap merge contract and focused descriptor tests.
2. #195: CLI validation completeness/strict mode and exit-status tests.
3. #196: pre-provider environment resolution only; re-evaluate the historical
   helper against current `ConfigValue` conventions.
4. #179/#180/#189: separate persistence fixes because serialization,
   reference retention, and semantic fidelity have different invariants.
5. #182/#183/#187: separate ownership/lifecycle fixes, coordinated only where
   shutdown ordering requires it.
6. #184/#185/#191/#192: separate validation/OpenAPI/discovery changes so API
   compatibility decisions remain reviewable.
7. #188/#190: separate support-policy changes from implementation and security
   policy changes.

Core engine, ABI, query-language, historical custom WorkGraph component, and
Copilot application findings are not refiled in this repository. Their report
row IDs remain enumerated in the JSON ledger with an explicit ownership
disposition. Withdrawn diagnoses remain withdrawn.
