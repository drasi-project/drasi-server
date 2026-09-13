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
