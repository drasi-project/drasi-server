# Rolling Plugin Upgrade POC — Implementation Report

**Date:** 2026-04-29  
**Repository:** drasi-project/drasi-server  
**Design Document:** `drasi-server/dual-load-rolling-plugin-upgrade.md`

---

## Executive Summary

This POC implements zero-downtime rolling plugin upgrades for Drasi Server. It enables operators to upgrade a plugin binary (source or reaction) while the server continues running, migrating dependent components one-at-a-time with automatic rollback on failure.

The implementation compiles cleanly, passes all 306 library tests (15 new + 291 existing), and is clippy-clean for the upgrade module.

---

## What Was Built

### Module Structure

```
src/upgrade/
├── mod.rs          — Module root with re-exports
├── error.rs        — UpgradeError enum (11 variants, thiserror)
├── plan.rs         — State machine: UpgradePlan, UpgradeTarget, status types
├── validation.rs   — ABI compatibility checks (SDK version, target triple)
└── engine.rs       — Core orchestration engine (plan/execute/rollback/cancel)

src/api/v1/
└── upgrade_handlers.rs  — REST API handlers + route builder
```

### REST API Endpoints

All mounted at `/api/v1/plugins/upgrades`:

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/` | Plan a new upgrade (validates ABI, discovers dependents) |
| `GET` | `/` | List all upgrade plans |
| `GET` | `/:plan_id` | Get specific plan details |
| `POST` | `/:plan_id/execute` | Execute a planned upgrade (rolling) |
| `POST` | `/:plan_id/rollback` | Rollback a completed/failed upgrade |
| `DELETE` | `/:plan_id` | Cancel a planned (not yet executing) upgrade |

### State Machine

```
Planned → InProgress → Complete
    ↓         ↓            ↓
 (cancel)  (failure)   (rollback)
              ↓            ↓
         RollingBack   RollingBack
              ↓            ↓
          RolledBack    RolledBack
