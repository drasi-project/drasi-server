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

use crate::api::mappings::DtoMapper;
use crate::api::models::bootstrap::{BootstrapProviderConfig, BootstrapProviderRef};
use crate::api::models::{ConfigValue, QueryConfigDto, ReactionConfig, SourceConfig};
use crate::config::{DrasiLibInstanceConfig, DrasiServerConfig};
use crate::instance_registry::InstanceRegistry;
use anyhow::{Context, Result};
use indexmap::IndexMap;
use log::{debug, error, info};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Snapshot-based persistence for DrasiServerConfig.
///
/// Uses a single-source-of-truth approach: all component state lives in the
/// ComputationGraph inside each DrasiLib instance. Runtime state is not cached
/// separately — the `save()` method calls
/// `snapshot_computation_configuration()` on every registered instance to capture
/// ordinary components and native graph declarations, then serialises them to YAML.
///
/// Writes are atomic (temp file → rename) to prevent corruption on crash.
pub struct ConfigPersistence {
    config_file_path: PathBuf,
    registry: InstanceRegistry,
    server_configuration: DrasiServerConfig,
    /// Config-only settings for both declared and API-created instances.
    instance_configs: Arc<RwLock<IndexMap<String, DrasiLibInstanceConfig>>>,
    /// Per-component `identityProvider` references for sources, keyed by
    /// `(instance_id, source_id)`. Seeded from the original config and
    /// kept current by the API handlers via
    /// [`ConfigPersistence::register_source_identity_provider`] /
    /// [`ConfigPersistence::unregister_source_identity_provider`].
    ///
    /// Source/reaction `identityProvider` is a string reference (the id of an
    /// entry in `identityProviders`) that is not stored on the runtime
    /// component, so `snapshot_configuration()` cannot recover it. Without
    /// this map the first `save()` would erase every `identityProvider: <id>`
    /// line from the YAML.
    source_identity_provider: Arc<RwLock<IndexMap<(String, String), String>>>,
    /// Per-component `identityProvider` references for reactions, keyed by
    /// `(instance_id, reaction_id)`. See `source_identity_provider`.
    reaction_identity_provider: Arc<RwLock<IndexMap<(String, String), String>>>,
    /// Per-component `bootstrapProvider` for sources, keyed by
    /// `(instance_id, source_id)`. Stores the full [`BootstrapProviderRef`]
    /// (either a top-level reference or an inline definition).
    ///
    /// drasi-lib's `snapshot_configuration()` does not reliably carry a
    /// source's bootstrap provider, so without this map the first `save()`
    /// would drop both inline `bootstrapProvider:` blocks (issue #105) and
    /// `bootstrapProvider: <id>` references. Seeded from the original config
    /// and kept current by the source API handlers.
    source_bootstrap_provider: Arc<RwLock<IndexMap<(String, String), BootstrapProviderRef>>>,
}

fn capture_instance_settings(
    mut config: DrasiLibInstanceConfig,
    source_identity: &mut IndexMap<(String, String), String>,
    reaction_identity: &mut IndexMap<(String, String), String>,
    source_bootstrap: &mut IndexMap<(String, String), BootstrapProviderRef>,
) -> Result<(String, DrasiLibInstanceConfig)> {
    let id: String = DtoMapper::new().resolve_typed(config.id.as_ref())?;
    anyhow::ensure!(!id.is_empty(), "Cannot preserve an empty instance identity");
    for source in &config.sources {
        if let Some(provider) = source.identity_provider() {
            source_identity.insert((id.clone(), source.id.clone()), provider.to_owned());
        }
        if let Some(provider) = source.bootstrap_provider() {
            source_bootstrap.insert((id.clone(), source.id.clone()), provider.clone());
        }
    }
    for reaction in &config.reactions {
        if let Some(provider) = reaction.identity_provider() {
            reaction_identity.insert((id.clone(), reaction.id.clone()), provider.to_owned());
        }
    }
    config.computation = None;
    config.sources.clear();
    config.queries.clear();
    config.reactions.clear();
    Ok((id, config))
}

