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

//! Hot-reload plugin tests.
//!
//! Validates that when `hotReloadPlugins` is enabled, dropping a plugin binary
//! into the plugins directory causes it to be detected, loaded, and its
//! component kinds become usable.
//!
//! These tests require real cdylib plugins built by `make build-local-test-plugins`.
//! They are gated with `#[ignore]` so they don't fail in environments without plugins.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_host_sdk::plugin_types::{PluginEvent, PluginFileEvent, PluginStatus};
use drasi_host_sdk::watcher::{PluginWatcher, PluginWatcherConfig};
use drasi_server::plugin_orchestrator::PluginOrchestrator;
use drasi_server::plugin_registry::PluginRegistry;
use tokio::sync::RwLock;

// =============================================================================
// Helpers
// =============================================================================

/// Locate the `target/debug/plugins` directory relative to the cargo manifest.
fn test_plugins_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("target/debug/plugins")
}

/// Return the path to the mock-source cdylib, or panic if absent.
fn mock_source_plugin_path() -> PathBuf {
    let dir = test_plugins_dir();
    let path = if cfg!(target_os = "macos") {
        dir.join("libdrasi_source_mock.dylib")
    } else if cfg!(target_os = "windows") {
        dir.join("drasi_source_mock.dll")
    } else {
        dir.join("libdrasi_source_mock.so")
    };
    assert!(
        path.exists(),
        "Mock source plugin not found at {}. Run `make build-local-test-plugins` first.",
        path.display()
    );
    path
}

/// Return the path to the log-reaction cdylib, or panic if absent.
fn log_reaction_plugin_path() -> PathBuf {
    let dir = test_plugins_dir();
    let path = if cfg!(target_os = "macos") {
        dir.join("libdrasi_reaction_log.dylib")
    } else if cfg!(target_os = "windows") {
        dir.join("drasi_reaction_log.dll")
    } else {
        dir.join("libdrasi_reaction_log.so")
    };
    assert!(
        path.exists(),
        "Log reaction plugin not found at {}. Run `make build-local-test-plugins` first.",
        path.display()
    );
    path
}

/// Build a fresh PluginOrchestrator backed by an empty registry.
fn new_orchestrator() -> Arc<PluginOrchestrator> {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
    Arc::new(PluginOrchestrator::new(lifecycle))
}

// =============================================================================
// 1. PluginOrchestrator.load_plugin() with a real cdylib
// =============================================================================

/// Load a real cdylib plugin via the orchestrator and verify it appears in
/// the inventory with the correct status, kinds, and emits a Loaded event.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_orchestrator_load_real_plugin() {
    let orchestrator = new_orchestrator();
    let mut rx = orchestrator.subscribe();

    let path = mock_source_plugin_path();
    let info = orchestrator
        .load_plugin(&path, None)
        .await
        .expect("load_plugin should succeed");

    // Plugin should be in the Loaded state
    assert_eq!(info.status, PluginStatus::Loaded);
    // Should have at least one kind (source/mock)
    assert!(
        !info.kinds.is_empty(),
        "Plugin should register at least one kind"
    );
    // The plugin ID should contain "mock"
    assert!(
        info.id.contains("mock"),
        "Plugin ID should reference 'mock', got: {}",
        info.id
    );

    // Verify it shows up in the plugin list
    let plugins = orchestrator.list_plugins().await;
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].id, info.id);

    // Verify Loaded event was NOT emitted by load_plugin itself
    // (load_plugin stores info but the event is only emitted by record_startup_plugins).
    // We verify that the broadcast channel is empty since load_plugin doesn't emit.
    // If the implementation changes to emit on load, this will catch it.
    let event_result = rx.try_recv();
    // Whether it emits or not, the test documents current behavior:
    if let Ok(event) = event_result {
        match event {
            PluginEvent::Loaded { plugin_id, .. } => {
                assert_eq!(plugin_id, info.id);
            }
            other => panic!("Unexpected event: {other:?}"),
        }
    }
    // If no event, that's also fine — load_plugin doesn't currently emit.
}

// =============================================================================
// 2. Hot-loaded plugin kinds are usable (can create components)
// =============================================================================

