# PR #84 Review Comment Response Plan

## Overview

PR #84 has 19 review comment threads from two sources:
- **Copilot Bot** (automated reviewer): 12 comments (C1–C12)
- **Daniel Gerlag** (human reviewer, repo MEMBER): 7 comments (C13–C19)

**Progress: 13 of 19 resolved** — see [Completed Items](#completed-items) below.

---

## Remaining Items

### High Priority (bugs / correctness — Daniel's concerns first)

#### C14 — No rollback if load fails after removal (Daniel Gerlag)
- **File:** `src/plugin_orchestrator.rs:480-603`
- **Issue:** If new component creation fails after removing the old one, we can't get back to a working state.
- **Investigation findings:** This is part of a **systemic pattern** across plugin lifecycle management — every operation (load, upgrade, retire, promote) mutates state in-place with no rollback. `migrate_component()` deletes the old component before creating the new one; `upgrade_plugin()` collects partial failures but doesn't roll back successful migrations; `PluginRegistry` has no staging/commit model; and there's no per-plugin concurrency guard against racing upgrades.
- **Plan — Scoped to C14 (create-before-destroy):**
  1. In `migrate_component()`, create the new component **before** removing the old one
  2. Only stop and remove the old component after the new one is successfully created and added
  3. If creation fails, the old component remains untouched and running
  4. Note: broader lifecycle robustness (upgrade rollback, registry staging, concurrency guards) is out of scope for this PR but documented as follow-up work

#### C17 — RwLock held across await in plugin_orchestrator (Daniel Gerlag)
- **File:** `src/plugin_orchestrator.rs:534`
- **Issue:** Registry read lock held across async `create_source()`/`create_reaction()` calls, blocking writers and risking deadlock.
- **Plan:** Extract needed metadata under the lock, drop guard, then call async constructors. Same pattern as C2 but in different code.
- **Depends on:** C14 (rollback restructuring touches same code)

#### C2 — RwLock read-guard held across `.await` in instance_handlers (Copilot Bot)
- **File:** `src/api/shared/handlers/instance_handlers.rs:268`
- **Issue:** `plugin_registry.read().await` guard is held across async `create_source`/`create_reaction` calls, blocking writers.
- **Plan:** Clone the registry reference or extract needed data under the lock, then drop the guard before calling async constructors.

### Medium Priority (robustness / correctness)

#### C3 — Env-var-mutating tests not serialized (Copilot Bot)
- **File:** `src/api/mappings/core/resolver.rs:207`
- **Issue:** Tests use `set_var`/`remove_var` on process-wide env, causing potential flaky failures with parallel test execution.
- **Plan:** Add a `static Mutex` guard that env-mutating tests acquire before modifying env vars.

#### C9 — Plugin error codes defaulting to 500 (Copilot Bot)
- **File:** `src/api/shared/error.rs:228`
- **Issue:** New plugin-related error codes like `PLUGIN_NO_DIRECTORY` default to 500 when they may represent client errors.
- **Plan:** Add explicit HTTP status mappings for all plugin error codes (PLUGIN_NOT_FOUND → 404, PLUGIN_ALREADY_EXISTS → 409, etc.).

### Low Priority (polish / documentation)

#### C13 — Plugin upgrade behavior for sources with running queries (Daniel Gerlag)
- **File:** `src/plugin_orchestrator.rs:529`
- **Issue:** How does drain-then-retire work when a source has queries running against it?
- **Plan:** Verify and document the query recovery flow during plugin upgrade in code comments.

---

## Remaining Todos

| ID | Title | Depends On | Priority |
|----|-------|------------|----------|
| c14-rollback-migrate | Create-before-destroy in migrate_component | — | High |
| c17-rwlock-orchestrator | Fix RwLock across await in plugin_orchestrator | c14-rollback-migrate | High |
| c2-rwlock-instance | Fix RwLock across await in instance_handlers | — | High |
| c3-env-test-mutex | Serialize env-mutating tests | — | Medium |
| c9-error-mappings | Add plugin error code HTTP status mappings | — | Medium |
| c13-upgrade-docs | Document upgrade behavior for running queries | — | Low |

---

## Completed Items

<details>
<summary>13 items resolved (click to expand)</summary>

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
