// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Integration test for the rolling plugin upgrade engine.
//!
//! This test exercises the full upgrade lifecycle:
//! 1. Load a v1 plugin binary
//! 2. Create a DrasiLib instance with a source from v1
//! 3. Plan an upgrade to v2
//! 4. Execute the upgrade (rolling migration)
//! 5. Verify the plan completed and the source was replaced
//!
//! Requires the upgrade test plugin binaries. Run:
//!   `make build-upgrade-test-plugins`
//!
//! Tests are `#[ignore]` gated so they don't fail without the binaries.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_lib::DrasiLib;
use drasi_server::instance_registry::InstanceRegistry;
use drasi_server::plugin_orchestrator::PluginOrchestrator;
use drasi_server::plugin_registry::PluginRegistry;
use drasi_server::upgrade::engine::UpgradeEngine;
use drasi_server::upgrade::plan::UpgradeStatus;
use indexmap::IndexMap;
use tokio::sync::RwLock;

// =============================================================================
// Helpers
// =============================================================================

fn test_plugins_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/plugins")
}

/// Path to the v1 upgrade-test plugin binary.
fn upgrade_test_v1_path() -> PathBuf {
    let path = test_plugins_dir().join(plugin_filename("drasi_source_upgrade_test"));
    assert!(
        path.exists(),
        "Upgrade test plugin v1 not found at {}. Run `make build-upgrade-test-plugins` first.",
        path.display()
    );
    path
}

/// Path to the v2 upgrade-test plugin binary.
fn upgrade_test_v2_path() -> PathBuf {
    let path = test_plugins_dir().join(plugin_filename("drasi_source_upgrade_test_v2"));
    assert!(
        path.exists(),
        "Upgrade test plugin v2 not found at {}. Run `make build-upgrade-test-plugins` first.",
        path.display()
    );
    path
}

fn plugin_filename(stem: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{stem}.dylib")
    } else if cfg!(target_os = "windows") {
        format!("{stem}.dll")
    } else {
        format!("lib{stem}.so")
    }
}

fn new_orchestrator_with_registry() -> (Arc<PluginOrchestrator>, Arc<RwLock<PluginRegistry>>) {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));
    (orchestrator, registry)
}