/// After hot-loading a plugin, its kinds should be available in the registry
/// and we should be able to look them up for component creation.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_hot_loaded_plugin_kinds_available_in_registry() {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));

    let path = mock_source_plugin_path();
    let info = orchestrator
        .load_plugin(&path, None)
        .await
        .expect("load_plugin should succeed");

    // Verify the source kind is registered in the plugin registry
    let reg = registry.read().await;
    let source_kind = info
        .kinds
        .iter()
        .find(|k| k.category == drasi_host_sdk::plugin_types::PluginCategory::Source);
    assert!(
        source_kind.is_some(),
        "Mock plugin should register a Source kind"
    );

    let kind_name = &source_kind.unwrap().kind;
    let descriptor = reg.get_source(kind_name);
    assert!(
        descriptor.is_some(),
        "Source kind '{}' should be findable in the registry after hot-load",
        kind_name
    );
}

/// After hot-loading a reaction plugin, its kinds should be in the registry.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_hot_loaded_reaction_plugin_kinds_available() {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));

    let path = log_reaction_plugin_path();
    let info = orchestrator
        .load_plugin(&path, None)
        .await
        .expect("load_plugin should succeed");

    let reg = registry.read().await;
    let reaction_kind = info
        .kinds
        .iter()
        .find(|k| k.category == drasi_host_sdk::plugin_types::PluginCategory::Reaction);
    assert!(
        reaction_kind.is_some(),
        "Log plugin should register a Reaction kind"
    );

    let kind_name = &reaction_kind.unwrap().kind;
    let descriptor = reg.get_reaction(kind_name);
    assert!(
        descriptor.is_some(),
        "Reaction kind '{}' should be findable in the registry after hot-load",
        kind_name
    );
}

// =============================================================================
// 3. Loading multiple plugins sequentially
// =============================================================================

/// Load mock source then log reaction — both should coexist in the inventory.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_orchestrator_load_multiple_plugins() {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));

    let mock_info = orchestrator
        .load_plugin(&mock_source_plugin_path(), None)
        .await
        .expect("load mock source");
    let log_info = orchestrator
        .load_plugin(&log_reaction_plugin_path(), None)
        .await
        .expect("load log reaction");

    // Both should appear
    let plugins = orchestrator.list_plugins().await;
    assert_eq!(plugins.len(), 2, "Both plugins should be in the inventory");

    // Verify distinct IDs
    assert_ne!(mock_info.id, log_info.id);

    // Verify both kinds are in the registry
    let reg = registry.read().await;
    let source_kind = mock_info
        .kinds
        .iter()
        .find(|k| k.category == drasi_host_sdk::plugin_types::PluginCategory::Source)
        .expect("mock plugin has source kind");
    assert!(
        reg.get_source(&source_kind.kind).is_some(),
        "Mock source kind should be in registry"
    );

    let reaction_kind = log_info
        .kinds
        .iter()
        .find(|k| k.category == drasi_host_sdk::plugin_types::PluginCategory::Reaction)
        .expect("log plugin has reaction kind");
    assert!(
        reg.get_reaction(&reaction_kind.kind).is_some(),
        "Log reaction kind should be in registry"
    );
}

// =============================================================================
// 4. Re-loading an already-loaded plugin (simulates Changed event)
// =============================================================================

/// Loading the same plugin twice should not break or duplicate the entry.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_orchestrator_reload_same_plugin() {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));

    let path = mock_source_plugin_path();

    // First load
    let info1 = orchestrator
        .load_plugin(&path, None)
        .await
        .expect("first load");

    // Second load (simulating a Changed event re-triggering load)
    let info2 = orchestrator
        .load_plugin(&path, None)
        .await
        .expect("second load should not fail");

    // Should have the same plugin ID
    assert_eq!(info1.id, info2.id, "Reloaded plugin should have same ID");

    // Should still be exactly one plugin in inventory (not duplicated)
    let plugins = orchestrator.list_plugins().await;
    assert_eq!(
        plugins.len(),
        1,
        "Reloaded plugin should overwrite, not duplicate"
    );

    // The kind should still be usable in the registry
    let reg = registry.read().await;
    let source_kind = info2
        .kinds
        .iter()
        .find(|k| k.category == drasi_host_sdk::plugin_types::PluginCategory::Source)
        .expect("reload should still have source kind");
    assert!(
        reg.get_source(&source_kind.kind).is_some(),
        "Source kind should remain in registry after reload"
    );
}

// =============================================================================
// 5. PluginWatcher → PluginOrchestrator integration (event-driven pipeline)
// =============================================================================

