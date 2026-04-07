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

use crate::api::models::bootstrap::BootstrapProviderConfig;
use crate::api::models::{ConfigValue, QueryConfigDto};
use crate::config::{
    DrasiLibInstanceConfig, DrasiServerConfig, PluginDependency, ReactionConfig, SourceConfig,
    TrustedIdentity,
};
use crate::instance_registry::InstanceRegistry;
use anyhow::Result;
use indexmap::IndexMap;
use log::{debug, error, info};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Server-level settings preserved from the original config file.
///
/// These fields are not part of any DrasiLib instance — they are server-wide
/// settings that `save()` must round-trip faithfully to avoid erasing user
/// configuration on the first persist operation.
#[derive(Clone)]
struct PreservedServerSettings {
    enable_ui: bool,
    plugin_registry: Option<String>,
    auto_install_plugins: bool,
    plugins: Vec<PluginDependency>,
    verify_plugins: bool,
    trusted_identities: Vec<TrustedIdentity>,
    hot_reload_plugins: bool,
    hot_reload_debounce_ms: u64,
    hot_reload_mode: String,
    cors_allowed_origins: Vec<String>,
}

/// Snapshot-based persistence for DrasiServerConfig.
///
/// Uses a single-source-of-truth approach: all component state lives in the
/// ComponentGraph inside each DrasiLib instance. There is no shadow state or
/// separate registration cache — the `save()` method calls
/// `snapshot_configuration()` on every registered instance to capture the
/// current sources, queries, and reactions, then serialises them to YAML.
///
/// Writes are atomic (temp file → rename) to prevent corruption on crash.
pub struct ConfigPersistence {
    config_file_path: PathBuf,
    registry: InstanceRegistry,
    host: String,
    port: u16,
    log_level: String,
    persist_config: bool,
    persist_settings: IndexMap<String, bool>,
    solutions_dir: Option<String>,
    /// Server-level settings preserved from the original config file.
    preserved: PreservedServerSettings,
    /// Instance configs for dynamic instances
    instance_configs: Arc<RwLock<IndexMap<String, DrasiLibInstanceConfig>>>,
}

