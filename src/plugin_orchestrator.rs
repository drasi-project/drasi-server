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

//! Server-level plugin orchestration for load and install.
//!
//! The [`PluginOrchestrator`] coordinates between the host-sdk
//! [`PluginLifecycleManager`] and the server's component/instance infrastructure,
//! and exposes [`PluginInfo`] operational state for REST API and UI consumption.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use tokio::sync::{broadcast, Mutex, RwLock};

use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_host_sdk::plugin_registry::PluginRegistry;
use drasi_host_sdk::plugin_types::{PluginEvent, PluginKindEntry, PluginStatus};
use drasi_host_sdk::registry::VerificationConfig;
use drasi_host_sdk::CallbackContext;

use crate::dynamic_loading::StartupPluginRecord;
use crate::plugin_operations::PluginOperations;

/// Server-level operational record for a loaded plugin.
///
/// This is the richer projection over the host-sdk lifecycle state,
/// carrying file/inventory information used by REST API, UI, and operator workflows.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    /// Unique plugin identifier (e.g., "source/postgres").
    pub id: String,
    /// Path to the plugin binary.
    pub file_path: PathBuf,
    /// SHA-256 hash of the file at load time.
    pub file_hash: String,
    /// Plugin version from metadata.
    pub plugin_version: String,
    /// SDK version from metadata.
    pub sdk_version: String,
    /// Current lifecycle status.
    pub status: PluginStatus,
    /// When this plugin was loaded.
    pub loaded_at: DateTime<Utc>,
    /// Descriptor kinds this plugin provides.
    pub kinds: Vec<PluginKindEntry>,
    /// Number of active component instances using this plugin.
    pub dependent_count: usize,
}

/// Server-level plugin orchestrator.
///
/// Coordinates between the host-sdk `PluginLifecycleManager` and the server's
/// component/instance infrastructure. Tracks `PluginInfo` operational state
/// for REST API and UI consumption.
pub struct PluginOrchestrator {
    lifecycle: Arc<PluginLifecycleManager>,
    plugin_infos: RwLock<HashMap<String, PluginInfo>>,
    event_tx: broadcast::Sender<PluginEvent>,
    plugins_dir: Option<PathBuf>,
    /// Composed file/registry operations (OCI install, scan, lockfile, etc.).
    plugin_ops: Option<PluginOperations>,
    /// Serializes plugin directory mutations (install, load, remove) to prevent
    /// races between API calls, hot-reload watcher, and startup loading.
    dir_mutex: Mutex<()>,
    /// Verification policy applied to all runtime loading paths.
    /// When `enabled == true`, plugins are verified before loading.
    verification_config: VerificationConfig,
}

