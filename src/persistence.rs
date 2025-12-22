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

use crate::config::DrasiServerConfig;
use anyhow::Result;
use log::{debug, error, info};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Handles persistence of DrasiServerConfig to a YAML file.
/// Uses atomic writes (temp file + rename) to prevent corruption.
pub struct ConfigPersistence {
    config_file_path: PathBuf,
    core: Arc<drasi_lib::DrasiLib>,
    host: String,
    port: u16,
    log_level: String,
    disable_persistence: bool,
}

impl ConfigPersistence {
    /// Create a new ConfigPersistence instance
    pub fn new(
        config_file_path: PathBuf,
        core: Arc<drasi_lib::DrasiLib>,
        host: String,
        port: u16,
        log_level: String,
        disable_persistence: bool,
    ) -> Self {
        Self {
            config_file_path,
            core,
            host,
            port,
            log_level,
            disable_persistence,
        }
    }

    /// Save the current configuration to the config file using atomic writes.
    /// Uses Core's public API to get current configuration snapshot.
    pub async fn save(&self) -> Result<()> {
        if self.disable_persistence {
            debug!("Persistence disabled, skipping save");
            return Ok(());
        }

        info!(
            "Saving configuration to {}",
            self.config_file_path.display()
        );

        // Get current configuration from Core using public API
        let core_config = self
            .core
            .get_current_config()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get current config from DrasiLib: {e}"))?;

        // Wrap Core config with wrapper settings
        // Note: sources and reactions are empty here because they are owned by the core
        // and we don't have access to the original config enums. The core manages them
        // dynamically through the builder pattern or API.
        let wrapper_config = DrasiServerConfig {
            host: crate::api::models::ConfigValue::Static(self.host.clone()),
            port: crate::api::models::ConfigValue::Static(self.port),
            log_level: crate::api::models::ConfigValue::Static(self.log_level.clone()),
            disable_persistence: self.disable_persistence,
            sources: Vec::new(),
            reactions: Vec::new(),
            core_config,
        };

        // Validate before saving
        wrapper_config.validate()?;

        // Use atomic write: write to temp file, then rename
        let temp_path = self.config_file_path.with_extension("tmp");

        // Serialize to YAML
        let yaml_content = serde_yaml::to_string(&wrapper_config)?;

        // Write to temp file
        std::fs::write(&temp_path, yaml_content).map_err(|e| {
            error!(
                "Failed to write temp config file {}: {e}",
                temp_path.display()
            );
            anyhow::anyhow!("Failed to write temp config file: {e}")
        })?;

        // Atomically rename temp file to actual config file
        std::fs::rename(&temp_path, &self.config_file_path).map_err(|e| {
            error!(
                "Failed to rename temp config file {} to {}: {e}",
                temp_path.display(),
                self.config_file_path.display()
            );
            // Clean up temp file if rename fails
            let _ = std::fs::remove_file(&temp_path);
            anyhow::anyhow!("Failed to rename config file: {e}")
        })?;

        info!(
            "Configuration saved successfully to {}",
            self.config_file_path.display()
        );
        Ok(())
    }

    /// Check if the config file is writable
    pub fn is_writable(&self) -> bool {
        Self::check_write_access(&self.config_file_path)
    }