impl ConfigPersistence {
    /// Create a new ConfigPersistence instance.
    ///
    /// The `original_config` parameter captures server-level settings (plugin
    /// registry, hot-reload, verification, etc.) so they are preserved across
    /// save operations rather than being reset to hard-coded defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config_file_path: PathBuf,
        registry: InstanceRegistry,
        host: String,
        port: u16,
        log_level: String,
        persist_config: bool,
        persist_settings: IndexMap<String, bool>,
        solutions_dir: Option<String>,
        original_config: &DrasiServerConfig,
    ) -> Self {
        Self {
            config_file_path,
            registry,
            host,
            port,
            log_level,
            persist_config,
            persist_settings,
            solutions_dir,
            preserved: PreservedServerSettings {
                enable_ui: original_config.enable_ui,
                plugin_registry: original_config.plugin_registry.clone(),
                auto_install_plugins: original_config.auto_install_plugins,
                plugins: original_config.plugins.clone(),
                verify_plugins: original_config.verify_plugins,
                trusted_identities: original_config.trusted_identities.clone(),
                hot_reload_plugins: original_config.hot_reload_plugins,
                hot_reload_debounce_ms: original_config.hot_reload_debounce_ms,
                hot_reload_mode: original_config.hot_reload_mode.clone(),
                cors_allowed_origins: original_config.cors_allowed_origins.clone(),
            },
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

    /// Save the current configuration to the config file using atomic writes.
    /// Uses `snapshot_configuration()` to get current state from each DrasiLib instance.
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

        let dynamic_instance_configs = self.instance_configs.read().await;

        let mut instance_configs = Vec::new();

        for (id, core) in self.registry.list().await {
            let snapshot = core
                .snapshot_configuration()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to snapshot instance '{id}': {e}"))?;

            let persist_index = *self.persist_settings.get(&id).unwrap_or(&false);

            // Map snapshot sources to SourceConfig, filtering internal sources
            let sources: Vec<SourceConfig> = snapshot
                .sources
                .iter()
                .filter(|s| !s.id.starts_with("__"))
                .map(|s| {
                    let mut config_map = serde_json::Map::new();
                    for (k, v) in &s.properties {
                        config_map.insert(k.clone(), v.clone());
                    }
                    SourceConfig {
                        kind: s.source_type.clone(),
                        id: s.id.clone(),
                        auto_start: s.auto_start,
                        bootstrap_provider: s.bootstrap_provider.as_ref().map(|bp| {
                            let mut bp_config = serde_json::Map::new();
                            for (k, v) in &bp.properties {
                                bp_config.insert(k.clone(), v.clone());
                            }
                            BootstrapProviderConfig {
                                kind: bp.kind.clone(),
                                config: serde_json::Value::Object(bp_config),
                            }
                        }),
                        config: serde_json::Value::Object(config_map),
                    }
                })
                .collect();

            // Map snapshot queries to QueryConfigDto
            let queries: Vec<QueryConfigDto> = snapshot
                .queries
                .iter()
                .map(|q| QueryConfigDto::from(q.config.clone()))
                .collect();

            // Map snapshot reactions to ReactionConfig
            let reactions: Vec<ReactionConfig> = snapshot
                .reactions
                .iter()
                .map(|r| {
                    let mut config_map = serde_json::Map::new();
                    for (k, v) in &r.properties {
                        config_map.insert(k.clone(), v.clone());
                    }
                    ReactionConfig {
                        kind: r.reaction_type.clone(),
                        id: r.id.clone(),
                        queries: r.queries.clone(),
                        auto_start: r.auto_start,
                        config: serde_json::Value::Object(config_map),
                    }
                })
                .collect();

            // Check if this is a dynamically created instance
            let instance_config = if let Some(dynamic_config) = dynamic_instance_configs.get(&id) {
                DrasiLibInstanceConfig {
                    id: ConfigValue::Static(snapshot.instance_id.clone()),
                    persist_index: dynamic_config.persist_index,
                    state_store: dynamic_config.state_store.clone(),
                    default_priority_queue_capacity: dynamic_config
                        .default_priority_queue_capacity
                        .clone(),
                    default_dispatch_buffer_capacity: dynamic_config
                        .default_dispatch_buffer_capacity
                        .clone(),
                    sources,
                    reactions,
                    queries,
                }
            } else {
                DrasiLibInstanceConfig {
                    id: ConfigValue::Static(snapshot.instance_id.clone()),
                    persist_index,
                    state_store: None,
                    default_priority_queue_capacity: None,
                    default_dispatch_buffer_capacity: None,
                    sources,
                    reactions,
                    queries,
                }
            };
            instance_configs.push(instance_config);
        }

        // Dynamic format selection based on instance count
        let wrapper_config = if instance_configs.len() == 1 {
            // Single instance → use single-instance format (root-level fields)
            let instance = instance_configs.remove(0);
            DrasiServerConfig {
                api_version: None,
                id: instance.id,
                host: ConfigValue::Static(self.host.clone()),
                port: ConfigValue::Static(self.port),
                log_level: ConfigValue::Static(self.log_level.clone()),
                persist_config: self.persist_config,
                persist_index: instance.persist_index,
                enable_ui: self.preserved.enable_ui,
                solutions_dir: self.solutions_dir.clone(),
                state_store: instance.state_store,
                default_priority_queue_capacity: instance.default_priority_queue_capacity,
                default_dispatch_buffer_capacity: instance.default_dispatch_buffer_capacity,
                plugin_registry: self.preserved.plugin_registry.clone(),
                auto_install_plugins: self.preserved.auto_install_plugins,
                plugins: self.preserved.plugins.clone(),
                verify_plugins: self.preserved.verify_plugins,
                trusted_identities: self.preserved.trusted_identities.clone(),
                hot_reload_plugins: self.preserved.hot_reload_plugins,
                hot_reload_debounce_ms: self.preserved.hot_reload_debounce_ms,
                hot_reload_mode: self.preserved.hot_reload_mode.clone(),
                cors_allowed_origins: self.preserved.cors_allowed_origins.clone(),
                sources: instance.sources,
                queries: instance.queries,
                reactions: instance.reactions,
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
                api_version: None,
                id: ConfigValue::Static(first_id),
                host: ConfigValue::Static(self.host.clone()),
                port: ConfigValue::Static(self.port),
                log_level: ConfigValue::Static(self.log_level.clone()),
                persist_config: self.persist_config,
                persist_index: false, // Per-instance setting in multi-instance mode
                enable_ui: self.preserved.enable_ui,
                solutions_dir: self.solutions_dir.clone(),
                state_store: None, // Per-instance setting in multi-instance mode
                default_priority_queue_capacity: None,
                default_dispatch_buffer_capacity: None,
                plugin_registry: self.preserved.plugin_registry.clone(),
                auto_install_plugins: self.preserved.auto_install_plugins,
                plugins: self.preserved.plugins.clone(),
                verify_plugins: self.preserved.verify_plugins,
                trusted_identities: self.preserved.trusted_identities.clone(),
                hot_reload_plugins: self.preserved.hot_reload_plugins,
                hot_reload_debounce_ms: self.preserved.hot_reload_debounce_ms,
                hot_reload_mode: self.preserved.hot_reload_mode.clone(),
                cors_allowed_origins: self.preserved.cors_allowed_origins.clone(),
                sources: Vec::new(),
                queries: Vec::new(),
                reactions: Vec::new(),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use drasi_lib::{
        channels::{
            dispatcher::{ChangeDispatcher, ChannelChangeDispatcher},
            ComponentStatus, SubscriptionResponse,
        },
        config::SourceSubscriptionSettings,
        context::{ReactionRuntimeContext, SourceRuntimeContext},
        reactions::Reaction,
        sources::Source,
        DrasiLib, Query,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    // ─── Test Stubs ──────────────────────────────────────────────────

    /// Source stub with configurable properties, type name, and auto_start.
    struct TestSource {
        id: String,
        type_name: String,
        auto_start: bool,
        props: HashMap<String, serde_json::Value>,
    }

    impl TestSource {
        fn new(id: &str, kind: &str) -> Self {
            Self {
                id: id.to_string(),
                type_name: kind.to_string(),
                auto_start: false,
                props: HashMap::new(),
            }
        }

        fn with_auto_start(mut self, auto_start: bool) -> Self {
            self.auto_start = auto_start;
            self
        }

        fn with_property(mut self, key: &str, value: serde_json::Value) -> Self {
            self.props.insert(key.to_string(), value);
            self
        }
    }

    #[async_trait]
    impl Source for TestSource {
        fn id(&self) -> &str {
            &self.id
        }
        fn type_name(&self) -> &str {
            &self.type_name
        }
        fn properties(&self) -> HashMap<String, serde_json::Value> {
            self.props.clone()
        }
        fn auto_start(&self) -> bool {
            self.auto_start
        }
        async fn start(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn status(&self) -> ComponentStatus {
            ComponentStatus::Stopped
        }
        async fn subscribe(
            &self,
            settings: SourceSubscriptionSettings,
        ) -> anyhow::Result<SubscriptionResponse> {
            let dispatcher =
                ChannelChangeDispatcher::<drasi_lib::channels::SourceEventWrapper>::new(10);
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
        async fn initialize(&self, _context: SourceRuntimeContext) {}
    }

    /// Reaction stub with configurable properties, type name, and auto_start.
    struct TestReaction {
        id: String,
        type_name: String,
        auto_start: bool,
        queries: Vec<String>,
        props: HashMap<String, serde_json::Value>,
    }

    impl TestReaction {
        fn new(id: &str, kind: &str, queries: Vec<String>) -> Self {
            Self {
                id: id.to_string(),
                type_name: kind.to_string(),
                auto_start: false,
                queries,
                props: HashMap::new(),
            }
        }

        fn with_auto_start(mut self, auto_start: bool) -> Self {
            self.auto_start = auto_start;
            self
        }

        fn with_property(mut self, key: &str, value: serde_json::Value) -> Self {
            self.props.insert(key.to_string(), value);
            self
        }
    }

    #[async_trait]
    impl Reaction for TestReaction {
        fn id(&self) -> &str {
            &self.id
        }
        fn type_name(&self) -> &str {
            &self.type_name
        }
        fn properties(&self) -> HashMap<String, serde_json::Value> {
            self.props.clone()
        }
        fn query_ids(&self) -> Vec<String> {
            self.queries.clone()
        }
        fn auto_start(&self) -> bool {
            self.auto_start
        }
        async fn initialize(&self, _context: ReactionRuntimeContext) {}
        async fn start(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn status(&self) -> ComponentStatus {
            ComponentStatus::Stopped
        }
    }

    // ─── Helpers ─────────────────────────────────────────────────────

    /// Build a DrasiLib with the given id, sources, queries, and reactions.
    async fn build_core(
        id: &str,
        sources: Vec<TestSource>,
        queries: Vec<drasi_lib::QueryConfig>,
        reactions: Vec<TestReaction>,
    ) -> Arc<DrasiLib> {
        let mut builder = DrasiLib::builder().with_id(id);
        for s in sources {
            builder = builder.with_source(s);
        }
        for q in queries {
            builder = builder.with_query(q);
        }
        for r in reactions {
            builder = builder.with_reaction(r);
        }
        Arc::new(builder.build().await.unwrap())
    }

    /// Create a ConfigPersistence wired to a single DrasiLib instance.
    fn make_persistence(
        core: Arc<DrasiLib>,
        instance_id: &str,
        path: std::path::PathBuf,
        persist: bool,
    ) -> ConfigPersistence {
        let default_config = DrasiServerConfig::default();
        make_persistence_with_config(core, instance_id, path, persist, &default_config)
    }

    /// Create a ConfigPersistence with custom server-level settings.
    fn make_persistence_with_config(
        core: Arc<DrasiLib>,
        instance_id: &str,
        path: std::path::PathBuf,
        persist: bool,
        original_config: &DrasiServerConfig,
    ) -> ConfigPersistence {
        let mut map = IndexMap::new();
        map.insert(instance_id.to_string(), core);
        let registry = InstanceRegistry::from_map(map);
        ConfigPersistence::new(
            path,
            registry,
            "0.0.0.0".to_string(),
            8080,
            "info".to_string(),
            persist,
            IndexMap::new(),
            None,
            original_config,
        )
    }

    /// Parse the saved YAML file back into a serde_yaml::Value for assertions.
    fn read_yaml(path: &std::path::Path) -> serde_yaml::Value {
        let content = std::fs::read_to_string(path).unwrap();
        serde_yaml::from_str(&content).unwrap()
    }

    // ─── Tests ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_save_writes_valid_yaml() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "mock")],
            vec![Query::cypher("q1")
                .query("MATCH (n) RETURN n")
                .from_source("src1")
                .build()],
            vec![TestReaction::new("rx1", "log", vec!["q1".into()])],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        // File must exist and parse as valid YAML
        let val = read_yaml(&cfg_path);
        assert!(val.is_mapping(), "root should be a YAML mapping");

        // Verify top-level server fields
        let map = val.as_mapping().unwrap();
        assert_eq!(
            map.get(serde_yaml::Value::String("host".into()))
                .unwrap()
                .as_str()
                .unwrap(),
            "0.0.0.0"
        );
        assert_eq!(
            map.get(serde_yaml::Value::String("port".into()))
                .unwrap()
                .as_u64()
                .unwrap(),
            8080
        );
    }

    #[tokio::test]
    async fn test_save_captures_all_sources() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![
                TestSource::new("src-a", "http")
                    .with_property("url", serde_json::json!("http://example.com")),
                TestSource::new("src-b", "postgres")
                    .with_property("host", serde_json::json!("db.local")),
                TestSource::new("src-c", "mock"),
            ],
            vec![],
            vec![],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let sources = val["sources"].as_sequence().unwrap();
        assert_eq!(sources.len(), 3, "all three sources must be present");

        let ids: Vec<&str> = sources.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"src-a"));
        assert!(ids.contains(&"src-b"));
        assert!(ids.contains(&"src-c"));
    }

    #[tokio::test]
    async fn test_save_captures_all_queries() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![
                TestSource::new("events", "mock"),
                TestSource::new("metrics", "mock"),
            ],
            vec![
                Query::cypher("q-alpha")
                    .query("MATCH (e:Event) RETURN e")
                    .from_source("events")
                    .build(),
                Query::cypher("q-beta")
                    .query("MATCH (m:Metric) WHERE m.value > 10 RETURN m")
                    .from_source("metrics")
                    .build(),
            ],
            vec![],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let queries = val["queries"].as_sequence().unwrap();
        assert_eq!(queries.len(), 2);

        // Verify query text is preserved (camelCase field name)
        let q_alpha = queries
            .iter()
            .find(|q| q["id"].as_str() == Some("q-alpha"))
            .unwrap();
        assert_eq!(
            q_alpha["query"].as_str().unwrap(),
            "MATCH (e:Event) RETURN e"
        );

        // Verify source subscription (camelCase: sourceId)
        let sources = q_alpha["sources"].as_sequence().unwrap();
        assert_eq!(sources[0]["sourceId"].as_str().unwrap(), "events");
    }

    #[tokio::test]
    async fn test_save_captures_all_reactions() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "mock")],
            vec![Query::cypher("q1")
                .query("MATCH (n) RETURN n")
                .from_source("src1")
                .build()],
            vec![
                TestReaction::new("rx-webhook", "http", vec!["q1".into()])
                    .with_property("endpoint", serde_json::json!("https://hook.example.com")),
                TestReaction::new("rx-logger", "log", vec!["q1".into()]),
            ],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let reactions = val["reactions"].as_sequence().unwrap();
        assert_eq!(reactions.len(), 2);

        let webhook = reactions
            .iter()
            .find(|r| r["id"].as_str() == Some("rx-webhook"))
            .unwrap();
        assert_eq!(webhook["kind"].as_str().unwrap(), "http");
        assert_eq!(
            webhook["queries"].as_sequence().unwrap()[0]
                .as_str()
                .unwrap(),
            "q1"
        );
        assert_eq!(
            webhook["endpoint"].as_str().unwrap(),
            "https://hook.example.com"
        );
    }

    #[tokio::test]
    async fn test_save_filters_internal_sources() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        // DrasiLib automatically creates an internal `__component_graph__` source.
        // Additionally add a user source whose id starts with `__` to verify filtering.
        let core = build_core(
            "inst1",
            vec![
                TestSource::new("visible", "mock"),
                TestSource::new("__internal_hidden", "noop"),
            ],
            vec![],
            vec![],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let sources = val["sources"].as_sequence().unwrap();
        let ids: Vec<&str> = sources.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"visible"), "user source must be present");
        assert!(
            !ids.iter().any(|id| id.starts_with("__")),
            "internal sources (starting with __) must be filtered out"
        );
    }

    #[tokio::test]
    async fn test_save_noop_when_disabled() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "mock")],
            vec![],
            vec![],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), false);
        p.save().await.unwrap();

        assert!(
            !cfg_path.exists(),
            "config file must NOT be created when persist_config is false"
        );
    }

    #[tokio::test]
    async fn test_save_single_instance_format() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "mock")],
            vec![Query::cypher("q1")
                .query("MATCH (n) RETURN n")
                .from_source("src1")
                .build()],
            vec![TestReaction::new("rx1", "log", vec!["q1".into()])],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let map = val.as_mapping().unwrap();

        // Single-instance format: sources/queries/reactions at root
        assert!(map.contains_key("sources"), "sources must be at root level");
        assert!(map.contains_key("queries"), "queries must be at root level");
        assert!(
            map.contains_key("reactions"),
            "reactions must be at root level"
        );

        // instances array should be absent or empty
        let instances = map.get("instances");
        assert!(
            instances.is_none()
                || instances
                    .and_then(|v| v.as_sequence())
                    .is_none_or(|s| s.is_empty()),
            "single-instance format must not have a populated instances array"
        );
    }

    #[tokio::test]
    async fn test_save_preserves_auto_start() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "inst1",
            vec![
                TestSource::new("src-on", "mock").with_auto_start(true),
                TestSource::new("src-off", "mock").with_auto_start(false),
            ],
            vec![
                Query::cypher("q-on")
                    .query("MATCH (n) RETURN n")
                    .from_source("src-on")
                    .auto_start(true)
                    .build(),
                Query::cypher("q-off")
                    .query("MATCH (n) RETURN n")
                    .from_source("src-off")
                    .auto_start(false)
                    .build(),
            ],
            vec![TestReaction::new("rx1", "log", vec!["q-on".into()])],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);

        // Sources: auto_start metadata is stored by the builder
        let sources = val["sources"].as_sequence().unwrap();
        let src_on = sources
            .iter()
            .find(|s| s["id"].as_str() == Some("src-on"))
            .unwrap();
        let src_off = sources
            .iter()
            .find(|s| s["id"].as_str() == Some("src-off"))
            .unwrap();
        assert!(src_on["autoStart"].as_bool().unwrap());
        assert!(!src_off["autoStart"].as_bool().unwrap());

        // Queries: auto_start metadata is stored by the builder
        let queries = val["queries"].as_sequence().unwrap();
        let q_on = queries
            .iter()
            .find(|q| q["id"].as_str() == Some("q-on"))
            .unwrap();
        let q_off = queries
            .iter()
            .find(|q| q["id"].as_str() == Some("q-off"))
            .unwrap();
        assert!(q_on["autoStart"].as_bool().unwrap());
        assert!(!q_off["autoStart"].as_bool().unwrap());

        // Reactions: verify autoStart field is present in the YAML output
        let reactions = val["reactions"].as_sequence().unwrap();
        let rx = reactions
            .iter()
            .find(|r| r["id"].as_str() == Some("rx1"))
            .unwrap();
        assert!(
            rx["autoStart"].as_bool().is_some(),
            "reaction autoStart must be serialized"
        );
    }

    #[tokio::test]
    async fn test_save_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core(
            "roundtrip-inst",
            vec![TestSource::new("src1", "mock")
                .with_auto_start(true)
                .with_property("interval", serde_json::json!(500))],
            vec![Query::cypher("q1")
                .query("MATCH (s:Sensor) WHERE s.temp > 75 RETURN s")
                .from_source("src1")
                .auto_start(true)
                .build()],
            vec![TestReaction::new("rx1", "log", vec!["q1".into()])
                .with_auto_start(true)
                .with_property("format", serde_json::json!("json"))],
        )
        .await;

        let p = make_persistence(core, "roundtrip-inst", cfg_path.clone(), true);
        p.save().await.unwrap();

        // Parse back as the strongly-typed DrasiServerConfig
        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let parsed: crate::config::DrasiServerConfig = serde_yaml::from_str(&content).unwrap();

        // Server settings
        assert_eq!(
            parsed.host,
            crate::api::models::ConfigValue::Static("0.0.0.0".to_string())
        );
        assert_eq!(parsed.port, crate::api::models::ConfigValue::Static(8080));
        assert!(parsed.persist_config);

        // Single-instance → sources/queries/reactions at root
        assert_eq!(parsed.sources.len(), 1);
        assert_eq!(parsed.sources[0].id, "src1");
        assert_eq!(parsed.sources[0].kind, "mock");

        assert_eq!(parsed.queries.len(), 1);
        assert_eq!(parsed.queries[0].id, "q1");
        assert_eq!(
            parsed.queries[0].query,
            "MATCH (s:Sensor) WHERE s.temp > 75 RETURN s"
        );
        assert_eq!(parsed.queries[0].sources[0].source_id, "src1");

        assert_eq!(parsed.reactions.len(), 1);
        assert_eq!(parsed.reactions[0].id, "rx1");
        assert_eq!(parsed.reactions[0].kind, "log");
        assert_eq!(parsed.reactions[0].queries, vec!["q1".to_string()]);
    }

    #[tokio::test]
    async fn test_save_preserves_server_level_settings() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core("settings-inst", vec![], vec![], vec![]).await;

        // Create a config with non-default server-level settings
        let mut original_config = DrasiServerConfig::default();
        original_config.enable_ui = false;
        original_config.plugin_registry = Some("my-registry.io/plugins".to_string());
        original_config.auto_install_plugins = true;
        original_config.plugins = vec![PluginDependency {
            reference: "source/postgres:0.5.0".to_string(),
        }];
        original_config.verify_plugins = true;
        original_config.trusted_identities = vec![TrustedIdentity {
            issuer: "https://accounts.google.com".to_string(),
            subject_pattern: "builder@my-org.iam.gserviceaccount.com".to_string(),
        }];
        original_config.hot_reload_plugins = true;
        original_config.hot_reload_debounce_ms = 500;
        original_config.hot_reload_mode = "side-by-side".to_string();
        original_config.cors_allowed_origins =
            vec!["http://localhost:3000".to_string(), "https://dashboard.example.com".to_string()];

        let p = make_persistence_with_config(
            core,
            "settings-inst",
            cfg_path.clone(),
            true,
            &original_config,
        );
        p.save().await.unwrap();

        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let parsed: DrasiServerConfig = serde_yaml::from_str(&content).unwrap();

        // All server-level settings should be preserved, not reset to defaults
        assert!(!parsed.enable_ui);
        assert_eq!(
            parsed.plugin_registry,
            Some("my-registry.io/plugins".to_string())
        );
        assert!(parsed.auto_install_plugins);
        assert_eq!(parsed.plugins.len(), 1);
        assert_eq!(parsed.plugins[0].reference, "source/postgres:0.5.0");
        assert!(parsed.verify_plugins);
        assert_eq!(parsed.trusted_identities.len(), 1);
        assert_eq!(
            parsed.trusted_identities[0].issuer,
            "https://accounts.google.com"
        );
        assert!(parsed.hot_reload_plugins);
        assert_eq!(parsed.hot_reload_debounce_ms, 500);
        assert_eq!(parsed.hot_reload_mode, "side-by-side");
        assert_eq!(parsed.cors_allowed_origins.len(), 2);
        assert_eq!(parsed.cors_allowed_origins[0], "http://localhost:3000");
        assert_eq!(
            parsed.cors_allowed_origins[1],
            "https://dashboard.example.com"
        );
    }
}