/// Wait for a source to reach Running status (up to 5 seconds).
async fn wait_for_source_running(drasi_lib: &DrasiLib, source_id: &str) {
    for _ in 0..50 {
        let snapshot = drasi_lib.snapshot_configuration().await.unwrap();
        if let Some(src) = snapshot.sources.iter().find(|s| s.id == source_id) {
            if src.status == drasi_lib::ComponentStatus::Running {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("Source '{source_id}' did not reach Running state within 5 seconds");
}

/// Find user sources in snapshot (excludes the internal component-graph source).
fn user_sources(
    snapshot: &drasi_lib::config::snapshot::ConfigurationSnapshot,
) -> Vec<&drasi_lib::config::snapshot::SourceSnapshot> {
    snapshot
        .sources
        .iter()
        .filter(|s| s.source_type != "component_graph")
        .collect()
}

// =============================================================================
// 1. Full upgrade lifecycle — plan, execute, verify
// =============================================================================

/// End-to-end test: load v1, create source, upgrade to v2, verify completion.
#[tokio::test]
#[ignore = "requires upgrade test plugins — run `make build-upgrade-test-plugins` first"]
async fn test_upgrade_plan_and_execute_end_to_end() {
    let (orchestrator, registry) = new_orchestrator_with_registry();

    // ── Step 1: Load v1 plugin ──
    let v1_info = orchestrator
        .load_plugin(&upgrade_test_v1_path(), None)
        .await
        .expect("should load v1 plugin");

    assert_eq!(v1_info.plugin_version, "1.0.0");
    assert!(
        v1_info.kinds.iter().any(|k| k.kind == "upgrade-test"),
        "v1 should register 'upgrade-test' kind"
    );

    // ── Step 2: Create a DrasiLib instance with a source from v1 ──
    let source = {
        let reg = registry.read().await;
        let descriptor = reg
            .get_source("upgrade-test")
            .expect("upgrade-test source descriptor should be in registry");
        descriptor
            .create_source("test-source-1", &serde_json::json!({}), true)
            .await
            .expect("should create source from v1 descriptor")
    };

    let drasi_lib = DrasiLib::builder()
        .with_id("test-instance")
        .build()
        .await
        .expect("should build DrasiLib");

    drasi_lib
        .add_source(source)
        .await
        .expect("should add source to DrasiLib");

    // Start the source and wait for it to reach Running
    drasi_lib
        .start_source("test-source-1")
        .await
        .expect("should start source");
    wait_for_source_running(&drasi_lib, "test-source-1").await;

    // Verify source is registered and running
    let snapshot = drasi_lib
        .snapshot_configuration()
        .await
        .expect("should get snapshot");
    let sources = user_sources(&snapshot);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].source_type, "upgrade-test");
    assert_eq!(sources[0].id, "test-source-1");

    // Verify the source's properties contain v1's version
    let version_prop = sources[0]
        .properties
        .get("version")
        .expect("source should have version property");
    assert_eq!(version_prop, &serde_json::json!("1.0.0"));

    // ── Step 3: Set up InstanceRegistry and UpgradeEngine ──
    let mut instances = IndexMap::new();
    instances.insert("test-instance".to_string(), Arc::new(drasi_lib));
    let instance_registry = InstanceRegistry::from_map(instances);

    let engine = UpgradeEngine::new(
        instance_registry.clone(),
        registry.clone(),
        orchestrator.clone(),
    );

    // ── Step 4: Plan the upgrade ──
    let plan = engine
        .plan_upgrade("source/upgrade-test", &upgrade_test_v2_path())
        .await
        .expect("should create upgrade plan");

    assert_eq!(plan.plugin_kind, "source/upgrade-test");
    assert_eq!(plan.from_version, "1.0.0");
    assert_eq!(plan.to_version, "2.0.0");
    assert_eq!(plan.targets.len(), 1, "should find 1 dependent source");
    assert_eq!(plan.targets[0].component_id, "test-source-1");
    assert!(matches!(plan.status, UpgradeStatus::Planned));

    // ── Step 5: Execute the upgrade ──
    let completed_plan = engine
        .execute_upgrade(&plan.id)
        .await
        .expect("upgrade execution should succeed");

    assert!(
        matches!(completed_plan.status, UpgradeStatus::Complete),
        "Plan should be Complete, got: {:?}",
        completed_plan.status
    );
    assert!(completed_plan.started_at.is_some());
    assert!(completed_plan.completed_at.is_some());

    // ── Step 6: Verify the source was upgraded ──
    // The source should now have v2's properties
    let instance = instance_registry
        .get("test-instance")
        .await
        .expect("instance should still exist");
    let snapshot_after = instance
        .snapshot_configuration()
        .await
        .expect("should get post-upgrade snapshot");

    let sources_after = user_sources(&snapshot_after);
    assert_eq!(sources_after.len(), 1);
    assert_eq!(sources_after[0].source_type, "upgrade-test");

    // The new source instance was created by v2's descriptor, which embeds version "2.0.0"
    let version_after = sources_after[0]
        .properties
        .get("version")
        .expect("upgraded source should have version property");
    assert_eq!(
        version_after,
        &serde_json::json!("2.0.0"),
        "Source should now report v2.0.0 after upgrade"
    );
}

// =============================================================================
// 2. Plan validation — missing binary rejected
// =============================================================================

/// Verify that planning an upgrade with a non-existent binary fails gracefully.
#[tokio::test]
#[ignore = "requires upgrade test plugins — run `make build-upgrade-test-plugins` first"]
async fn test_upgrade_plan_rejects_missing_binary() {
    let (orchestrator, registry) = new_orchestrator_with_registry();

    orchestrator
        .load_plugin(&upgrade_test_v1_path(), None)
        .await
        .expect("should load v1 plugin");

    let instance_registry = InstanceRegistry::new();
    let engine = UpgradeEngine::new(instance_registry, registry, orchestrator);

    let result = engine
        .plan_upgrade("source/upgrade-test", &PathBuf::from("/nonexistent/plugin.dylib"))
        .await;

    assert!(result.is_err(), "Should reject missing binary");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("not found") || err_str.contains("Binary not found"),
        "Error should mention missing file, got: {err_str}"
    );
}

