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

//! Integration tests for the state_store configuration option.
//!
//! These tests verify:
//! - REDB StateStoreProvider can be created and used
//! - DrasiLib builder accepts state store provider
//! - state_store config setting is properly parsed and applied
//! - DrasiServerBuilder with_state_store_provider method works correctly
//! - Factory function creates providers correctly

use anyhow::Result;
use drasi_lib::state_store::StateStoreProvider;
use drasi_lib::DrasiLib;
use drasi_server::models::ConfigValue;
use drasi_server::{
    create_state_store_provider, DrasiServerBuilder, DrasiServerConfig, StateStoreConfig,
};
use drasi_state_store_redb::RedbStateStoreProvider;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that RedbStateStoreProvider can be created with valid path
#[test]
fn test_redb_state_store_provider_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join("state.redb");

    let provider = RedbStateStoreProvider::new(&path).expect("Failed to create provider");

    // Provider should be created successfully
    drop(provider); // Clean up
}

/// Test that StateStoreConfig::redb helper creates correct config
#[test]
fn test_state_store_config_redb_helper() {
    let config = StateStoreConfig::redb("./data/test.redb");
    assert_eq!(config.kind(), "redb");
}

/// Test that create_state_store_provider factory works for REDB
#[test]
fn test_create_state_store_provider_redb() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join("state.redb");

    let config = StateStoreConfig::Redb {
        path: ConfigValue::Static(path.to_string_lossy().to_string()),
    };

    let provider = create_state_store_provider(config).expect("Failed to create provider");

    // Provider should be successfully created as Arc
    drop(provider);
}

/// Test DrasiLib builder with REDB state store provider
#[tokio::test]
async fn test_drasi_lib_builder_with_redb_provider() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let state_path = temp_dir.path().join("state.redb");

    let provider = RedbStateStoreProvider::new(&state_path)?;

    // Build DrasiLib with the REDB provider
    let core = DrasiLib::builder()
        .with_id("test-state-store")
        .with_state_store_provider(Arc::new(provider))
        .build()
        .await?;

    // Start and stop to verify basic operation
    core.start().await?;
    assert!(core.is_running().await);

    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}

/// Test DrasiServerBuilder with state store provider
#[tokio::test]
async fn test_drasi_server_builder_with_state_store_provider() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let state_path = temp_dir.path().join("state.redb");

    let provider = RedbStateStoreProvider::new(&state_path)?;

    // Build using DrasiServerBuilder
    let core = DrasiServerBuilder::new()
        .with_id("test-server-state")
        .with_state_store_provider(Arc::new(provider))
        .build_core()
        .await?;

    // Start and verify
    core.start().await?;
    assert!(core.is_running().await);

    core.stop().await?;

    Ok(())
}

/// Test that state_store config is correctly deserialized
#[test]
fn test_state_store_config_deserialization() {
    let yaml = r#"
        id: test-server
        host: 127.0.0.1
        port: 8080
        state_store:
          kind: redb
          path: ./data/state.redb
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert!(
        config.state_store.is_some(),
        "state_store should be present"
    );
    assert_eq!(config.state_store.as_ref().unwrap().kind(), "redb");
}

/// Test that state_store defaults to None when not specified
#[test]
fn test_state_store_config_default() {
    let yaml = r#"
        id: test-server
        host: 127.0.0.1
        port: 8080
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert!(
        config.state_store.is_none(),
        "state_store should default to None when not specified in config"
    );
}

/// Test full config with state_store alongside other settings
#[test]
fn test_state_store_with_full_config() {
    let yaml = r#"
        id: production-server
        host: 0.0.0.0
        port: 9090
        log_level: debug
        disable_persistence: false
        persist_index: true
        state_store:
          kind: redb
          path: /var/lib/drasi/state.redb
        sources: []
        queries: []
        reactions: []
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");

    assert!(config.state_store.is_some());
    assert_eq!(config.state_store.as_ref().unwrap().kind(), "redb");
    assert!(config.persist_index);
    assert!(!config.disable_persistence);

    match &config.port {
        ConfigValue::Static(port) => assert_eq!(*port, 9090),
        _ => panic!("Expected static port value"),
    }
}

/// Test config serialization roundtrip preserves state_store
#[test]
fn test_state_store_serialization_roundtrip() {
    let original = DrasiServerConfig {
        state_store: Some(StateStoreConfig::redb("./data/test.redb")),
        ..Default::default()
    };

    let yaml = serde_yaml::to_string(&original).expect("Failed to serialize config");

    assert!(
        yaml.contains("state_store:"),
        "Serialized config should contain 'state_store:'"
    );
    assert!(
        yaml.contains("kind: redb"),
        "Serialized config should contain 'kind: redb'"
    );

    let deserialized: DrasiServerConfig =
        serde_yaml::from_str(&yaml).expect("Failed to deserialize config");

    assert!(
        deserialized.state_store.is_some(),
        "Deserialized config should have state_store"
    );
    assert_eq!(deserialized.state_store.as_ref().unwrap().kind(), "redb");
}