    /// Check if we have write access to a file
    fn check_write_access(path: &Path) -> bool {
        use std::fs::OpenOptions;
        OpenOptions::new().append(true).open(path).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use drasi_lib::channels::dispatcher::ChangeDispatcher;
    use drasi_lib::channels::{ComponentEventSender, ComponentStatus, SubscriptionResponse};
    use drasi_lib::plugin_core::Source as SourceTrait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    // Mock source for testing
    struct MockSource {
        id: String,
        status: Arc<RwLock<ComponentStatus>>,
    }

    impl MockSource {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                status: Arc::new(RwLock::new(ComponentStatus::Stopped)),
            }
        }
    }

    #[async_trait]
    impl SourceTrait for MockSource {
        fn id(&self) -> &str {
            &self.id
        }

        fn type_name(&self) -> &str {
            "mock"
        }

        fn properties(&self) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }

        async fn start(&self) -> anyhow::Result<()> {
            *self.status.write().await = ComponentStatus::Running;
            Ok(())
        }

        async fn stop(&self) -> anyhow::Result<()> {
            *self.status.write().await = ComponentStatus::Stopped;
            Ok(())
        }

        async fn status(&self) -> ComponentStatus {
            self.status.read().await.clone()
        }

        async fn subscribe(
            &self,
            settings: drasi_lib::config::SourceSubscriptionSettings,
        ) -> anyhow::Result<SubscriptionResponse> {
            use drasi_lib::channels::dispatcher::ChannelChangeDispatcher;
            let dispatcher =
                ChannelChangeDispatcher::<drasi_lib::channels::SourceEventWrapper>::new(100);
            let receiver = dispatcher.create_receiver().await?;
            Ok(SubscriptionResponse {
                query_id: settings.query_id,
                source_id: self.id.clone(),
                receiver,
                bootstrap_receiver: None,
            })
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        async fn inject_event_tx(&self, _tx: ComponentEventSender) {
            // No-op for testing
        }
    }

    async fn create_test_core() -> Arc<drasi_lib::DrasiLib> {
        use drasi_lib::Query;

        let source = MockSource::new("test-source");

        let core = drasi_lib::DrasiLib::builder()
            .with_id("test-server")
            .with_source(source)
            .with_query(
                Query::cypher("test-query")
                    .query("MATCH (n) RETURN n")
                    .from_source("test-source")
                    .auto_start(false)
                    .build(),
            )
            .build()
            .await
            .expect("Failed to build test core");

        Arc::new(core)
    }

    #[tokio::test]
    async fn test_persistence_saves_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create a test file
        std::fs::write(&config_path, "").expect("Failed to create test file");

        let core = create_test_core().await;

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            core,
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false,
        );

        // Save should succeed
        persistence.save().await.expect("Save failed");

        // Verify file was written
        assert!(config_path.exists());

        // Verify content is valid YAML
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        // Verify wrapper settings
        assert_eq!(
            loaded_config.host,
            crate::api::models::ConfigValue::Static("127.0.0.1".to_string())
        );
        assert_eq!(
            loaded_config.port,
            crate::api::models::ConfigValue::Static(8080)
        );
        assert_eq!(
            loaded_config.log_level,
            crate::api::models::ConfigValue::Static("info".to_string())
        );
        assert!(!loaded_config.disable_persistence);

        // Verify queries (sources are created dynamically via registry, not in config)
        assert_eq!(loaded_config.core_config.queries.len(), 1);
        assert_eq!(loaded_config.core_config.queries[0].id, "test-query");
    }

    #[tokio::test]
    async fn test_persistence_skips_when_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        let core = create_test_core().await;

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            core,
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // disable_persistence = true
        );

        // Save should succeed but not write anything
        persistence.save().await.expect("Save failed");

        // File should not exist
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn test_persistence_atomic_write() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create initial file with some content
        std::fs::write(&config_path, "initial content").expect("Failed to create initial file");

        let core = create_test_core().await;

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            core,
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false,
        );

        // Save should succeed
        persistence.save().await.expect("Save failed");

        // Verify temp file doesn't exist (was renamed)
        let temp_path = config_path.with_extension("tmp");
        assert!(!temp_path.exists());

        // Verify main file exists with valid content
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        assert!(content.contains("host:"));
        assert!(!content.contains("initial content"));
    }

    #[tokio::test]
    async fn test_is_writable() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create a writable file
        std::fs::write(&config_path, "test").expect("Failed to create test file");

        let core = create_test_core().await;

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            core,
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false,
        );

        // Should be writable
        assert!(persistence.is_writable());

        // Test non-existent file
        let non_existent = temp_dir.path().join("does-not-exist.yaml");
        let persistence_non_existent = ConfigPersistence::new(
            non_existent,
            create_test_core().await,
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false,
        );

        // Should not be writable
        assert!(!persistence_non_existent.is_writable());
    }
}
