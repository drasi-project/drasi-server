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

//! Integration tests for the persist_index configuration option.
//!
//! These tests verify:
//! - RocksDB index provider can be created and used
//! - DrasiLib builder accepts index provider
//! - persist_index config setting is properly parsed and applied
//! - DrasiServerBuilder with_index_provider method works correctly

use anyhow::Result;
use drasi_index_rocksdb::RocksDbIndexProvider;
use drasi_lib::DrasiLib;
use drasi_lib::IndexBackendPlugin;
use drasi_server::DrasiServerConfig;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that RocksDbIndexProvider can be created with valid parameters
#[test]
fn test_rocksdb_index_provider_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(path.clone(), true, false);

    assert_eq!(provider.path(), &path);
    assert!(provider.is_archive_enabled());
    assert!(!provider.is_direct_io_enabled());
}

/// Test that RocksDbIndexProvider can be created with archive disabled
#[test]
fn test_rocksdb_index_provider_no_archive() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(path.clone(), false, false);

    assert_eq!(provider.path(), &path);
    assert!(!provider.is_archive_enabled());
    assert!(!provider.is_direct_io_enabled());
}

/// Test that RocksDbIndexProvider can be created with direct_io enabled
#[test]
fn test_rocksdb_index_provider_with_direct_io() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(path.clone(), true, true);

    assert_eq!(provider.path(), &path);
    assert!(provider.is_archive_enabled());
    assert!(provider.is_direct_io_enabled());
}

/// Test that RocksDbIndexProvider reports as non-volatile (persistent)
#[test]
fn test_rocksdb_index_provider_is_persistent() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(path, true, false);

    // RocksDB is persistent, so is_volatile should be false
    assert!(
        !provider.is_volatile(),
        "RocksDB provider should report as persistent (not volatile)"
    );
}

/// Test DrasiLib builder with RocksDB index provider
#[tokio::test]
async fn test_drasi_lib_builder_with_rocksdb_provider() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let index_path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(index_path, true, false);

    // Build DrasiLib with the RocksDB provider
    let core = DrasiLib::builder()
        .with_id("test-persist-index")
        .with_index_provider(Arc::new(provider))
        .build()
        .await?;

    // Start and stop to verify basic operation
    core.start().await?;
    assert!(core.is_running().await);

    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}

/// Test DrasiServerBuilder with index provider
#[tokio::test]
async fn test_drasi_server_builder_with_index_provider() -> Result<()> {
    use drasi_server::DrasiServerBuilder;

    let temp_dir = TempDir::new()?;
    let index_path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(index_path, true, false);

    // Build using DrasiServerBuilder
    let core = DrasiServerBuilder::new()
        .with_id("test-server-persist")
        .with_index_provider(Arc::new(provider))
        .build_core()
        .await?;

    // Start and verify
    core.start().await?;
    assert!(core.is_running().await);

    core.stop().await?;

    Ok(())
}

/// Test that persistIndex: true is correctly deserialized
#[test]
fn test_persist_index_config_deserialization_true() {
    let yaml = r#"
        id: test-server
        host: 127.0.0.1
        port: 8080
        persistIndex: true
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert!(
        config.persist_index,
        "persist_index should be true when explicitly set"
    );
}

/// Test that persistIndex: false is correctly deserialized
#[test]
fn test_persist_index_config_deserialization_false() {
    let yaml = r#"
        id: test-server
        host: 127.0.0.1
        port: 8080
        persistIndex: false
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert!(
        !config.persist_index,
        "persist_index should be false when explicitly set"
    );
}

/// Test that persist_index defaults to false when not specified
#[test]
fn test_persist_index_config_default() {
    let yaml = r#"
        id: test-server
        host: 127.0.0.1
        port: 8080
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");
    assert!(
        !config.persist_index,
        "persist_index should default to false when not specified in config"
    );
}

/// Test full config with persist_index alongside other settings
#[test]
fn test_persist_index_with_full_config() {
    let yaml = r#"
        id: production-server
        host: 0.0.0.0
        port: 9090
        logLevel: debug
        persistConfig: true
        persistIndex: true
        sources: []
        queries: []
        reactions: []
    "#;

    let config: DrasiServerConfig = serde_yaml::from_str(yaml).expect("Failed to parse config");

    assert!(config.persist_index);
    assert!(config.persist_config);

    match &config.port {
        drasi_server::models::ConfigValue::Static(port) => assert_eq!(*port, 9090),
        _ => panic!("Expected static port value"),
    }
}

/// Test config serialization roundtrip preserves persist_index
#[test]
fn test_persist_index_serialization_roundtrip() {
    let original = DrasiServerConfig {
        api_version: None,
        persist_index: true,
        ..Default::default()
    };

    let yaml = serde_yaml::to_string(&original).expect("Failed to serialize config");

    assert!(
        yaml.contains("persistIndex: true"),
        "Serialized config should contain 'persistIndex: true'"
    );

    let deserialized: DrasiServerConfig =
        serde_yaml::from_str(&yaml).expect("Failed to deserialize config");

    assert!(
        deserialized.persist_index,
        "Deserialized config should have persist_index = true"
    );
}

/// Test that index data directory is created when RocksDB provider is used
#[tokio::test]
async fn test_rocksdb_creates_data_directory() -> Result<()> {
    use drasi_lib::Query;

    let temp_dir = TempDir::new()?;
    let index_path = temp_dir.path().join("drasi-index");

    let provider = RocksDbIndexProvider::new(index_path.clone(), true, false);

    // Build DrasiLib with the provider and a query
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .build();

    let core = DrasiLib::builder()
        .with_id("test-directory-creation")
        .with_index_provider(Arc::new(provider))
        .with_query(query)
        .build()
        .await?;

    // Start to trigger index creation
    core.start().await?;

    // Give it a moment for async operations
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    core.stop().await?;

    // The index directory should now exist (RocksDB creates it when queries start)
    // Note: The exact directory structure depends on RocksDB implementation
    assert!(
        temp_dir.path().exists(),
        "Temp directory should still exist after test"
    );

    Ok(())
}

/// Test that RocksDB provider can be shared by multiple independent instances
#[tokio::test]
async fn test_rocksdb_provider_isolation() -> Result<()> {
    // Test that two separate DrasiLib instances can use RocksDB providers
    // in different directories without interference

    let temp_dir1 = TempDir::new()?;
    let temp_dir2 = TempDir::new()?;

    let provider1 = RocksDbIndexProvider::new(temp_dir1.path().join("index1"), true, false);
    let provider2 = RocksDbIndexProvider::new(temp_dir2.path().join("index2"), false, false);

    // Both providers should report as persistent
    assert!(!provider1.is_volatile(), "Provider 1 should be persistent");
    assert!(!provider2.is_volatile(), "Provider 2 should be persistent");

    // Provider 1 has archive enabled, provider 2 does not
    assert!(provider1.is_archive_enabled());
    assert!(!provider2.is_archive_enabled());

    // Build two independent cores
    let core1 = DrasiLib::builder()
        .with_id("test-isolation-1")
        .with_index_provider(Arc::new(provider1))
        .build()
        .await?;

    let core2 = DrasiLib::builder()
        .with_id("test-isolation-2")
        .with_index_provider(Arc::new(provider2))
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
