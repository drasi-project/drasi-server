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

use crate::api::models::{ConfigValue, QueryConfigDto};
use crate::config::{DrasiLibInstanceConfig, DrasiServerConfig, ReactionConfig, SourceConfig};
use crate::instance_registry::InstanceRegistry;
use anyhow::Result;
use indexmap::IndexMap;
use log::{debug, error, info};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handles persistence of DrasiServerConfig to a YAML file.
/// Uses atomic writes (temp file + rename) to prevent corruption.
/// Stores source, reaction, and query configs in memory for persistence since
/// they cannot be retrieved from running plugin instances or drasi-lib.
pub struct ConfigPersistence {
    config_file_path: PathBuf,
    registry: InstanceRegistry,
    host: String,
    port: u16,
    log_level: String,
    persist_config: bool,
    persist_settings: IndexMap<String, bool>,
    /// Source configs by instance_id -> source_id -> config
    source_configs: Arc<RwLock<IndexMap<String, IndexMap<String, SourceConfig>>>>,
    /// Reaction configs by instance_id -> reaction_id -> config
    reaction_configs: Arc<RwLock<IndexMap<String, IndexMap<String, ReactionConfig>>>>,
    /// Query configs by instance_id -> query_id -> config
    query_configs: Arc<RwLock<IndexMap<String, IndexMap<String, QueryConfigDto>>>>,
    /// Instance configs for dynamic instances
    instance_configs: Arc<RwLock<IndexMap<String, DrasiLibInstanceConfig>>>,
}

