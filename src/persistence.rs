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
    core: Arc<drasi_server_core::DrasiServerCore>,
    api_host: String,
    api_port: u16,
    log_level: String,
    disable_persistence: bool,
}

impl ConfigPersistence {
    /// Create a new ConfigPersistence instance
    pub fn new(
        config_file_path: PathBuf,
        core: Arc<drasi_server_core::DrasiServerCore>,
        api_host: String,
        api_port: u16,
        log_level: String,
        disable_persistence: bool,
    ) -> Self {
        Self {
            config_file_path,
            core,
            api_host,
            api_port,
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
        let core_config = self.core.get_current_config().await.map_err(|e| {
            anyhow::anyhow!("Failed to get current config from DrasiServerCore: {}", e)
        })?;

        // Wrap Core config with wrapper settings
        let wrapper_config = DrasiServerConfig {
            api: crate::config::ApiSettings {
                host: self.api_host.clone(),
                port: self.api_port,
            },
            server: crate::config::ServerSettings {
                log_level: self.log_level.clone(),
                disable_persistence: self.disable_persistence,
            },
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
                "Failed to write temp config file {}: {}",
                temp_path.display(),
                e
            );
            anyhow::anyhow!("Failed to write temp config file: {}", e)
        })?;

        // Atomically rename temp file to actual config file
        std::fs::rename(&temp_path, &self.config_file_path).map_err(|e| {
            error!(
                "Failed to rename temp config file {} to {}: {}",
                temp_path.display(),
                self.config_file_path.display(),
                e
            );
            // Clean up temp file if rename fails
            let _ = std::fs::remove_file(&temp_path);
            anyhow::anyhow!("Failed to rename config file: {}", e)
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
    use drasi_server_core::config::{QueryConfig, QueryLanguage, SourceConfig};
    use std::collections::HashMap;
    use tempfile::TempDir;

    async fn create_test_core() -> Arc<drasi_server_core::DrasiServerCore> {
        let core = drasi_server_core::DrasiServerCore::builder()
            .with_id("test-server")
            .add_source(SourceConfig {
                id: "test-source".to_string(),
                source_type: "mock".to_string(),
                auto_start: false,
                properties: HashMap::new(),
                bootstrap_provider: None,
            })
            .add_query(QueryConfig {
                id: "test-query".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                query_language: QueryLanguage::default(),
                sources: vec!["test-source".to_string()],
                auto_start: false,
                properties: HashMap::new(),
                joins: None,
            })
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
            serde_yaml::from_str(&content).expect("Failed to parse saved config");

        // Verify wrapper settings
        assert_eq!(loaded_config.api.host, "127.0.0.1");
        assert_eq!(loaded_config.api.port, 8080);
        assert_eq!(loaded_config.server.log_level, "info");
        assert!(!loaded_config.server.disable_persistence);

        // Verify components
        assert_eq!(loaded_config.core_config.sources.len(), 1);
        assert_eq!(loaded_config.core_config.sources[0].id, "test-source");
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
        assert!(content.contains("api:"));
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