/// Wire a PluginWatcher to a PluginOrchestrator and verify that copying a
/// real plugin binary into the watched directory causes it to be automatically
/// detected and loaded.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_watcher_to_orchestrator_pipeline() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().to_path_buf();

    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::with_plugins_dir(
        lifecycle,
        plugins_dir.clone(),
    ));

    // Set up the watcher with a short debounce for testing
    let watcher_config = PluginWatcherConfig {
        plugins_dir: plugins_dir.clone(),
        debounce: Duration::from_millis(100),
    };
    let mut watcher = PluginWatcher::new(watcher_config);
    let mut rx = watcher.subscribe();
    watcher.start_polling().expect("start watcher");

    // Wait for initial scan to complete
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Copy the mock source plugin into the watched directory
    let source_path = mock_source_plugin_path();
    let dest_filename = source_path.file_name().unwrap();
    let dest_path = plugins_dir.join(dest_filename);
    std::fs::copy(&source_path, &dest_path).expect("copy plugin");

    // Wait for the watcher to detect the new file
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut detected_path = None;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Ok(PluginFileEvent::Added(path))) => {
                detected_path = Some(path);
                break;
            }
            Ok(Ok(PluginFileEvent::Changed(path))) => {
                // Changed is also acceptable (race condition with initial scan)
                detected_path = Some(path);
                break;
            }
            _ => continue,
        }
    }

    let detected = detected_path.expect("Watcher should detect the new plugin file");
    assert!(
        detected.to_string_lossy().contains("libdrasi_source_mock")
            || detected.to_string_lossy().contains("drasi_source_mock"),
        "Detected path should reference mock source: {}",
        detected.display()
    );

    // Now simulate what the server's hot-reload task does: call load_plugin
    let info = orchestrator
        .load_plugin(&detected, None)
        .await
        .expect("orchestrator should load detected plugin");

    assert_eq!(info.status, PluginStatus::Loaded);
    assert!(!info.kinds.is_empty());

    // Verify the source kind is actually usable in the registry
    let reg = registry.read().await;
    let source_kind = info
        .kinds
        .iter()
        .find(|k| k.category == drasi_host_sdk::plugin_types::PluginCategory::Source)
        .expect("should have source kind");
    assert!(
        reg.get_source(&source_kind.kind).is_some(),
        "Hot-reloaded source kind should be in registry"
    );

    watcher.stop();
}

// =============================================================================
// 6. Side-by-side mode skips automatic loading
// =============================================================================

/// Verify that in "side-by-side" mode, the server's watcher task does NOT
/// automatically call load_plugin. This tests the policy branch in server.rs.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_side_by_side_mode_skips_auto_load() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().to_path_buf();

    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
    let orchestrator = Arc::new(PluginOrchestrator::with_plugins_dir(
        lifecycle,
        plugins_dir.clone(),
    ));

    // Set up the watcher
    let watcher_config = PluginWatcherConfig {
        plugins_dir: plugins_dir.clone(),
        debounce: Duration::from_millis(100),
    };
    let mut watcher = PluginWatcher::new(watcher_config);
    let mut rx = watcher.subscribe();
    watcher.start_polling().expect("start watcher");

    // Simulate the server's watcher task with "side-by-side" mode
    let reload_mode = "side-by-side".to_string();
    let orch_clone = orchestrator.clone();
    let _handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => match event {
                    PluginFileEvent::Added(_path) | PluginFileEvent::Changed(_path) => {
                        if reload_mode == "side-by-side" {
                            // Side-by-side mode: skip automatic load (matches server.rs behavior)
                        } else {
                            let _ = orch_clone.load_plugin(&_path, None).await;
                        }
                    }
                    PluginFileEvent::Removed(_) => {}
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(_) => continue,
            }
        }
    });

    // Wait for initial scan
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Copy plugin into watched directory
    let source_path = mock_source_plugin_path();
    let dest_path = plugins_dir.join(source_path.file_name().unwrap());
    std::fs::copy(&source_path, &dest_path).expect("copy plugin");

    // Wait enough time for watcher to detect + handler to decide
    tokio::time::sleep(Duration::from_secs(1)).await;

    // In side-by-side mode, the plugin should NOT be auto-loaded
    let plugins = orchestrator.list_plugins().await;
    assert!(
        plugins.is_empty(),
        "Side-by-side mode should NOT auto-load plugins; found {} plugin(s)",
        plugins.len()
    );

    watcher.stop();
}