impl ConfigPersistence {
    /// Create a new ConfigPersistence instance
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config_file_path: PathBuf,
        registry: InstanceRegistry,
        host: String,
        port: u16,
        log_level: String,
        persist_config: bool,
        persist_settings: IndexMap<String, bool>,
        initial_source_configs: IndexMap<String, IndexMap<String, SourceConfig>>,
        initial_reaction_configs: IndexMap<String, IndexMap<String, ReactionConfig>>,
        initial_query_configs: IndexMap<String, IndexMap<String, QueryConfigDto>>,
    ) -> Self {
        Self {
            config_file_path,
            registry,
            host,
            port,
            log_level,
            persist_config,
            persist_settings,
            source_configs: Arc::new(RwLock::new(initial_source_configs)),
            reaction_configs: Arc::new(RwLock::new(initial_reaction_configs)),
            query_configs: Arc::new(RwLock::new(initial_query_configs)),
            instance_configs: Arc::new(RwLock::new(IndexMap::new())),
        }
    }

    /// Register a new instance config for persistence
    pub async fn register_instance(&self, config: DrasiLibInstanceConfig) {
        if !self.persist_config {
            return;
        }
        let mut instance_configs = self.instance_configs.write().await;
        // Extract the ID from the ConfigValue
        let id = match &config.id {
            crate::api::models::ConfigValue::Static(s) => s.clone(),
            crate::api::models::ConfigValue::EnvironmentVariable { name, default } => {
                std::env::var(name).unwrap_or_else(|_| default.clone().unwrap_or_default())
            }
            crate::api::models::ConfigValue::Secret { name } => name.clone(),
        };
        instance_configs.insert(id, config);
    }

    /// Register a source config for persistence
    pub async fn register_source(&self, instance_id: &str, config: SourceConfig) {
        if !self.persist_config {
            return;
        }
        let mut source_configs = self.source_configs.write().await;
        source_configs
            .entry(instance_id.to_string())
            .or_default()
            .insert(config.id().to_string(), config);
    }

    /// Unregister a source config (called on deletion)
    pub async fn unregister_source(&self, instance_id: &str, source_id: &str) {
        if !self.persist_config {
            return;
        }
        let mut source_configs = self.source_configs.write().await;
        if let Some(instance_sources) = source_configs.get_mut(instance_id) {
            instance_sources.swap_remove(source_id);
        }
    }

    /// Register a reaction config for persistence
    pub async fn register_reaction(&self, instance_id: &str, config: ReactionConfig) {
        if !self.persist_config {
            return;
        }
        let mut reaction_configs = self.reaction_configs.write().await;
        reaction_configs
            .entry(instance_id.to_string())
            .or_default()
            .insert(config.id().to_string(), config);
    }

    /// Unregister a reaction config (called on deletion)
    pub async fn unregister_reaction(&self, instance_id: &str, reaction_id: &str) {
        if !self.persist_config {
            return;
        }
        let mut reaction_configs = self.reaction_configs.write().await;
        if let Some(instance_reactions) = reaction_configs.get_mut(instance_id) {
            instance_reactions.swap_remove(reaction_id);
        }
    }

    /// Register a query config for persistence
    pub async fn register_query(&self, instance_id: &str, config: QueryConfigDto) {
        if !self.persist_config {
            return;
        }
        let mut query_configs = self.query_configs.write().await;
        query_configs
            .entry(instance_id.to_string())
            .or_default()
            .insert(config.id.clone(), config);
    }

    /// Get a stored source config, if available
    pub async fn get_source_config(&self, instance_id: &str, source_id: &str) -> Option<SourceConfig> {
        let source_configs = self.source_configs.read().await;
        source_configs
            .get(instance_id)
            .and_then(|configs| configs.get(source_id).cloned())
    }

    /// Get a stored reaction config, if available
    pub async fn get_reaction_config(
        &self,
        instance_id: &str,
        reaction_id: &str,
    ) -> Option<ReactionConfig> {
        let reaction_configs = self.reaction_configs.read().await;
        reaction_configs
            .get(instance_id)
            .and_then(|configs| configs.get(reaction_id).cloned())
    }

    /// Get a stored query config, if available
    pub async fn get_query_config(
        &self,
        instance_id: &str,
        query_id: &str,
    ) -> Option<QueryConfigDto> {
        let query_configs = self.query_configs.read().await;
        query_configs
            .get(instance_id)
            .and_then(|configs| configs.get(query_id).cloned())
    }

    /// Unregister a query config (called on deletion)
    pub async fn unregister_query(&self, instance_id: &str, query_id: &str) {
        if !self.persist_config {
            return;
        }
        let mut query_configs = self.query_configs.write().await;
        if let Some(instance_queries) = query_configs.get_mut(instance_id) {
            instance_queries.swap_remove(query_id);
        }
    }

    /// Save the current configuration to the config file using atomic writes.
    /// Uses Core's public API to get current configuration snapshot.
    /// Includes source and reaction configs from the in-memory registry.
    /// Uses single-instance format when there's 1 instance, multi-instance format otherwise.
    pub async fn save(&self) -> Result<()> {
        if !self.persist_config {
            debug!("Persistence disabled (persist_config: false), skipping save");
            return Ok(());
        }

        info!(
            "Saving configuration to {}",
            self.config_file_path.display()
        );

        // Get stored source, reaction, and query configs
        let source_configs = self.source_configs.read().await;
        let reaction_configs = self.reaction_configs.read().await;
        let query_configs = self.query_configs.read().await;
        let dynamic_instance_configs = self.instance_configs.read().await;

        let mut instance_configs = Vec::new();

        for (id, core) in self.registry.list().await {
            let lib_config = core.get_current_config().await.map_err(|e| {
                anyhow::anyhow!("Failed to get current config from DrasiLib '{id}': {e}")
            })?;

            let persist_index = *self.persist_settings.get(&id).unwrap_or(&false);

            // Get source, reaction, and query configs for this instance from our DTO storage
            let sources: Vec<SourceConfig> = source_configs
                .get(&id)
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default();
            let reactions: Vec<ReactionConfig> = reaction_configs
                .get(&id)
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default();
            let queries: Vec<QueryConfigDto> = query_configs
                .get(&id)
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default();

            // Check if this is a dynamically created instance
            let instance_config = if let Some(dynamic_config) = dynamic_instance_configs.get(&id) {
                // Use stored config for dynamic instances
                DrasiLibInstanceConfig {
                    id: ConfigValue::Static(lib_config.id.clone()),
                    persist_index: dynamic_config.persist_index,
                    state_store: dynamic_config.state_store.clone(),
                    default_priority_queue_capacity: lib_config
                        .priority_queue_capacity
                        .map(ConfigValue::Static),
                    default_dispatch_buffer_capacity: lib_config
                        .dispatch_buffer_capacity
                        .map(ConfigValue::Static),
                    sources,
                    reactions,
                    queries,
                }
            } else {
                DrasiLibInstanceConfig {
                    id: ConfigValue::Static(lib_config.id.clone()),
                    persist_index,
                    state_store: None, // State store config not persisted dynamically
                    default_priority_queue_capacity: lib_config
                        .priority_queue_capacity
                        .map(ConfigValue::Static),
                    default_dispatch_buffer_capacity: lib_config
                        .dispatch_buffer_capacity
                        .map(ConfigValue::Static),
                    sources,
                    reactions,
                    queries, // Now using stored QueryConfigDto instead of empty vec
                }
            };
            instance_configs.push(instance_config);
        }

        // Dynamic format selection based on instance count
        let wrapper_config = if instance_configs.len() == 1 {
            // Single instance → use single-instance format (root-level fields)
            let instance = instance_configs.remove(0);
            DrasiServerConfig {
                id: instance.id,
                host: ConfigValue::Static(self.host.clone()),
                port: ConfigValue::Static(self.port),
                log_level: ConfigValue::Static(self.log_level.clone()),
                persist_config: self.persist_config,
                persist_index: instance.persist_index,
                state_store: instance.state_store,
                default_priority_queue_capacity: instance.default_priority_queue_capacity,
                default_dispatch_buffer_capacity: instance.default_dispatch_buffer_capacity,
                sources: instance.sources,
                reactions: instance.reactions,
                queries: instance.queries,
                instances: Vec::new(), // Empty = single-instance format
            }
        } else {
            // Multiple instances → use multi-instance format (instances array)
            let first_id = instance_configs
                .first()
                .and_then(|cfg| match &cfg.id {
                    ConfigValue::Static(id) => Some(id.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            DrasiServerConfig {
                id: ConfigValue::Static(first_id),
                host: ConfigValue::Static(self.host.clone()),
                port: ConfigValue::Static(self.port),
                log_level: ConfigValue::Static(self.log_level.clone()),
                persist_config: self.persist_config,
                persist_index: false, // Per-instance setting in multi-instance mode
                state_store: None,    // Per-instance setting in multi-instance mode
                default_priority_queue_capacity: None,
                default_dispatch_buffer_capacity: None,
                sources: Vec::new(),
                reactions: Vec::new(),
                queries: Vec::new(),
                instances: instance_configs,
            }
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
    use crate::api::models::sources::mock::DataTypeDto;
    use async_trait::async_trait;
    use drasi_lib::channels::dispatcher::ChangeDispatcher;
    use drasi_lib::channels::{ComponentStatus, SubscriptionResponse};
    use drasi_lib::Source as SourceTrait;
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

        async fn initialize(&self, _context: drasi_lib::SourceRuntimeContext) {
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
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
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
            ConfigValue::Static("127.0.0.1".to_string())
        );
        assert_eq!(loaded_config.port, ConfigValue::Static(8080));
        assert_eq!(
            loaded_config.log_level,
            ConfigValue::Static("info".to_string())
        );
        assert!(loaded_config.persist_config);

        // With single instance, dynamic format selection outputs single-instance format
        // (instances array empty)
        // Note: Queries are only persisted if they were registered via register_query()
        // Since this test doesn't register any queries, we expect an empty queries array
        assert!(
            loaded_config.instances.is_empty(),
            "Expected empty instances array for single-instance format"
        );
        assert_eq!(
            loaded_config.queries.len(),
            0,
            "Expected no queries since none were registered"
        );
    }

    #[tokio::test]
    async fn test_persistence_skips_when_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false, // persist_config = false (persistence disabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
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
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
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
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings.clone(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Should be writable
        assert!(persistence.is_writable());

        // Test non-existent file
        let non_existent = temp_dir.path().join("does-not-exist.yaml");
        let mut missing_instances = IndexMap::new();
        missing_instances.insert("test-server".to_string(), create_test_core().await);
        let persistence_non_existent = ConfigPersistence::new(
            non_existent,
            InstanceRegistry::from_map(missing_instances),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Should not be writable
        assert!(!persistence_non_existent.is_writable());
    }

    // ==================== Multi-Instance Format Tests ====================

    async fn create_test_core_with_id(id: &str) -> Arc<drasi_lib::DrasiLib> {
        use drasi_lib::Query;

        let source = MockSource::new(&format!("{id}-source"));

        let core = drasi_lib::DrasiLib::builder()
            .with_id(id)
            .with_source(source)
            .with_query(
                Query::cypher(format!("{id}-query"))
                    .query("MATCH (n) RETURN n")
                    .from_source(format!("{id}-source"))
                    .auto_start(false)
                    .build(),
            )
            .build()
            .await
            .expect("Failed to build test core");

        Arc::new(core)
    }

    #[tokio::test]
    async fn test_multi_instance_format_persistence() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create a test file
        std::fs::write(&config_path, "").expect("Failed to create test file");

        // Create two instances
        let core1 = create_test_core_with_id("instance-1").await;
        let core2 = create_test_core_with_id("instance-2").await;

        let mut instances_map = IndexMap::new();
        instances_map.insert("instance-1".to_string(), core1);
        instances_map.insert("instance-2".to_string(), core2);

        let mut persist_settings = IndexMap::new();
        persist_settings.insert("instance-1".to_string(), false);
        persist_settings.insert("instance-2".to_string(), true);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "0.0.0.0".to_string(),
            8080,
            "debug".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Save should succeed
        persistence.save().await.expect("Save failed");

        // Verify file was written
        assert!(config_path.exists());

        // Verify content is valid YAML with multi-instance format
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        // With multiple instances, should use multi-instance format
        assert_eq!(
            loaded_config.instances.len(),
            2,
            "Expected 2 instances in multi-instance format"
        );

        // Root-level arrays should be empty in multi-instance format
        assert!(
            loaded_config.sources.is_empty(),
            "Expected empty sources at root level"
        );
        assert!(
            loaded_config.queries.is_empty(),
            "Expected empty queries at root level"
        );
        assert!(
            loaded_config.reactions.is_empty(),
            "Expected empty reactions at root level"
        );

        // Verify instances exist but have no queries (queries only saved when registered)
        let instance1 = loaded_config
            .instances
            .iter()
            .find(|i| match &i.id {
                ConfigValue::Static(id) => id == "instance-1",
                _ => false,
            })
            .expect("instance-1 not found");
        // Queries are only saved when registered, not from DrasiLib instances
        assert_eq!(
            instance1.queries.len(),
            0,
            "No queries should be saved (not registered)"
        );

        let instance2 = loaded_config
            .instances
            .iter()
            .find(|i| match &i.id {
                ConfigValue::Static(id) => id == "instance-2",
                _ => false,
            })
            .expect("instance-2 not found");
        // Queries are only saved when registered, not from DrasiLib instances
        assert_eq!(
            instance2.queries.len(),
            0,
            "No queries should be saved (not registered)"
        );
        assert!(
            instance2.persist_index,
            "instance-2 should have persist_index=true"
        );
    }

    // ==================== Config Loading Tests ====================

    #[test]
    fn test_load_single_instance_config_format() {
        let config_yaml = r#"
id: my-server
host: localhost
port: 9090
logLevel: info
persistConfig: true
persistIndex: true
sources:
  - kind: mock
    id: test-source
    autoStart: true
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher
    sources:
      - sourceId: test-source
reactions:
  - kind: log
    id: test-reaction
    queries:
      - test-query
    autoStart: true
instances: []
"#;

        let config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(config_yaml).expect("Failed to parse config");

        // Verify single-instance format was loaded correctly
        assert!(
            config.instances.is_empty(),
            "instances should be empty for single-instance format"
        );
        assert_eq!(config.sources.len(), 1, "Should have 1 source at root");
        assert_eq!(config.queries.len(), 1, "Should have 1 query at root");
        assert_eq!(config.reactions.len(), 1, "Should have 1 reaction at root");
        assert!(config.persist_index, "persist_index should be true");
        assert!(config.persist_config, "persist_config should be true");

        // Verify source details
        assert_eq!(config.sources[0].id(), "test-source");

        // Verify query details
        assert_eq!(config.queries[0].id, "test-query");

        // Verify reaction details
        assert_eq!(config.reactions[0].id(), "test-reaction");
    }

    #[test]
    fn test_load_multi_instance_config_format() {
        let config_yaml = r#"
host: 0.0.0.0
port: 8080
logLevel: debug
persistConfig: true
sources: []
queries: []
reactions: []
instances:
  - id: analytics
    persistIndex: true
    sources:
      - kind: mock
        id: analytics-source
        autoStart: true
    queries:
      - id: analytics-query
        query: "MATCH (n) RETURN n"
        queryLanguage: Cypher
        sources:
          - sourceId: analytics-source
    reactions:
      - kind: log
        id: analytics-reaction
        queries:
          - analytics-query
        autoStart: true
  - id: monitoring
    persistIndex: false
    sources:
      - kind: mock
        id: monitoring-source
        autoStart: false
    queries:
      - id: monitoring-query
        query: "MATCH (m) RETURN m"
        queryLanguage: Cypher
        sources:
          - sourceId: monitoring-source
    reactions: []
"#;

        let config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(config_yaml).expect("Failed to parse config");

        // Verify multi-instance format was loaded correctly
        assert_eq!(config.instances.len(), 2, "Should have 2 instances");
        assert!(config.sources.is_empty(), "Root sources should be empty");
        assert!(config.queries.is_empty(), "Root queries should be empty");
        assert!(
            config.reactions.is_empty(),
            "Root reactions should be empty"
        );

        // Verify first instance
        let analytics = &config.instances[0];
        match &analytics.id {
            ConfigValue::Static(id) => assert_eq!(id, "analytics"),
            _ => panic!("Expected static id"),
        }
        assert!(
            analytics.persist_index,
            "analytics should have persist_index=true"
        );
        assert_eq!(analytics.sources.len(), 1);
        assert_eq!(analytics.queries.len(), 1);
        assert_eq!(analytics.reactions.len(), 1);

        // Verify second instance
        let monitoring = &config.instances[1];
        match &monitoring.id {
            ConfigValue::Static(id) => assert_eq!(id, "monitoring"),
            _ => panic!("Expected static id"),
        }
        assert!(
            !monitoring.persist_index,
            "monitoring should have persist_index=false"
        );
        assert_eq!(monitoring.sources.len(), 1);
        assert_eq!(monitoring.queries.len(), 1);
        assert!(monitoring.reactions.is_empty());
    }

    // ==================== Source/Reaction Registration Tests ====================

    #[tokio::test]
    async fn test_source_reaction_registration_when_enabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");
        std::fs::write(&config_path, "").expect("Failed to create test file");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Register a source config
        let source_config = SourceConfig::Mock {
            id: "dynamic-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: crate::api::models::MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(1000),
            },
        };
        persistence
            .register_source("test-server", source_config)
            .await;

        // Register a reaction config
        let reaction_config = ReactionConfig::Log {
            id: "dynamic-reaction".to_string(),
            queries: vec!["test-query".to_string()],
            auto_start: true,
            config: crate::api::models::LogReactionConfigDto {
                routes: std::collections::HashMap::new(),
                default_template: None,
            },
        };
        persistence
            .register_reaction("test-server", reaction_config)
            .await;

        // Save and verify source/reaction configs are included
        persistence.save().await.expect("Save failed");

        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        // Single instance format - sources/reactions at root level
        assert_eq!(
            loaded_config.sources.len(),
            1,
            "Should have registered source"
        );
        assert_eq!(loaded_config.sources[0].id(), "dynamic-source");

        assert_eq!(
            loaded_config.reactions.len(),
            1,
            "Should have registered reaction"
        );
        assert_eq!(loaded_config.reactions[0].id(), "dynamic-reaction");
    }

    #[tokio::test]
    async fn test_source_reaction_registration_skipped_when_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false, // persist_config = false (persistence disabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Try to register a source config - should be skipped
        let source_config = SourceConfig::Mock {
            id: "dynamic-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: crate::api::models::MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(1000),
            },
        };
        persistence
            .register_source("test-server", source_config)
            .await;

        // Try to register a reaction config - should be skipped
        let reaction_config = ReactionConfig::Log {
            id: "dynamic-reaction".to_string(),
            queries: vec!["test-query".to_string()],
            auto_start: true,
            config: crate::api::models::LogReactionConfigDto {
                routes: std::collections::HashMap::new(),
                default_template: None,
            },
        };
        persistence
            .register_reaction("test-server", reaction_config)
            .await;

        // Verify internal maps are empty (registration was skipped)
        let source_configs = persistence.source_configs.read().await;
        assert!(
            source_configs.is_empty(),
            "Source configs should be empty when persistence disabled"
        );

        let reaction_configs = persistence.reaction_configs.read().await;
        assert!(
            reaction_configs.is_empty(),
            "Reaction configs should be empty when persistence disabled"
        );

        // Save should also not write anything
        persistence
            .save()
            .await
            .expect("Save should succeed (no-op)");
        assert!(
            !config_path.exists(),
            "File should not exist when persistence disabled"
        );
    }

    #[tokio::test]
    async fn test_unregister_skipped_when_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        // Start with some initial configs
        let mut initial_sources = IndexMap::new();
        let mut instance_sources = IndexMap::new();
        instance_sources.insert(
            "existing-source".to_string(),
            SourceConfig::Mock {
                id: "existing-source".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: crate::api::models::MockSourceConfigDto {
                    data_type: DataTypeDto::Generic,
                    interval_ms: ConfigValue::Static(1000),
                },
            },
        );
        initial_sources.insert("test-server".to_string(), instance_sources);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            false, // persist_config = false (persistence disabled)
            persist_settings,
            initial_sources,
            IndexMap::new(),
            IndexMap::new(),
        );

        // Try to unregister - should be skipped because persistence is disabled
        persistence
            .unregister_source("test-server", "existing-source")
            .await;

        // The internal map should still have the source (unregister was skipped)
        let source_configs = persistence.source_configs.read().await;
        assert!(
            source_configs.get("test-server").is_some(),
            "Source should still exist because unregister was skipped"
        );
    }

    // ==================== Persistence Enabled/Disabled Behavior Tests ====================

    #[tokio::test]
    async fn test_changes_persisted_when_enabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");
        std::fs::write(&config_path, "").expect("Failed to create test file");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // First save
        persistence.save().await.expect("Save failed");

        // Verify file exists and has content
        assert!(config_path.exists(), "Config file should exist");
        let content1 = std::fs::read_to_string(&config_path).expect("Failed to read config");
        assert!(content1.contains("host:"), "Config should contain host");
        // Queries are only saved when registered via API, not from DrasiLib instances
        // Since no queries were registered, the config should not contain any

        // Register a new source
        let source_config = SourceConfig::Mock {
            id: "new-source".to_string(),
            auto_start: false,
            bootstrap_provider: None,
            config: crate::api::models::MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(1000),
            },
        };
        persistence
            .register_source("test-server", source_config)
            .await;

        // Second save
        persistence.save().await.expect("Second save failed");

        // Verify new content includes the registered source
        let content2 = std::fs::read_to_string(&config_path).expect("Failed to read config");
        assert!(
            content2.contains("new-source"),
            "Config should contain new source"
        );
    }

    #[tokio::test]
    async fn test_changes_not_persisted_when_disabled() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create an initial config file
        let initial_content = r#"