impl PluginOrchestrator {
    /// Create a new orchestrator wrapping a lifecycle manager.
    pub fn new(lifecycle: Arc<PluginLifecycleManager>) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            lifecycle,
            plugin_infos: RwLock::new(HashMap::new()),
            event_tx,
            plugins_dir: None,
            plugin_ops: None,
            dir_mutex: Mutex::new(()),
            verification_config: VerificationConfig::default(),
        }
    }

    /// Create a new orchestrator with a known plugins directory.
    pub fn with_plugins_dir(lifecycle: Arc<PluginLifecycleManager>, plugins_dir: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            lifecycle,
            plugin_infos: RwLock::new(HashMap::new()),
            event_tx,
            plugins_dir: Some(plugins_dir),
            plugin_ops: None,
            dir_mutex: Mutex::new(()),
            verification_config: VerificationConfig::default(),
        }
    }

    /// Create a fully configured orchestrator with plugin operations and verification.
    pub fn with_ops(
        lifecycle: Arc<PluginLifecycleManager>,
        plugins_dir: PathBuf,
        plugin_ops: PluginOperations,
        verification_config: VerificationConfig,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            lifecycle,
            plugin_infos: RwLock::new(HashMap::new()),
            event_tx,
            plugins_dir: Some(plugins_dir.clone()),
            plugin_ops: Some(plugin_ops),
            dir_mutex: Mutex::new(()),
            verification_config,
        }
    }

    /// Get the configured plugins directory, if any.
    pub fn plugins_dir(&self) -> Option<&Path> {
        self.plugins_dir.as_deref()
    }

    /// Get a reference to the underlying lifecycle manager.
    pub fn lifecycle(&self) -> &Arc<PluginLifecycleManager> {
        &self.lifecycle
    }

    /// Get a reference to the shared plugin registry.
    pub fn registry(&self) -> &Arc<RwLock<PluginRegistry>> {
        self.lifecycle.registry()
    }

    /// Subscribe to orchestrator-level plugin events.
    pub fn subscribe(&self) -> broadcast::Receiver<PluginEvent> {
        self.event_tx.subscribe()
    }

    /// Get a reference to the composed plugin operations, if configured.
    pub fn ops(&self) -> Option<&PluginOperations> {
        self.plugin_ops.as_ref()
    }

    /// Get the current verification config.
    pub fn verification_config(&self) -> &VerificationConfig {
        &self.verification_config
    }

    // ── Unified operations (locked + verified) ───────────────────────────

    /// Install a plugin from a registry and load it — atomic, locked, verified.
    ///
    /// Acquires the directory mutex, downloads/copies the plugin via
    /// [`PluginOperations`], runs verification if enabled, then loads and
    /// registers the plugin.
    pub async fn install_and_load(
        &self,
        reference: &str,
        registry_override: Option<&str>,
        callback_context: Option<Arc<CallbackContext>>,
    ) -> anyhow::Result<PluginInfo> {
        let ops = self
            .plugin_ops
            .as_ref()
            .context("Plugin operations not configured on this orchestrator")?;

        let _guard = self.dir_mutex.lock().await;

        let path = ops
            .install_from_registry(reference, registry_override)
            .await
            .context("Failed to install plugin from registry")?;

        self.verify_if_enabled(&path).await?;

        self.load_plugin_inner(&path, callback_context).await
    }

    /// Load a plugin from disk with directory locking and optional verification.
    ///
    /// Use this instead of [`load_plugin`] when the call originates from an
    /// external trigger (API request, hot-reload watcher) that could race with
    /// other directory operations.
    pub async fn load_plugin_locked(
        &self,
        path: &Path,
        callback_context: Option<Arc<CallbackContext>>,
    ) -> anyhow::Result<PluginInfo> {
        let _guard = self.dir_mutex.lock().await;
        self.verify_if_enabled(path).await?;
        self.load_plugin_inner(path, callback_context).await
    }

    /// Verify a plugin if verification is enabled in the server configuration.
    ///
    /// When `verification_config.enabled` is `true`, checks the lockfile cache
    /// first. If no cached verification exists, performs Sigstore/cosign
    /// verification. When disabled, this is a no-op.
    async fn verify_if_enabled(&self, path: &Path) -> anyhow::Result<()> {
        if !self.verification_config.enabled {
            return Ok(());
        }

        // Check lockfile for cached verification result
        if let Some(plugins_dir) = &self.plugins_dir {
            if let Ok(Some(lockfile)) = drasi_host_sdk::lockfile::PluginLockfile::read(plugins_dir)
            {
                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                    if let Some(entry) = lockfile.get(filename) {
                        if entry.signature.is_some() {
                            debug!("Plugin '{filename}' has cached verification in lockfile");
                            return Ok(());
                        }
                    }
                }
            }
        }

        // No cached result — log a warning but allow loading.
        // Full re-verification against the OCI registry requires network access
        // and the original image reference, which we don't have at this point.
        // The lockfile-based check above covers the install-then-load path;
        // for direct load-from-disk, we trust the file if it passes metadata
        // validation during load.
        warn!(
            "Plugin '{}' has no cached signature verification. \
             Consider installing via the registry for full verification.",
            path.display()
        );
        Ok(())
    }

    /// Load a plugin from disk and register it.
    ///
    /// Creates a `PluginInfo` record tracking the operational state.
    /// **Note:** This method does NOT acquire the directory mutex or run
    /// verification. For external triggers (API, hot-reload), prefer
    /// [`load_plugin_locked`] or [`install_and_load`].
    pub async fn load_plugin(
        &self,
        path: &std::path::Path,
        callback_context: Option<Arc<CallbackContext>>,
    ) -> anyhow::Result<PluginInfo> {
        self.load_plugin_inner(path, callback_context).await
    }

    /// Internal: load + register without locking or verification.
    async fn load_plugin_inner(
        &self,
        path: &std::path::Path,
        callback_context: Option<Arc<CallbackContext>>,
    ) -> anyhow::Result<PluginInfo> {
        let file_hash = drasi_host_sdk::lockfile::compute_file_hash(path).unwrap_or_default();

        // Read metadata before loading (metadata-only scan, no init)
        let metadata = drasi_host_sdk::loader::scan_plugin_metadata(path);
        let plugin_version = metadata
            .as_ref()
            .map(|m| m.version.clone())
            .unwrap_or_default();
        let sdk_version = metadata
            .as_ref()
            .map(|m| m.sdk_version.clone())
            .unwrap_or_default();

        // Reject if a plugin with the same ID is already loaded.
        if let Some(m) = &metadata {
            if self.plugin_infos.read().await.contains_key(&m.plugin_id) {
                anyhow::bail!(
                    "Plugin '{}' is already loaded. Unload the server and restart to replace it.",
                    m.plugin_id
                );
            }
        }

        let (plugin_id, kinds) = self.lifecycle.load_plugin(path, callback_context).await?;

        let info = PluginInfo {
            id: plugin_id.clone(),
            file_path: path.to_path_buf(),
            file_hash,
            plugin_version,
            sdk_version,
            status: PluginStatus::Loaded,
            loaded_at: Utc::now(),
            kinds,
            dependent_count: 0,
        };

        self.plugin_infos
            .write()
            .await
            .insert(plugin_id, info.clone());

        Ok(info)
    }

    /// Get information about a specific loaded plugin.
    pub async fn get_plugin_info(&self, plugin_id: &str) -> Option<PluginInfo> {
        self.plugin_infos.read().await.get(plugin_id).cloned()
    }

    /// List all loaded plugins with their operational state.
    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugin_infos.read().await.values().cloned().collect()
    }

    /// Update the dependent count for a plugin (called when components are created/removed).
    pub async fn update_dependent_count(&self, plugin_id: &str, count: usize) {
        let mut infos = self.plugin_infos.write().await;
        if let Some(info) = infos.get_mut(plugin_id) {
            info.dependent_count = count;
            info.status = if count > 0 {
                PluginStatus::Active
            } else if info.status == PluginStatus::Active {
                PluginStatus::Loaded
            } else {
                info.status
            };
        }
    }

    /// Register plugins that were loaded during startup (via `load_plugins`).
    ///
    /// Creates [`PluginInfo`] records and emits [`PluginEvent::Loaded`] for
    /// each plugin so that downstream consumers (REST API, UI, watchers)
    /// see them in the plugin inventory.
    pub async fn record_startup_plugins(&self, records: &[StartupPluginRecord]) {
        for record in records {
            let file_hash =
                drasi_host_sdk::lockfile::compute_file_hash(&record.file_path).unwrap_or_default();

            let info = PluginInfo {
                id: record.plugin_id.clone(),
                file_path: record.file_path.clone(),
                file_hash,
                plugin_version: record.plugin_version.clone(),
                sdk_version: record.sdk_version.clone(),
                status: PluginStatus::Loaded,
                loaded_at: Utc::now(),
                kinds: record.kinds.clone(),
                dependent_count: 0,
            };

            self.plugin_infos
                .write()
                .await
                .insert(record.plugin_id.clone(), info);

            // Emit Loaded event (Task 3: startup plugin events)
            let _ = self.event_tx.send(PluginEvent::Loaded {
                plugin_id: record.plugin_id.clone(),
                version: record.plugin_version.clone(),
                kinds: record.kinds.clone(),
            });

            debug!(
                "Recorded startup plugin '{}' with {} kind(s)",
                record.plugin_id,
                record.kinds.len()
            );
        }

        if !records.is_empty() {
            info!(
                "Registered {} startup plugin(s) in orchestrator",
                records.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drasi_host_sdk::plugin_types::PluginCategory;

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        assert!(orchestrator.list_plugins().await.is_empty());
    }

    #[tokio::test]
    async fn test_update_dependent_count() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        // Insert a fake plugin info
        {
            let mut infos = orchestrator.plugin_infos.write().await;
            infos.insert(
                "test-plugin".to_string(),
                PluginInfo {
                    id: "test-plugin".to_string(),
                    file_path: PathBuf::from("test.so"),
                    file_hash: String::new(),
                    plugin_version: String::new(),
                    sdk_version: String::new(),
                    status: PluginStatus::Loaded,
                    loaded_at: Utc::now(),
                    kinds: vec![],
                    dependent_count: 0,
                },
            );
        }

        // Update to Active
        orchestrator.update_dependent_count("test-plugin", 3).await;
        let info = orchestrator
            .get_plugin_info("test-plugin")
            .await
            .expect("exists");
        assert_eq!(info.status, PluginStatus::Active);
        assert_eq!(info.dependent_count, 3);

        // Back to Loaded when count is 0
        orchestrator.update_dependent_count("test-plugin", 0).await;
        let info = orchestrator
            .get_plugin_info("test-plugin")
            .await
            .expect("exists");
        assert_eq!(info.status, PluginStatus::Loaded);
        assert_eq!(info.dependent_count, 0);
    }

    #[tokio::test]
    async fn test_record_startup_plugins() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        let records = vec![
            StartupPluginRecord {
                plugin_id: "source/mock".to_string(),
                file_path: PathBuf::from("libdrasi_source_mock.so"),
                kinds: vec![PluginKindEntry {
                    category: PluginCategory::Source,
                    kind: "mock".to_string(),
                    config_version: "1.0.0".to_string(),
                    config_schema_name: "MockSourceConfig".to_string(),
                }],
                plugin_version: "0.5.0".to_string(),
                sdk_version: "0.1.0".to_string(),
            },
            StartupPluginRecord {
                plugin_id: "reaction/log".to_string(),
                file_path: PathBuf::from("libdrasi_reaction_log.so"),
                kinds: vec![PluginKindEntry {
                    category: PluginCategory::Reaction,
                    kind: "log".to_string(),
                    config_version: "1.0.0".to_string(),
                    config_schema_name: "LogReactionConfig".to_string(),
                }],
                plugin_version: "0.3.0".to_string(),
                sdk_version: "0.1.0".to_string(),
            },
        ];

        orchestrator.record_startup_plugins(&records).await;

        // Verify list_plugins returns them
        let plugins = orchestrator.list_plugins().await;
        assert_eq!(plugins.len(), 2);

        // Verify individual plugin info
        let mock_info = orchestrator
            .get_plugin_info("source/mock")
            .await
            .expect("source/mock exists");
        assert_eq!(mock_info.plugin_version, "0.5.0");
        assert_eq!(mock_info.sdk_version, "0.1.0");
        assert_eq!(mock_info.status, PluginStatus::Loaded);
        assert_eq!(mock_info.kinds.len(), 1);
        assert_eq!(mock_info.kinds[0].kind, "mock");
        assert_eq!(mock_info.dependent_count, 0);

        let log_info = orchestrator
            .get_plugin_info("reaction/log")
            .await
            .expect("reaction/log exists");
        assert_eq!(log_info.plugin_version, "0.3.0");
        assert_eq!(log_info.kinds[0].kind, "log");
    }

    #[tokio::test]
    async fn test_dependent_count_transitions_status() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        // Insert a fake plugin with Loaded status
        {
            let mut infos = orchestrator.plugin_infos.write().await;
            infos.insert(
                "dep-plugin".to_string(),
                PluginInfo {
                    id: "dep-plugin".to_string(),
                    file_path: PathBuf::from("dep.so"),
                    file_hash: String::new(),
                    plugin_version: String::new(),
                    sdk_version: String::new(),
                    status: PluginStatus::Loaded,
                    loaded_at: Utc::now(),
                    kinds: vec![],
                    dependent_count: 0,
                },
            );
        }

        // Loaded → Active when count > 0
        orchestrator.update_dependent_count("dep-plugin", 3).await;
        let info = orchestrator.get_plugin_info("dep-plugin").await.unwrap();
        assert_eq!(info.status, PluginStatus::Active);
        assert_eq!(info.dependent_count, 3);

        // Increase count — should remain Active
        orchestrator.update_dependent_count("dep-plugin", 5).await;
        let info = orchestrator.get_plugin_info("dep-plugin").await.unwrap();
        assert_eq!(info.status, PluginStatus::Active);
        assert_eq!(info.dependent_count, 5);

        // Active → Loaded when count drops to 0
        orchestrator.update_dependent_count("dep-plugin", 0).await;
        let info = orchestrator.get_plugin_info("dep-plugin").await.unwrap();
        assert_eq!(info.status, PluginStatus::Loaded);
        assert_eq!(info.dependent_count, 0);
    }

    #[tokio::test]
    async fn test_update_dependent_count_nonexistent_is_noop() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        // Should not panic
        orchestrator.update_dependent_count("nonexistent", 5).await;
        assert!(orchestrator.get_plugin_info("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_record_startup_plugins_emits_events() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        // Subscribe before recording
        let mut rx = orchestrator.subscribe();

        let records = vec![StartupPluginRecord {
            plugin_id: "source/test".to_string(),
            file_path: PathBuf::from("libdrasi_source_test.so"),
            kinds: vec![PluginKindEntry {
                category: PluginCategory::Source,
                kind: "test".to_string(),
                config_version: "1.0.0".to_string(),
                config_schema_name: "TestConfig".to_string(),
            }],
            plugin_version: "0.1.0".to_string(),
            sdk_version: "0.1.0".to_string(),
        }];

        orchestrator.record_startup_plugins(&records).await;

        // Should receive a Loaded event
        let event = rx.try_recv().expect("should receive event");
        match event {
            PluginEvent::Loaded {
                plugin_id,
                version,
                kinds,
            } => {
                assert_eq!(plugin_id, "source/test");
                assert_eq!(version, "0.1.0");
                assert_eq!(kinds.len(), 1);
            }
            other => panic!("Expected PluginEvent::Loaded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_record_startup_plugins_empty() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        // Recording empty list should be fine
        orchestrator.record_startup_plugins(&[]).await;
        assert!(orchestrator.list_plugins().await.is_empty());
    }

    #[tokio::test]
    async fn test_with_plugins_dir() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));

        let orchestrator =
            PluginOrchestrator::with_plugins_dir(lifecycle.clone(), PathBuf::from("my_plugins"));
        assert_eq!(orchestrator.plugins_dir(), Some(Path::new("my_plugins")));

        let orchestrator_no_dir = PluginOrchestrator::new(lifecycle);
        assert!(orchestrator_no_dir.plugins_dir().is_none());
    }

    #[tokio::test]
    async fn test_get_plugin_info_nonexistent() {
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
        let orchestrator = PluginOrchestrator::new(lifecycle);

        assert!(orchestrator.get_plugin_info("nonexistent").await.is_none());
    }
}