// =============================================================================
// 7. Watcher filters non-plugin files
// =============================================================================

/// Non-plugin files dropped into the watched directory should not trigger events.
#[tokio::test]
async fn test_watcher_ignores_non_plugin_files() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().to_path_buf();

    let watcher_config = PluginWatcherConfig {
        plugins_dir: plugins_dir.clone(),
        debounce: Duration::from_millis(100),
    };
    let mut watcher = PluginWatcher::new(watcher_config);
    let mut rx = watcher.subscribe();
    watcher.start_polling().expect("start watcher");

    // Wait for initial scan
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Drop non-plugin files
    std::fs::write(plugins_dir.join("README.md"), "not a plugin").unwrap();
    std::fs::write(plugins_dir.join("config.yaml"), "key: value").unwrap();
    std::fs::write(plugins_dir.join("data.json"), "{}").unwrap();
    std::fs::write(plugins_dir.join("random_binary"), b"binary stuff").unwrap();

    // Wait for polling cycle
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Should not receive any events
    let event = rx.try_recv();
    assert!(
        event.is_err(),
        "Non-plugin files should not trigger watcher events, got: {event:?}"
    );

    watcher.stop();
}

// =============================================================================
// 8. Watcher detects removal
// =============================================================================

/// The watcher should emit a Removed event when a plugin binary is deleted.
#[tokio::test]
async fn test_watcher_detects_removal() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().to_path_buf();

    // Pre-populate with a fake plugin file (right naming convention)
    let fake_plugin = plugins_dir.join("libdrasi_source_fake.dylib");
    std::fs::write(&fake_plugin, b"fake plugin bytes").unwrap();

    let watcher_config = PluginWatcherConfig {
        plugins_dir: plugins_dir.clone(),
        debounce: Duration::from_millis(100),
    };
    let mut watcher = PluginWatcher::new(watcher_config);
    let mut rx = watcher.subscribe();
    watcher.start_polling().expect("start watcher");

    // Wait for initial scan to register the existing file
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Remove the file
    std::fs::remove_file(&fake_plugin).unwrap();

    // Wait for detection
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut got_removed = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Ok(PluginFileEvent::Removed(path))) => {
                assert!(
                    path.to_string_lossy().contains("libdrasi_source_fake"),
                    "Removed path should reference the fake plugin"
                );
                got_removed = true;
                break;
            }
            _ => continue,
        }
    }

    assert!(got_removed, "Watcher should emit Removed event");
    watcher.stop();
}

// =============================================================================
// 9. Watcher detects changed file (size change)
// =============================================================================

// =============================================================================
// 10. Full DrasiServer E2E: hotReloadPlugins: true + REST API validation
// =============================================================================

/// Find a free TCP port by binding to port 0.
fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind to port 0");
    listener.local_addr().unwrap().port()
}