impl ConfigPersistence {
    /// Create a new ConfigPersistence instance.
    ///
    /// The `original_config` parameter captures server-level settings (plugin
    /// registry, hot-reload, verification, etc.) so they are preserved across
    /// save operations rather than being reset to hard-coded defaults.
    pub fn new(
        config_file_path: PathBuf,
        registry: InstanceRegistry,
        host: String,
        port: u16,
        log_level: String,
        persist_config: bool,
        original_config: &DrasiServerConfig,
    ) -> Result<Self> {
        let mut server_configuration = original_config.clone();
        server_configuration.instances.clear();
        server_configuration.host = ConfigValue::Static(host);
        server_configuration.port = ConfigValue::Static(port);
        server_configuration.log_level = ConfigValue::Static(log_level);
        server_configuration.persist_config = persist_config;
        let mut instance_configs = IndexMap::new();
        let mut source_identity_provider = IndexMap::new();
        let mut reaction_identity_provider = IndexMap::new();
        let mut source_bootstrap_provider = IndexMap::new();
        for instance in &original_config.instances {
            let (id, settings) = capture_instance_settings(
                instance.clone(),
                &mut source_identity_provider,
                &mut reaction_identity_provider,
                &mut source_bootstrap_provider,
            )?;
            anyhow::ensure!(
                instance_configs.insert(id.clone(), settings).is_none(),
                "Duplicate instance configuration '{id}'"
            );
        }
        Ok(Self {
            config_file_path,
            registry,
            server_configuration,
            instance_configs: Arc::new(RwLock::new(instance_configs)),
            source_identity_provider: Arc::new(RwLock::new(source_identity_provider)),
            reaction_identity_provider: Arc::new(RwLock::new(reaction_identity_provider)),
            source_bootstrap_provider: Arc::new(RwLock::new(source_bootstrap_provider)),
        })
    }

    /// Register an `identityProvider` reference for a source.
    ///
    /// Called by the source create/upsert API handlers so that the reference
    /// survives the next `save()` (since `snapshot_configuration()` does not
    /// carry it). A `None` value removes the entry. No-op when persistence
    /// is disabled.
    pub async fn register_source_identity_provider(
        &self,
        instance_id: &str,
        source_id: &str,
        identity_provider: Option<&str>,
    ) {
        if !self.server_configuration.persist_config {
            return;
        }
        let mut map = self.source_identity_provider.write().await;
        match identity_provider {
            Some(ip) => {
                map.insert(
                    (instance_id.to_string(), source_id.to_string()),
                    ip.to_string(),
                );
            }
            None => {
                map.shift_remove(&(instance_id.to_string(), source_id.to_string()));
            }
        }
    }

    /// Remove any preserved `identityProvider` reference for a source.
    /// Called by the source delete handler.
    pub async fn unregister_source_identity_provider(&self, instance_id: &str, source_id: &str) {
        if !self.server_configuration.persist_config {
            return;
        }
        let mut map = self.source_identity_provider.write().await;
        map.shift_remove(&(instance_id.to_string(), source_id.to_string()));
    }

    /// Register an `identityProvider` reference for a reaction. See
    /// [`Self::register_source_identity_provider`].
    pub async fn register_reaction_identity_provider(
        &self,
        instance_id: &str,
        reaction_id: &str,
        identity_provider: Option<&str>,
    ) {
        if !self.server_configuration.persist_config {
            return;
        }
        let mut map = self.reaction_identity_provider.write().await;
        match identity_provider {
            Some(ip) => {
                map.insert(
                    (instance_id.to_string(), reaction_id.to_string()),
                    ip.to_string(),
                );
            }
            None => {
                map.shift_remove(&(instance_id.to_string(), reaction_id.to_string()));
            }
        }
    }