// =============================================================================
// 3. Upgrade with multiple sources
// =============================================================================

/// Verify rolling upgrade works correctly with multiple sources of the same kind.
#[tokio::test]
#[ignore = "requires upgrade test plugins — run `make build-upgrade-test-plugins` first"]
async fn test_upgrade_multiple_sources_all_migrated() {
    let (orchestrator, registry) = new_orchestrator_with_registry();

    orchestrator
        .load_plugin(&upgrade_test_v1_path(), None)
        .await
        .expect("should load v1");

    // Create 3 sources from v1
    let drasi_lib = DrasiLib::builder()
        .with_id("multi-instance")
        .build()
        .await
        .expect("should build DrasiLib");

    for i in 1..=3 {
        let source = {
            let reg = registry.read().await;
            let descriptor = reg.get_source("upgrade-test").unwrap();
            descriptor
                .create_source(&format!("source-{i}"), &serde_json::json!({}), true)
                .await
                .expect("should create source")
        };
        drasi_lib
            .add_source(source)
            .await
            .expect("should add source");
        drasi_lib
            .start_source(&format!("source-{i}"))
            .await
            .expect("should start source");
    }

    // Wait for all sources to reach Running
    for i in 1..=3 {
        wait_for_source_running(&drasi_lib, &format!("source-{i}")).await;
    }

    let mut instances = IndexMap::new();
    instances.insert("multi-instance".to_string(), Arc::new(drasi_lib));
    let instance_registry = InstanceRegistry::from_map(instances);

    let engine = UpgradeEngine::new(
        instance_registry.clone(),
        registry.clone(),
        orchestrator.clone(),
    );

    // Plan should find all 3 targets
    let plan = engine
        .plan_upgrade("source/upgrade-test", &upgrade_test_v2_path())
        .await
        .expect("should plan upgrade");

    assert_eq!(plan.targets.len(), 3);

    // Execute
    let completed = engine
        .execute_upgrade(&plan.id)
        .await
        .expect("should execute");

    assert!(matches!(completed.status, UpgradeStatus::Complete));

    // All 3 sources should now report v2
    let instance = instance_registry.get("multi-instance").await.unwrap();
    let snapshot = instance.snapshot_configuration().await.unwrap();
    for source in user_sources(&snapshot) {
        let version = source.properties.get("version").unwrap();
        assert_eq!(
            version,
            &serde_json::json!("2.0.0"),
            "Source '{}' should be upgraded to v2",
            source.id
        );
    }
}

// =============================================================================
// 4. Cancel a planned upgrade
// =============================================================================

/// Verify that a Planned upgrade can be cancelled.
#[tokio::test]
#[ignore = "requires upgrade test plugins — run `make build-upgrade-test-plugins` first"]
async fn test_upgrade_cancel_planned() {
    let (orchestrator, registry) = new_orchestrator_with_registry();

    orchestrator
        .load_plugin(&upgrade_test_v1_path(), None)
        .await
        .expect("should load v1");

    // Create a source so plan succeeds
    let drasi_lib = DrasiLib::builder()
        .with_id("cancel-test")
        .build()
        .await
        .expect("build");

    let source = {
        let reg = registry.read().await;
        let desc = reg.get_source("upgrade-test").unwrap();
        desc.create_source("src-1", &serde_json::json!({}), true)
            .await
            .unwrap()
    };
    drasi_lib.add_source(source).await.unwrap();
    drasi_lib.start_source("src-1").await.unwrap();
    wait_for_source_running(&drasi_lib, "src-1").await;

    let mut instances = IndexMap::new();
    instances.insert("cancel-test".to_string(), Arc::new(drasi_lib));
    let instance_registry = InstanceRegistry::from_map(instances);

    let engine = UpgradeEngine::new(instance_registry, registry, orchestrator);

    let plan = engine
        .plan_upgrade("source/upgrade-test", &upgrade_test_v2_path())
        .await
        .expect("should plan");

    // Cancel it
    engine
        .cancel_upgrade(&plan.id)
        .await
        .expect("should cancel planned upgrade");

    // Plan should be gone
    let retrieved = engine.get_plan(&plan.id).await;
    assert!(retrieved.is_none(), "Cancelled plan should be removed");
}
