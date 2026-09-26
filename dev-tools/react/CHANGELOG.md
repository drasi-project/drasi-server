# Changelog

All notable changes to `@drasi/react` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- Removed package provisioning and deployment definitions (`queries`,
  `QueryDefinition`, `ReactionDefinition`, bind host/port and implicit defaults).
  Supply existing resource references instead; no management mode replaces them.
- Hook/status errors are `DrasiError` objects rather than strings. Render
  `.message`, inspect `.code`. Missing definition reads throw rather than
  returning `null`. Existing-resource connections never compare desired query text.