    /// Remove any preserved `identityProvider` reference for a reaction.
    pub async fn unregister_reaction_identity_provider(
        &self,
        instance_id: &str,
        reaction_id: &str,
    ) {
        if !self.server_configuration.persist_config {
            return;
        }
        let mut map = self.reaction_identity_provider.write().await;
        map.shift_remove(&(instance_id.to_string(), reaction_id.to_string()));
    }

    /// Track a source's `bootstrapProvider` so it round-trips through `save()`.
    ///
    /// Accepts the full [`BootstrapProviderRef`] — either a top-level reference
    /// or an inline definition — because drasi-lib's `snapshot_configuration()`
    /// does not reliably carry it (this is what caused issue #105 for inline
    /// providers). A `None` value removes any existing entry. No-op when
    /// persistence is disabled.
    pub async fn register_source_bootstrap_provider(
        &self,
        instance_id: &str,
        source_id: &str,
        bootstrap_provider: Option<&BootstrapProviderRef>,
    ) {
        if !self.server_configuration.persist_config {
            return;
        }
        let mut map = self.source_bootstrap_provider.write().await;
        match bootstrap_provider {
            Some(bp) => {
                map.insert((instance_id.to_string(), source_id.to_string()), bp.clone());
            }
            None => {
                map.shift_remove(&(instance_id.to_string(), source_id.to_string()));
            }
        }
    }

    /// Remove any preserved `bootstrapProvider` reference for a source.
    /// Called by the source delete handler.
    pub async fn unregister_source_bootstrap_provider(&self, instance_id: &str, source_id: &str) {
        if !self.server_configuration.persist_config {
            return;
        }
        let mut map = self.source_bootstrap_provider.write().await;
        map.shift_remove(&(instance_id.to_string(), source_id.to_string()));
    }

    /// Register a new instance config for persistence
    pub async fn register_instance(&self, config: DrasiLibInstanceConfig) -> Result<()> {
        if !self.server_configuration.persist_config {
            return Ok(());
        }
        let mut instance_configs = self.instance_configs.write().await;
        let mut source_identity = self.source_identity_provider.write().await;
        let mut reaction_identity = self.reaction_identity_provider.write().await;
        let mut source_bootstrap = self.source_bootstrap_provider.write().await;
        let (id, config) = capture_instance_settings(
            config,
            &mut source_identity,
            &mut reaction_identity,
            &mut source_bootstrap,
        )?;
        instance_configs.insert(id, config);
        Ok(())
    }