```

Per-component states: `Pending → Upgrading → Upgraded` (or `→ Failed`, `→ RolledBack`)

---

## Design Decisions & Trade-offs

### 1. No Dual-Load in Registry

**Problem:** The host-sdk `PluginRegistry` is keyed by kind (e.g., `"postgres"`) and replaces entries on registration. It does not support holding two versions simultaneously.

**Decision:** The POC loads the new plugin through the normal orchestrator path. The new descriptors replace the old ones in the registry. The "dual-load" happens implicitly — the old shared library remains loaded in memory (host-sdk intentionally leaks via `mem::forget`, no `dlclose`) so existing runtime instances continue working until replaced.

**Implication:** True rollback (re-creating components with the *old* factory) is not possible without a `retiring` map. The POC's rollback is degraded to stop/restart.

### 2. Component Discovery via Snapshot

**Problem:** Need to find all components using a specific plugin kind across all instances.

**Decision:** Uses `DrasiLib::snapshot_configuration()` which returns `SourceSnapshot.source_type` and `ReactionSnapshot.reaction_type` for kind matching. This avoids depending on internal component graph metadata.

### 3. Rolling Migration via update_source/update_reaction

**Problem:** Need to swap a running component's runtime without stopping the entire pipeline.

**Decision:** Uses existing `DrasiLib::update_source(id, new_source)` and `update_reaction(id, new_reaction)` which atomically replace the runtime instance. The new instance is created from the (now-updated) registry descriptors using the component's existing configuration.

### 4. State Tracking in Engine (Not Persisted)

**Decision:** `UpgradePlan` objects live in a `RwLock<HashMap>` inside the `UpgradeEngine`. They are not persisted to disk in this POC. A production implementation would persist plans for crash recovery.

### 5. Orchestrator Duplicate Check

**Problem:** `PluginOrchestrator::load_plugin_inner()` rejects loading a plugin with the same `plugin_id` as one already loaded.

**Decision:** The POC catches the "already loaded" error string and ignores it. A production implementation would add a dedicated `upgrade_plugin()` path to the orchestrator that skips or overrides the duplicate check.

---

## ABI Validation

The validation layer (`validation.rs`) enforces:

1. **SDK major.minor match** — New plugin's `sdk_version` must have same major.minor as the currently loaded plugin (patch differences allowed)
2. **Target triple match** — New binary must target the same platform (e.g., `aarch64-apple-darwin`)
3. **Version progression** — New plugin version must differ from current (prevents no-op upgrades)

---

## Test Coverage

### Unit Tests (15)

| Module | Tests |
|--------|-------|
| `plan.rs` | State machine transitions: `can_execute`, `can_rollback`, `can_cancel` for all states |
| `validation.rs` | Compatible versions, incompatible SDK, wrong target triple, same version rejection |
| `engine.rs` | `parse_plugin_kind` for source/reaction/invalid inputs |

### Integration Tests (4) — `tests/upgrade_test.rs`

| Test | What it exercises |
|------|-------------------|
| `test_upgrade_plan_and_execute_end_to_end` | Full lifecycle: load v1, create source, plan upgrade to v2, execute, verify source properties change from "1.0.0" to "2.0.0" |
| `test_upgrade_plan_rejects_missing_binary` | Error handling: non-existent binary path is rejected gracefully |
| `test_upgrade_multiple_sources_all_migrated` | Rolling migration of 3 sources: all upgraded sequentially, all report v2 properties |
| `test_upgrade_cancel_planned` | Cancel flow: plan created then cancelled, plan removed |

### Test Plugin

A dedicated `tests/fixtures/upgrade_test_plugin/` crate provides two cdylib binaries:
- **v1** — `libdrasi_source_upgrade_test.dylib` (plugin_version = "1.0.0")
- **v2** — `libdrasi_source_upgrade_test_v2.dylib` (plugin_version = "2.0.0")

Both register a "upgrade-test" source kind that embeds the version string in its `properties()`, enabling the test to verify the runtime swap actually happened.

Build with: `make build-upgrade-test-plugins`  
Run tests with: `make test-upgrade`

---

## Limitations & Known Gaps

| Area | Limitation | Production Fix |
|------|-----------|----------------|
| **Rollback** | Best-effort stop/restart (doesn't recreate with old factory) | Maintain `retiring` map of old descriptors |
| **Crash recovery** | In-progress plans lost on server restart | Persist plans to disk; check on startup |
| **Concurrency** | No lock preventing concurrent upgrades of the same plugin | Add upgrade mutex per plugin kind |
| **Registry conflict** | New plugin replaces old in registry immediately on load | Add `upgrade_plugin()` to orchestrator with staging |
| **Persistence** | Upgraded components not reflected in config persistence | Trigger `save()` after successful upgrade |
| **Health checks** | No post-upgrade health verification per component | Add configurable health probe between migrations |
| **Batch control** | All targets upgraded sequentially; no batch size control | Add `batchSize` parameter to plan |

---

## Files Modified (Existing)

| File | Change |
|------|--------|
| `src/lib.rs` | Added `pub mod upgrade;` |
| `src/api/v1/mod.rs` | Added `pub mod upgrade_handlers;` + re-export |
| `src/server.rs` | Created `UpgradeEngine`, nested upgrade router |

---

## Verification Results

```
$ cargo check
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.04s

$ cargo test --lib
test result: ok. 306 passed; 0 failed; 0 ignored

$ make build-upgrade-test-plugins
=== Building upgrade test plugins (v1 and v2) ===
=== Upgrade test plugins ready ===

$ cargo test --test upgrade_test -- --ignored
test result: ok. 4 passed; 0 failed; 0 ignored

$ cargo clippy -- -W clippy::all
# 0 issues in src/upgrade/ (pre-existing issue in src/api/models/solution.rs only)
```

---

## Recommended Next Steps

1. **Build test plugin binaries** — Create a `make build-upgrade-test-plugins` target that produces two versions of a mock plugin for integration testing
2. **Add `retiring` map to host-sdk** — Enable true dual-load by storing old descriptors alongside new ones, keyed by `(kind, version)`
3. **Orchestrator upgrade path** — Add `PluginOrchestrator::upgrade_plugin()` that explicitly handles the "replace existing" flow
4. **Persistence integration** — Call `ConfigPersistence::save()` after successful upgrades
5. **Crash recovery** — Serialize `UpgradePlan` to disk; on startup, resume or rollback incomplete plans
6. **Health probes** — Add configurable delay/health-check between component migrations
7. **API documentation** — Add utoipa annotations for OpenAPI spec generation