/// Test that state store file is created when REDB provider is used
#[tokio::test]
async fn test_redb_creates_database_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let state_path = temp_dir.path().join("drasi-state.redb");

    let provider = RedbStateStoreProvider::new(&state_path)?;

    // Build DrasiLib with the provider
    let core = DrasiLib::builder()
        .with_id("test-file-creation")
        .with_state_store_provider(Arc::new(provider))
        .build()
        .await?;

    // Start to trigger initialization
    core.start().await?;

    // Give it a moment for async operations
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    core.stop().await?;

    // The state file should now exist
    assert!(
        state_path.exists(),
        "State store file should exist after initialization"
    );

    Ok(())
}

/// Test that REDB state store operations work correctly
#[tokio::test]
async fn test_redb_state_store_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let state_path = temp_dir.path().join("ops-test.redb");

    let provider = RedbStateStoreProvider::new(&state_path)?;

    // Test basic operations
    let store_id = "test-store";
    let key = "test-key";
    let value = b"test-value".to_vec();

    // Set a value
    provider.set(store_id, key, value.clone()).await?;

    // Get the value back
    let retrieved = provider.get(store_id, key).await?;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), value);

    // Check key exists
    assert!(provider.contains_key(store_id, key).await?);

    // Delete the key
    let deleted = provider.delete(store_id, key).await?;
    assert!(deleted);

    // Verify deleted
    assert!(!provider.contains_key(store_id, key).await?);

    Ok(())
}

/// Test that two separate instances can use different REDB state stores
#[tokio::test]
async fn test_redb_provider_isolation() -> Result<()> {
    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;

    let provider1 = RedbStateStoreProvider::new(temp_dir1.path().join("state1.redb"))?;
    let provider2 = RedbStateStoreProvider::new(temp_dir2.path().join("state2.redb"))?;

    // Build two independent cores
    let core1 = DrasiLib::builder()
        .with_id("test-isolation-1")
        .with_state_store_provider(Arc::new(provider1))
        .build()
        .await?;

    let core2 = DrasiLib::builder()
        .with_id("test-isolation-2")
        .with_state_store_provider(Arc::new(provider2))
        .build()
        .await?;

    // Both can start independently
    core1.start().await?;
    core2.start().await?;

    assert!(core1.is_running().await);
    assert!(core2.is_running().await);

    core1.stop().await?;
    core2.stop().await?;

    Ok(())
}

/// Test state_store with environment variable in path
#[test]
fn test_state_store_with_env_var_path() {
    let yaml = r#"
        id: test-server
        host: 127.0.0.1
        port: 8080
        state_store:
          kind: redb
          path: ${STATE_STORE_PATH:-./data/default.redb}
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert!(config.state_store.is_some());
}

/// Test multi-instance config with different state stores
#[test]
fn test_multi_instance_state_store_config() {
    let yaml = r#"
        id: multi-instance-server
        host: 0.0.0.0
        port: 8080
        instances:
          - id: instance-1
            persist_index: true
            state_store:
              kind: redb
              path: ./data/instance1-state.redb
            sources: []
            queries: []
            reactions: []
          - id: instance-2
            persist_index: false
            state_store:
              kind: redb
              path: ./data/instance2-state.redb
            sources: []
            queries: []
            reactions: []
          - id: instance-3
            persist_index: false
            sources: []
            queries: []
            reactions: []
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert_eq!(config.instances.len(), 3);

    // Instance 1 and 2 have state_store configured
    assert!(config.instances[0].state_store.is_some());
    assert!(config.instances[1].state_store.is_some());

    // Instance 3 does not have state_store
    assert!(config.instances[2].state_store.is_none());

    // Verify the paths are different
    match (
        &config.instances[0].state_store,
        &config.instances[1].state_store,
    ) {
        (Some(StateStoreConfig::Redb { path: p1 }), Some(StateStoreConfig::Redb { path: p2 })) => {
            if let (ConfigValue::Static(path1), ConfigValue::Static(path2)) = (p1, p2) {
                assert_ne!(
                    path1, path2,
                    "Each instance should have its own state store path"
                );
            }
        }
        _ => panic!("Expected REDB state stores for both instances"),
    }
}