    /// Save the current configuration to the config file using atomic writes.
    /// Uses `snapshot_configuration()` to get current state from each DrasiLib instance.
    /// Always writes explicit instances, including an empty list.
    pub async fn save(&self) -> Result<()> {
        if !self.server_configuration.persist_config {
            debug!("Persistence disabled (persist_config: false), skipping save");
            return Ok(());
        }

        info!(
            "Saving configuration to {}",
            self.config_file_path.display()
        );

        let instance_settings = self.instance_configs.read().await;
        let source_identity_provider = self.source_identity_provider.read().await;
        let reaction_identity_provider = self.reaction_identity_provider.read().await;
        let source_bootstrap_provider = self.source_bootstrap_provider.read().await;

        let mut instance_configs = Vec::new();

        for (id, core) in self.registry.list().await {
            let mut settings = instance_settings.get(&id).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot persist instance '{id}': its config-only settings were not registered"
                )
            })?;
            if core.configuration_is_persistent() {
                anyhow::ensure!(
                    settings.configuration_store.is_some(),
                    "Cannot persist managed instance '{id}' without its configurationStore"
                );
                instance_configs.push(settings);
                continue;
            }
            anyhow::ensure!(
                settings.configuration_store.is_none(),
                "Cannot persist unmanaged instance '{id}' with configurationStore settings"
            );
            let snapshot = core
                .snapshot_computation_configuration()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to snapshot instance '{id}': {e}"))?;
            let computation =
                crate::computation::configuration_from_snapshot(&snapshot)?.map(Into::into);
            let snapshot = snapshot.instance;

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
                        identity_provider: source_identity_provider
                            .get(&(id.clone(), s.id.clone()))
                            .cloned(),
                        // Prefer the tracked bootstrapProvider (inline or
                        // reference) so it round-trips faithfully. Fall back to
                        // reconstructing an inline form from the runtime
                        // snapshot only if nothing was tracked (e.g. a source
                        // whose provider was attached out-of-band).
                        bootstrap_provider: source_bootstrap_provider
                            .get(&(id.clone(), s.id.clone()))
                            .cloned()
                            .or_else(|| {
                                s.bootstrap_provider.as_ref().map(|bp| {
                                    let mut bp_config = serde_json::Map::new();
                                    for (k, v) in &bp.properties {
                                        bp_config.insert(k.clone(), v.clone());
                                    }
                                    BootstrapProviderRef::Inline(BootstrapProviderConfig {
                                        kind: bp.kind.clone(),
                                        config: serde_json::Value::Object(bp_config),
                                    })
                                })
                            }),
                        config: serde_json::Value::Object(config_map),
                    }
                })
                .collect();

            // Map snapshot queries to QueryConfigDto
            let queries: Vec<QueryConfigDto> = snapshot
                .queries
                .iter()
                .map(|q| {
                    QueryConfigDto::try_from(q.config.clone()).with_context(|| {
                        format!("Cannot serialize query '{}' in instance '{id}'", q.id)
                    })
                })
                .collect::<Result<_>>()?;

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
                        identity_provider: reaction_identity_provider
                            .get(&(id.clone(), r.id.clone()))
                            .cloned(),
                        config: serde_json::Value::Object(config_map),
                    }
                })
                .collect();

            settings.sources = sources;
            settings.reactions = reactions;
            settings.queries = queries;
            settings.computation = computation;
            instance_configs.push(settings);
        }

        let mut wrapper_config = self.server_configuration.clone();
        wrapper_config.instances = instance_configs;

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
    use crate::api::models::TopLevelBootstrapProviderConfig;
    use crate::config::{PluginDependency, TrustedIdentity};
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
                position_handle: None,
                bootstrap_result_receiver: None,
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
        let default_config = DrasiServerConfig {
            instances: vec![DrasiLibInstanceConfig {
                id: ConfigValue::Static(instance_id.to_string()).into(),
                ..Default::default()
            }],
            ..Default::default()
        };
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
            original_config,
        )
        .expect("valid instance settings")
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
                .from_input("src1")
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
        let sources = val["instances"][0]["sources"].as_sequence().unwrap();
        assert_eq!(sources.len(), 3, "all three sources must be present");

        let ids: Vec<&str> = sources.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"src-a"));
        assert!(ids.contains(&"src-b"));
        assert!(ids.contains(&"src-c"));
    }

    #[tokio::test]
    async fn test_save_preserves_inline_bootstrap_provider() {
        // Regression test for #105: an inline `bootstrapProvider` must survive
        // `save()`. It was previously dropped because it isn't recovered from
        // `snapshot_configuration()`; it is now round-tripped via the tracked
        // `source_bootstrap_provider` map seeded from the original config.
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let original = DrasiServerConfig {
            persist_config: true,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                id: ConfigValue::Static("inst1".to_string()).into(),
                sources: vec![SourceConfig {
                    kind: "postgres".to_string(),
                    id: "src1".to_string(),
                    auto_start: false,
                    identity_provider: None,
                    bootstrap_provider: Some(BootstrapProviderRef::Inline(
                        BootstrapProviderConfig {
                            kind: "postgres".to_string(),
                            config: serde_json::json!({ "host": "db.local", "tables": ["Message"] }),
                        },
                    )),
                    config: serde_json::json!({ "host": "db.local" }),
                }],
                ..Default::default()
            }],
            ..DrasiServerConfig::default()
        };

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "postgres")],
            vec![],
            vec![],
        )
        .await;
        let p = make_persistence_with_config(core, "inst1", cfg_path.clone(), true, &original);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let sources = val["instances"][0]["sources"].as_sequence().unwrap();
        let bp = &sources[0]["bootstrapProvider"];
        assert!(
            bp.is_mapping(),
            "inline bootstrapProvider must be preserved as a mapping, got: {bp:?}"
        );
        assert_eq!(bp["kind"].as_str().unwrap(), "postgres");
        assert_eq!(bp["host"].as_str().unwrap(), "db.local");
        assert_eq!(bp["tables"][0].as_str().unwrap(), "Message");
    }

    #[tokio::test]
    async fn test_save_preserves_toplevel_bootstrap_provider_reference() {
        // #112: a source referencing a top-level `bootstrapProvider` persists as
        // a string reference (NOT re-inlined), and the top-level
        // `bootstrapProviders` block is retained across a save.
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let original = DrasiServerConfig {
            persist_config: true,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                id: ConfigValue::Static("inst1".to_string()).into(),
                bootstrap_providers: vec![TopLevelBootstrapProviderConfig {
                    id: "pg-bootstrap".to_string(),
                    inner: BootstrapProviderConfig {
                        kind: "postgres".to_string(),
                        config: serde_json::json!({ "host": "db.local", "tables": ["Message"] }),
                    },
                }],
                sources: vec![SourceConfig {
                    kind: "postgres".to_string(),
                    id: "src1".to_string(),
                    auto_start: false,
                    identity_provider: None,
                    bootstrap_provider: Some(BootstrapProviderRef::Reference(
                        "pg-bootstrap".to_string(),
                    )),
                    config: serde_json::json!({ "host": "db.local" }),
                }],
                ..Default::default()
            }],
            ..DrasiServerConfig::default()
        };

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "postgres")],
            vec![],
            vec![],
        )
        .await;
        let p = make_persistence_with_config(core, "inst1", cfg_path.clone(), true, &original);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);

        // The source keeps a string reference, not an inlined mapping.
        let bp = &val["instances"][0]["sources"].as_sequence().unwrap()[0]["bootstrapProvider"];
        assert!(
            bp.is_string(),
            "reference must persist as a string, got: {bp:?}"
        );
        assert_eq!(bp.as_str().unwrap(), "pg-bootstrap");

        // The instance's bootstrapProviders block is retained with the entry.
        let providers = val["instances"][0]["bootstrapProviders"]
            .as_sequence()
            .unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0]["id"].as_str().unwrap(), "pg-bootstrap");
        assert_eq!(providers[0]["kind"].as_str().unwrap(), "postgres");
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
                    .from_input("events")
                    .build(),
                Query::cypher("q-beta")
                    .query("MATCH (m:Metric) WHERE m.value > 10 RETURN m")
                    .from_input("metrics")
                    .build(),
            ],
            vec![],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let queries = val["instances"][0]["queries"].as_sequence().unwrap();
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

        // Verify the input binding uses the bare component shorthand
        let inputs = q_alpha["inputs"].as_sequence().unwrap();
        assert_eq!(inputs[0].as_str().unwrap(), "events");
        assert!(q_alpha.get("sources").is_none());
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
                .from_input("src1")
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
        let reactions = val["instances"][0]["reactions"].as_sequence().unwrap();
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

        // Internal source ids start with `__` and must not be persisted.
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
        let sources = val["instances"][0]["sources"].as_sequence().unwrap();
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
                .from_input("src1")
                .build()],
            vec![TestReaction::new("rx1", "log", vec!["q1".into()])],
        )
        .await;

        let p = make_persistence(core, "inst1", cfg_path.clone(), true);
        p.save().await.unwrap();

        let val = read_yaml(&cfg_path);
        let map = val.as_mapping().unwrap();

        let instances = val["instances"].as_sequence().unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0]["id"].as_str(), Some("inst1"));
        for field in ["sources", "queries", "reactions"] {
            assert!(
                !map.contains_key(field),
                "{field} must not be at root level"
            );
            assert_eq!(instances[0][field].as_sequence().unwrap().len(), 1);
        }
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
                    .from_input("src-on")
                    .auto_start(true)
                    .build(),
                Query::cypher("q-off")
                    .query("MATCH (n) RETURN n")
                    .from_input("src-off")
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
        let sources = val["instances"][0]["sources"].as_sequence().unwrap();
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
        let queries = val["instances"][0]["queries"].as_sequence().unwrap();
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
        let reactions = val["instances"][0]["reactions"].as_sequence().unwrap();
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
                .from_input("src1")
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

        assert_eq!(parsed.instances.len(), 1);
        assert_eq!(parsed.instances[0].sources.len(), 1);
        assert_eq!(parsed.instances[0].sources[0].id, "src1");
        assert_eq!(parsed.instances[0].sources[0].kind, "mock");

        assert_eq!(parsed.instances[0].queries.len(), 1);
        assert_eq!(parsed.instances[0].queries[0].id, "q1");
        assert_eq!(
            parsed.instances[0].queries[0].query,
            "MATCH (s:Sensor) WHERE s.temp > 75 RETURN s"
        );
        assert_eq!(
            parsed.instances[0].queries[0].inputs[0],
            crate::api::models::QueryInputConfigDto::Component("src1".to_string())
        );

        assert_eq!(parsed.instances[0].reactions.len(), 1);
        assert_eq!(parsed.instances[0].reactions[0].id, "rx1");
        assert_eq!(parsed.instances[0].reactions[0].kind, "log");
        assert_eq!(
            parsed.instances[0].reactions[0].queries,
            vec!["q1".to_string()]
        );
    }

    #[tokio::test]
    async fn test_save_preserves_server_level_settings() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");

        let core = build_core("settings-inst", vec![], vec![], vec![]).await;

        // Create a config with non-default server-level settings
        let original_config = DrasiServerConfig {
            enable_ui: false,
            plugin_registry: Some("my-registry.io/plugins".to_string()),
            auto_install_plugins: true,
            plugins: vec![PluginDependency {
                reference: "source/postgres:0.5.0".to_string(),
            }],
            verify_plugins: true,
            trusted_identities: vec![TrustedIdentity {
                issuer: "https://accounts.google.com".to_string(),
                subject_pattern: "builder@my-org.iam.gserviceaccount.com".to_string(),
            }],
            hot_reload_plugins: true,
            hot_reload_debounce_ms: 500,
            cors_allowed_origins: vec![
                "http://localhost:3000".to_string(),
                "https://dashboard.example.com".to_string(),
            ],
            instances: vec![DrasiLibInstanceConfig {
                id: ConfigValue::Static("settings-inst".into()).into(),
                ..Default::default()
            }],
            ..Default::default()
        };

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
        assert_eq!(parsed.cors_allowed_origins.len(), 2);
        assert_eq!(parsed.cors_allowed_origins[0], "http://localhost:3000");
        assert_eq!(
            parsed.cors_allowed_origins[1],
            "https://dashboard.example.com"
        );
    }

    #[tokio::test]
    async fn test_save_preserves_single_instance_memory_budget() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");
        let core = build_core("budget-inst", vec![], vec![], vec![]).await;
        let original_config = DrasiServerConfig {
            instances: vec![crate::config::DrasiLibInstanceConfig {
                id: ConfigValue::Static("budget-inst".to_string()).into(),
                persist_index: true,
                memory_budget_mib: Some(ConfigValue::Static(512)),
                ..Default::default()
            }],
            ..Default::default()
        };

        let persistence = make_persistence_with_config(
            core,
            "budget-inst",
            cfg_path.clone(),
            true,
            &original_config,
        );
        let instance_config: DrasiLibInstanceConfig = serde_yaml::from_str(
            r#"
                id: budget-inst
                persistIndex: true
                memoryBudgetMiB: 512
            "#,
        )
        .unwrap();
        persistence
            .register_instance(instance_config)
            .await
            .unwrap();
        persistence.save().await.unwrap();

        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let parsed: DrasiServerConfig = serde_yaml::from_str(&content).unwrap();
        assert!(parsed.instances[0].persist_index);
        assert_eq!(
            parsed.instances[0].memory_budget_mib,
            Some(ConfigValue::Static(512))
        );
    }

    #[tokio::test]
    async fn test_save_preserves_per_instance_memory_budgets() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");
        let first = build_core("first", vec![], vec![], vec![]).await;
        let second = build_core("second", vec![], vec![], vec![]).await;
        let registry = InstanceRegistry::from_map(IndexMap::from([
            ("first".to_string(), first),
            ("second".to_string(), second),
        ]));
        let original_config: DrasiServerConfig = serde_yaml::from_str(
            r#"
                instances:
                  - id: first
                    persistIndex: true
                    memoryBudgetMiB: 128
                  - id: second
                    persistIndex: true
                    memoryBudgetMiB: 256
            "#,
        )
        .unwrap();
        let persistence = ConfigPersistence::new(
            cfg_path.clone(),
            registry,
            "0.0.0.0".to_string(),
            8080,
            "info".to_string(),
            true,
            &original_config,
        )
        .expect("valid instance settings");
        persistence.save().await.unwrap();

        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let parsed: DrasiServerConfig = serde_yaml::from_str(&content).unwrap();
        assert_eq!(parsed.instances.len(), 2);
        assert_eq!(
            parsed.instances[0].memory_budget_mib,
            Some(ConfigValue::Static(128))
        );
        assert_eq!(
            parsed.instances[1].memory_budget_mib,
            Some(ConfigValue::Static(256))
        );
    }

    #[tokio::test]
    async fn test_missing_instance_settings_fail_without_inheriting_another_memory_budget() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");
        let remaining = build_core("remaining", vec![], vec![], vec![]).await;
        let registry =
            InstanceRegistry::from_map(IndexMap::from([("remaining".to_string(), remaining)]));
        let original_config = DrasiServerConfig {
            instances: vec![crate::config::DrasiLibInstanceConfig {
                id: ConfigValue::Static("original".to_string()).into(),
                persist_index: true,
                memory_budget_mib: Some(ConfigValue::Static(512)),
                ..Default::default()
            }],
            ..Default::default()
        };
        let persistence = ConfigPersistence::new(
            cfg_path.clone(),
            registry,
            "0.0.0.0".to_string(),
            8080,
            "info".to_string(),
            true,
            &original_config,
        )
        .expect("valid instance settings");

        let error = persistence.save().await.unwrap_err();
        assert!(error.to_string().contains("remaining"), "{error:#}");
        assert!(!cfg_path.exists());
        persistence
            .register_instance(DrasiLibInstanceConfig {
                id: ConfigValue::Static("remaining".into()).into(),
                ..Default::default()
            })
            .await
            .unwrap();
        persistence.save().await.unwrap();

        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let parsed: DrasiServerConfig = serde_yaml::from_str(&content).unwrap();
        assert!(!parsed.instances[0].persist_index);
        assert_eq!(parsed.instances[0].memory_budget_mib, None);
    }

    #[tokio::test]
    async fn test_registered_instance_does_not_inherit_preserved_memory_budget() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("server.yaml");
        let core = build_core("recreated", vec![], vec![], vec![]).await;
        let registry =
            InstanceRegistry::from_map(IndexMap::from([("recreated".to_string(), core)]));
        let original_config: DrasiServerConfig = serde_yaml::from_str(
            r#"
                instances:
                  - id: recreated
                    persistIndex: true
                    memoryBudgetMiB: 512
            "#,
        )
        .unwrap();
        let persistence = ConfigPersistence::new(
            cfg_path.clone(),
            registry,
            "0.0.0.0".to_string(),
            8080,
            "info".to_string(),
            true,
            &original_config,
        )
        .expect("valid instance settings");
        let recreated_config: DrasiLibInstanceConfig = serde_yaml::from_str(
            r#"
                id: recreated
                persistIndex: false
            "#,
        )
        .unwrap();
        persistence
            .register_instance(recreated_config)
            .await
            .unwrap();

        persistence.save().await.unwrap();

        let content = std::fs::read_to_string(&cfg_path).unwrap();
        let parsed: DrasiServerConfig = serde_yaml::from_str(&content).unwrap();
        assert!(!parsed.instances[0].persist_index);
        assert_eq!(parsed.instances[0].memory_budget_mib, None);
    }

    /// `persist_after_operation` must surface persistence failures to the
    /// caller as `PERSISTENCE_FAILED` with the underlying technical error
    /// in `details.technical_details`. The high-level message must not
    /// embed the raw error string.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_persist_after_operation_surfaces_failure() {
        use crate::api::shared::error::error_codes;
        use crate::api::shared::handlers::persist_after_operation;
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let cfg_dir = tmp.path().join("cfg");
        std::fs::create_dir(&cfg_dir).unwrap();
        let cfg_path = cfg_dir.join("server.yaml");

        let core = build_core(
            "inst1",
            vec![TestSource::new("src1", "mock")],
            vec![Query::cypher("q1")
                .query("MATCH (n) RETURN n")
                .from_input("src1")
                .build()],
            vec![TestReaction::new("rx1", "log", vec!["q1".into()])],
        )
        .await;

        let persistence = Arc::new(make_persistence(core, "inst1", cfg_path, true));

        // Sanity: succeeds when directory is writable.
        persist_after_operation(&Some(persistence.clone()), "test op")
            .await
            .expect("baseline persist should succeed");

        // Make the parent directory read-only so the temp-file write fails.
        let mut perms = std::fs::metadata(&cfg_dir).unwrap().permissions();
        let original_mode = perms.mode();
        perms.set_mode(0o555);
        std::fs::set_permissions(&cfg_dir, perms).unwrap();

        let err = persist_after_operation(&Some(persistence.clone()), "creating source")
            .await
            .expect_err("persist should fail when config dir is read-only");

        // Restore permissions before any assertion can panic so the TempDir
        // can be cleaned up.
        let mut restore = std::fs::metadata(&cfg_dir).unwrap().permissions();
        restore.set_mode(original_mode);
        let _ = std::fs::set_permissions(&cfg_dir, restore);

        assert_eq!(err.code, error_codes::PERSISTENCE_FAILED);
        assert!(
            err.message.contains("creating source")
                && err.message.contains("in memory")
                && err.message.contains("not be persisted"),
            "high-level message should describe the in-memory/on-disk divergence, got: {}",
            err.message
        );
        let details = err
            .details
            .as_ref()
            .expect("technical details should be populated");
        let tech = details
            .technical_details
            .as_ref()
            .expect("technical_details should carry the underlying error");
        assert!(
            !tech.is_empty(),
            "technical_details should contain the underlying error"
        );
        // The underlying error must NOT be embedded in the high-level message.
        assert!(
            !err.message.contains(tech),
            "underlying technical error must not be embedded in `message`"
        );
    }

    /// When persistence is disabled (`None`), `persist_after_operation` is a no-op.
    #[tokio::test]
    async fn test_persist_after_operation_none_is_noop() {
        use crate::api::shared::handlers::persist_after_operation;

        persist_after_operation(&None, "anything")
            .await
            .expect("None config persistence should be Ok");
    }
}