host: localhost
port: 9999
logLevel: warn
"#;
        std::fs::write(&config_path, initial_content).expect("Failed to create initial file");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(), // Different from initial
            8080,                    // Different from initial
            "info".to_string(),      // Different from initial
            false,                   // persist_config = false (persistence disabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Try to save - should be skipped
        persistence
            .save()
            .await
            .expect("Save should succeed (no-op)");

        // Verify original file is unchanged
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        assert!(
            content.contains("port: 9999"),
            "Original port should be preserved"
        );
        assert!(
            content.contains("localhost"),
            "Original host should be preserved"
        );
        assert!(
            !content.contains("127.0.0.1"),
            "New host should NOT be written"
        );
    }

    #[tokio::test]
    async fn test_single_instance_format_preserved_after_changes() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");
        std::fs::write(&config_path, "").expect("Failed to create test file");

        // Single instance
        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true, // persist_config = true (persistence enabled)
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Register some configs
        let source_config = SourceConfig::Mock {
            id: "added-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: crate::api::models::MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(1000),
            },
        };
        persistence
            .register_source("test-server", source_config)
            .await;

        let reaction_config = ReactionConfig::Log {
            id: "added-reaction".to_string(),
            queries: vec!["test-query".to_string()],
            auto_start: true,
            config: crate::api::models::LogReactionConfigDto {
                routes: std::collections::HashMap::new(),
                default_template: None,
            },
        };
        persistence
            .register_reaction("test-server", reaction_config)
            .await;

        // Save
        persistence.save().await.expect("Save failed");

        // Load and verify single-instance format is used
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        // Should be single-instance format
        assert!(
            loaded_config.instances.is_empty(),
            "Should use single-instance format"
        );
        assert_eq!(loaded_config.sources.len(), 1, "Source at root level");
        // Queries are only saved when registered, not from DrasiLib instances
        assert_eq!(
            loaded_config.queries.len(),
            0,
            "No queries saved (not registered)"
        );
        assert_eq!(loaded_config.reactions.len(), 1, "Reaction at root level");

        // Verify content
        assert_eq!(loaded_config.sources[0].id(), "added-source");
        assert_eq!(loaded_config.reactions[0].id(), "added-reaction");
    }

    // ==================== Query Persistence Tests ====================

    #[tokio::test]
    async fn test_query_registration_persists_correctly() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create a test file
        std::fs::write(&config_path, "").expect("Failed to create test file");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true,
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Register a query
        let query1 = QueryConfigDto {
            id: "test-query-1".to_string(),
            auto_start: true,
            query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            middleware: vec![],
            sources: vec![],
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        };
        persistence.register_query("test-server", query1).await;

        // Save
        persistence.save().await.expect("Save failed");

        // Verify query was persisted
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        assert_eq!(loaded_config.queries.len(), 1, "Query should be persisted");
        assert_eq!(loaded_config.queries[0].id, "test-query-1");
        assert_eq!(
            loaded_config.queries[0].query,
            ConfigValue::Static("MATCH (n) RETURN n".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiple_queries_persist_without_overwriting() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create a test file
        std::fs::write(&config_path, "").expect("Failed to create test file");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true,
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );

        // Register first query
        let query1 = QueryConfigDto {
            id: "query-1".to_string(),
            auto_start: true,
            query: ConfigValue::Static("MATCH (n:Node) RETURN n".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            middleware: vec![],
            sources: vec![],
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        };
        persistence.register_query("test-server", query1).await;

        // Save first time
        persistence.save().await.expect("Save failed");

        // Register second query
        let query2 = QueryConfigDto {
            id: "query-2".to_string(),
            auto_start: false,
            query: ConfigValue::Static("MATCH (s:Sensor) RETURN s".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            middleware: vec![],
            sources: vec![],
            enable_bootstrap: false,
            bootstrap_buffer_size: 5000,
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        };
        persistence.register_query("test-server", query2).await;

        // Save second time
        persistence.save().await.expect("Save failed");

        // Verify both queries are persisted (second didn't overwrite first)
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        assert_eq!(loaded_config.queries.len(), 2, "Both queries should be persisted");

        // Verify first query still exists
        let q1 = loaded_config.queries.iter().find(|q| q.id == "query-1");
        assert!(q1.is_some(), "First query should still exist");
        assert_eq!(
            q1.unwrap().query,
            ConfigValue::Static("MATCH (n:Node) RETURN n".to_string())
        );

        // Verify second query was added
        let q2 = loaded_config.queries.iter().find(|q| q.id == "query-2");
        assert!(q2.is_some(), "Second query should be added");
        assert_eq!(
            q2.unwrap().query,
            ConfigValue::Static("MATCH (s:Sensor) RETURN s".to_string())
        );
    }

    #[tokio::test]
    async fn test_initial_queries_preserved_on_save() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.yaml");

        // Create a test file
        std::fs::write(&config_path, "").expect("Failed to create test file");

        let core = create_test_core().await;
        let mut instances_map = IndexMap::new();
        instances_map.insert("test-server".to_string(), core.clone());
        let mut persist_settings = IndexMap::new();
        persist_settings.insert("test-server".to_string(), false);

        // Create initial query configs (simulating loading from config file)
        let initial_query = QueryConfigDto {
            id: "initial-query".to_string(),
            auto_start: true,
            query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            middleware: vec![],
            sources: vec![],
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        };

        let mut initial_query_configs: IndexMap<String, IndexMap<String, QueryConfigDto>> =
            IndexMap::new();
        let mut server_queries = IndexMap::new();
        server_queries.insert("initial-query".to_string(), initial_query);
        initial_query_configs.insert("test-server".to_string(), server_queries);

        let persistence = ConfigPersistence::new(
            config_path.clone(),
            InstanceRegistry::from_map(instances_map),
            "127.0.0.1".to_string(),
            8080,
            "info".to_string(),
            true,
            persist_settings,
            IndexMap::new(),
            IndexMap::new(),
            initial_query_configs,
        );

        // Register a new query
        let new_query = QueryConfigDto {
            id: "new-query".to_string(),
            auto_start: false,
            query: ConfigValue::Static("MATCH (s:Sensor) RETURN s".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            middleware: vec![],
            sources: vec![],
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        };
        persistence.register_query("test-server", new_query).await;

        // Save
        persistence.save().await.expect("Save failed");

        // Verify both initial and new queries are persisted
        let content = std::fs::read_to_string(&config_path).expect("Failed to read config");
        let loaded_config: DrasiServerConfig =
            crate::config::loader::from_yaml_str(&content).expect("Failed to parse saved config");

        assert_eq!(loaded_config.queries.len(), 2, "Both queries should be persisted");

        // Verify initial query is preserved
        assert!(
            loaded_config.queries.iter().any(|q| q.id == "initial-query"),
            "Initial query should be preserved"
        );

        // Verify new query was added
        assert!(
            loaded_config.queries.iter().any(|q| q.id == "new-query"),
            "New query should be added"
        );
    }
}
