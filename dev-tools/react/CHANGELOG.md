# Changelog

All notable changes to `@drasi/react` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Preserve explicit SSE endpoint path/query trailing slashes, including signed
  or opaque query values, instead of applying server-base trimming to them.
  Provider identity uses the same separation: equivalent server bases stay
  stable, while meaningful endpoint changes replace the connection. Existing
  URL safety validation, authentication and read-only ownership are unchanged.

### Current development-source integration
- Normally integrate the approved exact `211d0f2a` engine 0.5.9 / registry
  library 0.9.2 / SDK 0.11.2 / index 0.6.3 / signed SSE 0.3.7 / native ABI
  0.14 selection while retaining all P3 contracts and endpoint-identity fixes.
  Only engine/AST/Cypher are path-selected; unused sibling SDKs are not consumed.
- Keep earlier `1284e9f` / library 0.9.1 / ABI 0.13 proof historical and require
  own rebuilt-runtime/packed/browser/live checks. The temporary engine pin is
  not a released fix, library-codec adoption or stored-data repair guarantee.

### Historical ABI 0.13 parent integration
- Normally integrate the approved server 0.2.3 / registry library 0.9.1 /
  host SDK 0.11.0 / signed SSE 0.3.6 (plugin SDK 0.11.1, native ABI 0.13)
  runtime while retaining the reviewed engine correction and all P3 contracts.
- Keep original runtime fixtures and P1/P2/P3 schema-2 measurements historical.
  Current accounting includes both shipped declaration formats, all package
  chunks/CSS and recursive Trading assets without changing coverage floors or
  the 2% growth policy.
- Document that new archive/memory-budget controls are server/instance settings:
  the query read DTO is unchanged and `storageBackend` remains opaque JSON.
  No P4-P7 product features or query/resource-creation defaults are introduced.

### Added
- Real `@drasi/react/client`, `/react` and `/components` ESM/CommonJS exports
  with `.d.ts`/`.d.cts` declarations. The client runtime/type graph is
  React-independent; hooks do not import composed components or CSS. Root and
  explicit `styles.css` imports remain supported.
- Complete guarded v1 read DTOs, validated object-row boundaries, scoped
  component links and versioned unmodified real-server contract fixtures.
- Shared request credentials, copied headers and asynchronous per-request auth
  providers. Custom stream factories receive headers/credentials/cancellation;
  unsupported native EventSource auth fails explicitly when opening a stream.
- Named generic/concrete client, connection, hook-result, column/action/sort and
  error types; clean packed positive/negative type, client-only and SSR tests.
- Material configuration lifecycle: equivalent inline data props preserve the
  connection; changed references/auth/policy/callable identities dispose old
  work and suppress stale results. Retry callbacks are bound to their scope.
- Executable README snippets, instance/auth/import/SSR/reconfiguration/migration
  and compatibility documentation shipped in the tarball.
- Explicit `instanceId`, `queryIds` and `ReactionReference` connect-only contract;
  all validation/snapshot/definition reads use the selected instance.
- `DrasiError` with stable codes, safe messages, identity and retryability,
  preserved through hooks/context; REST classification of opaque SSE failures.
- Bounded opening, REST and snapshot retries; permanent failures stop work.
- Controlled `DrasiClientProvider` for an app-owned lifecycle without a second
  connection. Trading now owns idempotent setup, conflicts and cancellation.
- Extracted the reusable React building blocks into a private, unpublished
  package (`@drasi/react`) under `dev-tools/react`.
- `tsup`-based build emitting ESM, CommonJS, and TypeScript declarations.
- Source reorganized into `client/` (framework-agnostic core), `react/`
  (provider + hooks), and `components/` (ready-made UI) with barrel exports.
- Initial release: `DrasiProvider`, `useDrasiQuery`, `useDrasiConnectionStatus`,
  `useDrasiServerUiUrl`, `useDrasiQueryDefinition`, `QueryTable`,
  `CodeViewerDialog`, `useRowAnimation`, and the low-level `DrasiClient` /
  `DrasiSSEClient` classes.
- Package-owned namespaced CSS, lifecycle-safe SSE reconnection,
  snapshot/delta buffering (not an atomic handoff), and package/consumer CI.

### Changed
- Raw public values are validated `unknown`/`ResultRow`, not ambient `any`.
  Consumers narrow fields in transforms; a generic alone is not runtime schema
  validation. Trading owns its distinct query-creation type and typed transforms.
- Read contracts reject incomplete/malformed full-view DTOs and snapshots,
  unsupported enum values and cross-instance links. REST redirects are not
  followed. Timeouts bound even injected callbacks that do not settle on abort.
- React/React DOM peers are optional to install for client-only consumers but
  required for React/component/root execution; the verified peer version is
  18.3.1. Node 22/24 tooling is exercised; no React 19/future-major claim.
- Artifact measurement counts all entrypoints and shared runtime/declaration
  chunks rather than only the root barrel; the inherited 2% growth policy and
  coverage floors remain in force.
- Removed package provisioning and deployment definitions (`queries`,
  `QueryDefinition`, `ReactionDefinition`, bind host/port and implicit defaults).
  Supply existing resource references instead; no management mode replaces them.
- Hook/status errors are `DrasiError` objects rather than strings. Render
  `.message`, inspect `.code`. Missing definition reads throw rather than
  returning `null`. Existing-resource connections never compare desired query text.

### Deferred
- Issue #163 Part B: stable row identity, canonical deltas/explicit adapters,
  key-changing updates, snapshot/live consistency and the complete
  reconnect/stale/error state contract. Legacy keys, `_deleted`, unidentified
  routing and buffering behavior are retained, not declared complete.
- Composition/accessibility/theming (#164), standalone examples (#165), package
  publication and repository transfer remain separate work.
