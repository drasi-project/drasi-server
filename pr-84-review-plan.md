# PR #84 Review Comment Response Plan

## Overview

PR #84 has 19 review comment threads from two sources:
- **Copilot Bot** (automated reviewer): 12 comments (C1–C12)
- **Daniel Gerlag** (human reviewer, repo MEMBER): 7 comments (C13–C19)

**Progress: 17 of 19 resolved** — see [Completed Items](#completed-items) below.

---

## Remaining Items

### Medium Priority (robustness / correctness)

#### C3 — Env-var-mutating tests not serialized (Copilot Bot)
- **File:** `src/api/mappings/core/resolver.rs:207`
- **Issue:** Tests use `set_var`/`remove_var` on process-wide env, causing potential flaky failures with parallel test execution.
- **Plan:** Add a `static Mutex` guard that env-mutating tests acquire before modifying env vars.

#### C9 — Plugin error codes defaulting to 500 (Copilot Bot)
- **File:** `src/api/shared/error.rs:228`
- **Issue:** New plugin-related error codes like `PLUGIN_NO_DIRECTORY` default to 500 when they may represent client errors.
- **Plan:** Add explicit HTTP status mappings for all plugin error codes (PLUGIN_NOT_FOUND → 404, PLUGIN_ALREADY_EXISTS → 409, etc.).

---

## Remaining Todos

| ID | Title | Depends On | Priority |
|----|-------|------------|----------|
| c3-env-test-mutex | Serialize env-mutating tests | — | Medium |
| c9-error-mappings | Add plugin error code HTTP status mappings | — | Medium |

---

## Completed Items

<details>
<summary>17 items resolved (click to expand)</summary>

### C13 — Plugin upgrade behavior for sources with running queries (Daniel Gerlag) ✅
- **Original file:** `src/plugin_orchestrator.rs:529` (no longer exists)
- **Resolution:** Resolved by removal, not by patch. The `migrate_component()` / `upgrade_plugin()` drain-then-retire path was incomplete and contained the unresolved query-recovery question Daniel raised. Commit `2d03ef2` strips the entire half-implemented upgrade/migration surface (orchestrator methods, REST endpoints, UI panels, related tests). Live plugin replacement now requires a server restart, as documented in `README.md` and `CLAUDE.md`. A correct redesign — including defined query-recovery semantics — will land in a follow-up PR.

### C14 — No rollback if load fails after removal (Daniel Gerlag) ✅
- **Original file:** `src/plugin_orchestrator.rs:480-603` (no longer exists)
- **Resolution:** Resolved by removal, not by patch. Rather than retrofit create-before-destroy onto a partial design, commit `2d03ef2` removes the entire upgrade/migration surface (`migrate_component`, `upgrade_plugin`, `retire_plugin`, `promote_plugin`, `library_generation` tracking, the corresponding REST endpoints, and the UI controls). The systemic concerns Daniel identified (no rollback, no registry staging, no per-plugin concurrency guards) are tracked for the follow-up redesign that will reintroduce live replacement with atomic semantics.

### C17 — RwLock held across await in plugin_orchestrator (Daniel Gerlag) ✅
- **File:** `src/plugin_orchestrator.rs:534`
- **Fix:** Introduced `create_source_locked()` and `create_reaction_locked()` factory functions that acquire the `RwLock<PluginRegistry>`, clone Arc descriptors and extract metadata, drop the lock, then perform async component creation. Updated `migrate_component()` to use these `_locked` variants, eliminating the read-guard-across-await anti-pattern.

### C2 — RwLock read-guard held across `.await` in instance_handlers (Copilot Bot) ✅
- **File:** `src/api/shared/handlers/instance_handlers.rs:268`
- **Fix:** Replaced all `plugin_registry.read().await` + `create_source`/`create_reaction` patterns with `create_source_locked`/`create_reaction_locked` across all handler files: `instance_handlers.rs` (clone handler), `source_handlers.rs` (create/upsert), `reaction_handlers.rs` (create/upsert), `solution_handlers.rs` (deploy), `solutions.rs` (deploy_solution), `server.rs` (startup loop), and `plugin_orchestrator.rs` (migrate_component). The RwLock is now never held across an `.await` point in any code path.

### C1 — `persist_index` accepted but not applied (Copilot Bot) ✅
- **File:** `src/api/shared/handlers/instance_handlers.rs:95`
- **Fix:** Added RocksDB persistent indexing support for dynamically created instances, using the same `./data/{safe_id}/index` path convention as config-based instances.

### C5 — `NodeJS.Timeout` typing in browser code (Copilot Bot) ✅
- **File:** `examples/trading/app/src/hooks/useRowAnimation.ts:68`
- **Fix:** Replaced `NodeJS.Timeout` with `ReturnType<typeof setTimeout>` in useRowAnimation.ts and client.ts.

### C6 — Excessive console.log in SSE parsing (Copilot Bot) ✅
- **File:** `examples/trading/app/src/services/grpc/SSEClient.ts:161`
- **Fix:** All console.log gated behind `process.env.NODE_ENV === 'development'`.

### C7 — No `response.ok` check in TradingApi (Copilot Bot) ✅
- **File:** `examples/trading/app/src/services/TradingApi.ts:90`
- **Fix:** Added `parseResponse()` helper with `response.ok` guard and structured error handling.

### C8 — Brittle message suffix parsing for Added/Removed (Copilot Bot + Daniel) ✅
- **File:** `src/api/models/observability.rs:124`
- **Fix:** drasi-core now has structural `Added`/`Removed` variants on `ComponentStatus`. Replaced brittle `msg.ends_with(" added")` parsing with direct enum mapping. (commit 71cd3d7)

### C10 — `component_links` hardcodes `/api/v1/` (Copilot Bot) ✅
- **File:** `src/api/shared/handlers/mod.rs:95`
- **Fix:** Introduced `ApiPrefix` newtype injected via Axum `Extension` layer. Shared handlers extract the prefix instead of hardcoding. (commit 0df7a1f)

### C11 — Comment says "sources" but endpoint is /reactions (Copilot Bot) ✅
- **File:** `examples/trading/web_api_reaction.http:36`
- **Fix:** Updated all comments from "source" to "reaction".

### C12 — Step numbering duplicated in SKILL.md (Copilot Bot) ✅
- **Fix:** Already committed as d362d3a.

### C15 — Plugin mutation endpoints missing read-only checks (Daniel Gerlag) ✅
- **File:** `src/api/v1/plugin_handlers.rs`
- **Fix:** Added `Extension(read_only): Extension<Arc<bool>>` guard to all 5 plugin mutation handlers (`load_plugin`, `install_plugin`, `upgrade_plugin`, `retire_plugin`, `promote_plugin`). Returns 403 with `CONFIG_READ_ONLY` error code. Updated OpenAPI annotations.

### C16 — Solution handlers bypass read-only mode (Daniel Gerlag) ✅
- **File:** `src/api/v1/handlers/solution_handlers.rs`
- **Fix:** Added read-only guard to `create_solution_template` and `deploy_solution`. Consistent message format with all other mutation handlers. Updated OpenAPI annotations.

### C19 — Hot-reload watcher task not cancelled on shutdown (Daniel Gerlag) ✅
- **File:** `src/server.rs:281`
- **Fix:** Store watcher `JoinHandle` in `DrasiServer`, abort and await on shutdown before stopping instances. (commit 71cd3d7)

### C4 — Makefile test log message misleading (Copilot Bot) ✅
- **File:** `Makefile:212`
- **Fix:** Updated log message from "Running all cargo tests" to "Running unit and integration tests (including ignored/E2E)" for accuracy.

### C18 — No synchronization between PluginOperations and PluginOrchestrator (Daniel Gerlag) ✅
- **File:** `src/plugin_operations.rs:46`
- **Fix:** `PluginOperations` is composed inside `PluginOrchestrator` (as `plugin_ops: Option<PluginOperations>`) with a `dir_mutex: Mutex<()>` on the orchestrator serializing all directory-mutating operations (`install_and_load`, `load_plugin_locked`, `upgrade_plugin`). All API handlers route through these locked methods. Read-only operations bypass the mutex appropriately.

</details>