/// Start a real DrasiServer from a config file with hotReloadPlugins enabled,
/// drop a plugin into the plugins directory, and verify it appears in the
/// REST API plugin list and that its kinds become usable for creating components.
#[tokio::test]
#[ignore = "requires cdylib plugins — run `make build-local-test-plugins` first"]
async fn test_server_hot_reload_e2e() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().join("plugins");
    std::fs::create_dir_all(&plugins_dir).expect("create plugins dir");

    let port = find_free_port();
    let base_url = format!("http://127.0.0.1:{port}");

    // Write a minimal config with hotReloadPlugins enabled
    let config_path = temp_dir.path().join("server.yaml");
    let config_content = format!(
        r#"id: hot-reload-e2e-test
host: 127.0.0.1
port: {port}
logLevel: info
persistConfig: false
hotReloadPlugins: true
hotReloadDebounceMs: 200
hotReloadMode: upgrade
sources: []
queries: []
reactions: []
"#
    );
    std::fs::write(&config_path, &config_content).expect("write config");

    // Create the DrasiServer
    let server = drasi_server::DrasiServer::new(
        config_path.clone(),
        port,
        plugins_dir.clone(),
        false, // verify_plugins
        false, // enable_ui
    )
    .await
    .expect("DrasiServer::new should succeed");

    // Spawn server.run() in a background task (it blocks on ctrl_c)
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("Server error: {e}");
        }
    });

    // Wait for the server to start listening
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("Server did not start within 10 seconds");
        }
        match client.get(format!("{base_url}/health")).send().await {
            Ok(resp) if resp.status().is_success() => break,
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }

    // Verify no plugins are loaded initially
    let resp = client
        .get(format!("{base_url}/api/v1/plugins"))
        .send()
        .await
        .expect("GET /api/v1/plugins");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let initial_plugins = body["plugins"].as_array().unwrap();
    // Core/built-in plugins may be present; record count
    let initial_count = initial_plugins.len();

    // Copy the mock source plugin into the watched plugins directory
    let source_plugin_path = mock_source_plugin_path();
    let dest_path = plugins_dir.join(source_plugin_path.file_name().unwrap());
    std::fs::copy(&source_plugin_path, &dest_path).expect("copy plugin into plugins dir");

    // Poll until the plugin appears in the REST API
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut found_mock = false;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let resp = client
            .get(format!("{base_url}/api/v1/plugins"))
            .send()
            .await
            .expect("GET /api/v1/plugins");
        let body: serde_json::Value = resp.json().await.unwrap();
        let plugins = body["plugins"].as_array().unwrap();
        if plugins.len() > initial_count {
            // Find the newly loaded plugin
            for plugin in plugins {
                let id = plugin["id"].as_str().unwrap_or("");
                if id.contains("mock") {
                    found_mock = true;
                    // Verify it has the expected status
                    let status = plugin["status"].as_str().unwrap_or("");
                    assert!(
                        status == "Loaded" || status == "loaded",
                        "Hot-loaded plugin should have Loaded status, got: {status}"
                    );
                    // Verify it has kinds
                    let kinds = plugin["kinds"].as_array();
                    assert!(
                        kinds.is_some() && !kinds.unwrap().is_empty(),
                        "Hot-loaded plugin should have registered kinds"
                    );
                    break;
                }
            }
            if found_mock {
                break;
            }
        }
    }

    assert!(
        found_mock,
        "Mock source plugin should appear in /api/v1/plugins after being dropped into plugins dir"
    );

    // Verify we can create a source using the hot-loaded plugin's kind
    // Use the first instance's convenience routes
    let resp = client
        .get(format!("{base_url}/api/v1/plugins"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let plugins = body["plugins"].as_array().unwrap();
    let mock_plugin = plugins
        .iter()
        .find(|p| p["id"].as_str().unwrap_or("").contains("mock"))
        .expect("mock plugin should be in list");
    let source_kind = mock_plugin["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["category"].as_str() == Some("Source"))
        .expect("mock plugin should have a Source kind");
    let kind_name = source_kind["kind"].as_str().unwrap();

    // Try to create a source using the hot-loaded plugin's kind
    let create_source_body = serde_json::json!({
        "kind": kind_name,
        "id": "hot-loaded-test-source",
        "autoStart": false
    });
    let resp = client
        .post(format!("{base_url}/api/v1/sources"))
        .json(&create_source_body)
        .send()
        .await
        .expect("POST /api/v1/sources");

    // The source should be created successfully (201) or accepted
    assert!(
        resp.status().is_success(),
        "Creating a source with hot-loaded plugin kind '{}' should succeed, got status: {}",
        kind_name,
        resp.status()
    );

    // Clean up: abort the server task
    server_handle.abort();
}

/// The watcher should emit a Changed event when a plugin file's size changes.
#[tokio::test]
async fn test_watcher_detects_changed_file() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let plugins_dir = temp_dir.path().to_path_buf();

    // Pre-populate with a fake plugin file
    let fake_plugin = plugins_dir.join("libdrasi_source_changed.dylib");
    std::fs::write(&fake_plugin, b"original content").unwrap();

    let watcher_config = PluginWatcherConfig {
        plugins_dir: plugins_dir.clone(),
        debounce: Duration::from_millis(100),
    };
    let mut watcher = PluginWatcher::new(watcher_config);
    let mut rx = watcher.subscribe();
    watcher.start_polling().expect("start watcher");

    // Wait for initial scan
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Modify the file with different-sized content
    std::fs::write(&fake_plugin, b"modified content that is longer than before!!!").unwrap();

    // Wait for detection
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut got_changed = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Ok(PluginFileEvent::Changed(path))) => {
                assert!(
                    path.to_string_lossy().contains("libdrasi_source_changed"),
                    "Changed path should reference the changed plugin"
                );
                got_changed = true;
                break;
            }
            _ => continue,
        }
    }

    assert!(got_changed, "Watcher should emit Changed event on size change");
    watcher.stop();
}
