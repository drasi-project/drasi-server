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
//! - DrasiServerBuilder with_default_index_provider method works correctly

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
        .with_default_index_provider(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME,
            Arc::new(provider),
        )
        .build()
        .await?;

    // Start and stop to verify basic operation
    core.start().await?;
    assert!(core.is_running().await);
    drasi_lib::wait_for_status(
        &core.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("component graph should reach Running");

    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}

/// Test DrasiServerBuilder with RocksDB index provider
#[tokio::test]
async fn test_drasi_server_builder_with_default_index_provider() -> Result<()> {
    use drasi_server::DrasiServerBuilder;

    let temp_dir = TempDir::new()?;
    let index_path = temp_dir.path().join("index");

    let provider = RocksDbIndexProvider::new(index_path, true, false);

    // Build using DrasiServerBuilder
    let core = DrasiServerBuilder::new()
        .with_id("test-server-persist")
        .with_default_index_provider(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME,
            Arc::new(provider),
        )
        .build_core()
        .await?;

    // Start and verify
    core.start().await?;
    assert!(core.is_running().await);
    drasi_lib::wait_for_status(
        &core.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("component graph should reach Running");

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
        .with_default_index_provider(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME,
            Arc::new(provider),
        )
        .with_query(query)
        .build()
        .await?;

    // Start to trigger index creation
    core.start().await?;
    drasi_lib::wait_for_status(
        &core.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("component graph should reach Running");

    core.stop().await?;

    // The query has no explicit storage_backend, so it is only backed by RocksDB
    // because the provider was registered as the default. When the query starts,
    // RocksDB materializes on-disk index storage under `index_path`, so a
    // non-empty index directory proves the default provider actually served the
    // query. We assert the directory is populated rather than checking for a
    // specific child path, to avoid coupling the test to drasi-index-rocksdb's
    // internal on-disk layout (e.g. the per-query subdirectory naming scheme).
    let index_entry_count = std::fs::read_dir(&index_path)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert!(
        index_entry_count > 0,
        "RocksDB index directory '{}' should be populated, proving the default \
         provider backed a query with no explicit storage_backend",
        index_path.display()
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
        .with_default_index_provider(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME,
            Arc::new(provider1),
        )
        .build()
        .await?;

    let core2 = DrasiLib::builder()
        .with_id("test-isolation-2")
        .with_default_index_provider(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME,
            Arc::new(provider2),
        )
        .build()
        .await?;

    // Both can start independently
    core1.start().await?;
    core2.start().await?;

    drasi_lib::wait_for_status(
        &core1.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("core1 component graph should reach Running");
    drasi_lib::wait_for_status(
        &core2.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("core2 component graph should reach Running");

    assert!(core1.is_running().await);
    assert!(core2.is_running().await);

    core1.stop().await?;
    core2.stop().await?;

    Ok(())
}

/// A per-query `storageBackend` override is honored even when the instance has a
/// RocksDB default provider: a query that inherits the default is persisted,
/// while a query that explicitly overrides to an in-memory backend is not.
#[tokio::test]
async fn test_per_query_storage_backend_override() -> Result<()> {
    use drasi_lib::{Query, StorageBackendRef, StorageBackendSpec};

    let temp_dir = TempDir::new()?;
    let index_path = temp_dir.path().join("drasi-index");
    let provider = RocksDbIndexProvider::new(index_path.clone(), true, false);

    // Inherits the instance default (RocksDB).
    let persisted = Query::cypher("persisted-query")
        .query("MATCH (n) RETURN n")
        .build();
    // Explicitly overrides back to in-memory, despite the RocksDB default.
    let volatile = Query::cypher("volatile-query")
        .query("MATCH (n) RETURN n")
        .with_storage_backend(StorageBackendRef::Inline(StorageBackendSpec::Memory {
            enable_archive: true,
        }))
        .build();

    let core = DrasiLib::builder()
        .with_id("test-per-query-override")
        .with_default_index_provider(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME,
            Arc::new(provider),
        )
        .with_query(persisted)
        .with_query(volatile)
        .build()
        .await?;

    core.start().await?;
    drasi_lib::wait_for_status(
        &core.component_graph(),
        "__component_graph__",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("component graph should reach Running");
    core.stop().await?;

    // RocksDB materializes one on-disk database per persisted query under
    // `index_path` (`{index_path}/{query_id}`). Exactly one entry must exist:
    // the inheriting query persisted, the explicit in-memory override did not.
    let entries: Vec<_> = std::fs::read_dir(&index_path)
        .map(|dir| dir.filter_map(|e| e.ok()).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert_eq!(
        entries.len(),
        1,
        "exactly one query (the default-backed one) should have persisted index \
         storage; the in-memory override must not persist. Found: {entries:?}"
    );

    Ok(())
}

/// A query that references the named `rocksdb` backend when no provider is
/// registered must fail query startup, rather than silently falling back to
/// in-memory indexes (the documented contract for unregistered named backends).
#[tokio::test]
async fn test_unregistered_named_backend_fails_query_startup() -> Result<()> {
    use drasi_lib::{Query, StorageBackendRef};

    let query = Query::cypher("needs-rocksdb")
        .query("MATCH (n) RETURN n")
        .with_storage_backend(StorageBackendRef::Named(
            drasi_server::index_provider::PERSISTENT_INDEX_PROVIDER_NAME.to_string(),
        ))
        .build();

    // No index provider registered at all.
    let build_result = DrasiLib::builder()
        .with_id("test-unregistered-backend")
        .with_query(query)
        .build()
        .await;

    let core = match build_result {
        // Rejected at build time — an acceptable failure.
        Err(_) => return Ok(()),
        Ok(core) => core,
    };

    // Otherwise the failure must surface at startup: either start() errors, or
    // the query never reaches Running (it must not silently use in-memory).
    if core.start().await.is_err() {
        return Ok(());
    }

    let reached_running = drasi_lib::wait_for_status(
        &core.component_graph(),
        "needs-rocksdb",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(3),
    )
    .await
    .is_ok();
    let _ = core.stop().await;

    assert!(
        !reached_running,
        "a query referencing the unregistered 'rocksdb' backend must fail query \
         startup, not silently fall back to in-memory indexes"
    );

    Ok(())
}

/// Cleanup guard that removes a `./data/<id>` directory created by the
/// create-instance handler when `persistIndex` is enabled.
struct DataDirGuard(std::path::PathBuf);

impl Drop for DataDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The `POST /instances` handler must honor `persistIndex: true` end-to-end: a
/// query added to the created instance is backed by RocksDB on disk. This guards
/// against a regression in the JSON `persistIndex` -> `persist_index` mapping,
/// which the builder-level tests above would not catch.
#[tokio::test]
async fn test_create_instance_persist_index_via_http() -> Result<()> {
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use drasi_lib::Query;
    use drasi_server::api::v1::handlers;
    use drasi_server::api::v1::routes::build_v1_router;
    use drasi_server::instance_registry::InstanceRegistry;
    use drasi_server::plugin_registry::PluginRegistry;
    use tower::ServiceExt;

    const INSTANCE_ID: &str = "http-persist-index-instance";
    // The server derives the on-disk directory from a filesystem-safe storage
    // key (`id-` followed by the hex encoding of the instance id), shared by the
    // index and WAL paths. Replicate that here to locate the index directory.
    let storage_key = {
        let mut key = String::from("id-");
        for byte in INSTANCE_ID.bytes() {
            key.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
            key.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
        }
        key
    };
    let data_dir = std::path::PathBuf::from(format!("./data/{storage_key}"));
    let _ = std::fs::remove_dir_all(&data_dir);
    let _guard = DataDirGuard(data_dir.clone());

    // Empty registry; retain a clone to reach the instance created by the handler.
    let registry = InstanceRegistry::new();
    let mut plugin_registry = PluginRegistry::new();
    drasi_server::register_core_plugins(&mut plugin_registry);

    let router = Router::new()
        .route("/health", axum::routing::get(handlers::health_check))
        .merge(build_v1_router(
            registry.clone(),
            Arc::new(false),
            None,
            Arc::new(tokio::sync::RwLock::new(plugin_registry)),
            None,
        ));

    // Create the instance with persistent indexing enabled.
    let body = serde_json::json!({ "id": INSTANCE_ID, "persistIndex": true });
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/instances")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body)?))?,
        )
        .await
        .expect("create-instance request should complete");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await?;
    assert_eq!(
        status,
        StatusCode::OK,
        "creating an instance with persistIndex:true should succeed, body: {}",
        String::from_utf8_lossy(&bytes)
    );

    // Add an auto-starting query to the (running) instance; it opens its RocksDB
    // index, materializing `./data/<id>/index/<query_id>`.
    let core = registry
        .get(INSTANCE_ID)
        .await
        .expect("created instance should be registered");
    core.add_query(
        Query::cypher("http-persist-query")
            .query("MATCH (n) RETURN n")
            .auto_start(true)
            .build(),
    )
    .await?;
    drasi_lib::wait_for_status(
        &core.component_graph(),
        "http-persist-query",
        &[drasi_lib::channels::ComponentStatus::Running],
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("query should reach Running");
    core.stop().await?;

    let index_path = data_dir.join("index");
    let populated = std::fs::read_dir(&index_path)
        .map(|dir| dir.count() > 0)
        .unwrap_or(false);
    assert!(
        populated,
        "persistIndex:true via HTTP should persist the query's index under {}",
        index_path.display()
    );

    Ok(())
}
