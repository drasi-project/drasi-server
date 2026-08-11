# Changelog

All notable changes to `@drasi/react` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Extracted the reusable React building blocks into a standalone, publishable
  package (`@drasi/react`) under `dev-tools/react`.
- `tsup`-based build emitting ESM, CommonJS, and TypeScript declarations.
- Source reorganized into `client/` (framework-agnostic core), `react/`
  (provider + hooks), and `components/` (ready-made UI) with barrel exports.

## [0.1.0]

### Added
- Initial release: `DrasiProvider`, `useDrasiQuery`, `useDrasiConnectionStatus`,
  `useDrasiServerUiUrl`, `useDrasiQueryDefinition`, `QueryTable`,
  `CodeViewerDialog`, `useRowAnimation`, and the low-level `DrasiClient` /
  `DrasiSSEClient` classes.
