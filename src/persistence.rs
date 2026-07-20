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

use crate::api::models::bootstrap::{BootstrapProviderConfig, BootstrapProviderRef};
use crate::api::models::{ConfigValue, IdentityProviderConfig, QueryConfigDto};
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
    cors_allowed_origins: Vec<String>,
    /// Top-level `identityProviders` from the original single-instance config.
    ///
    /// Identity providers are config-only (they have no runtime ComponentGraph
    /// representation) so they cannot be recovered from `snapshot_configuration()`.
    /// They must be preserved here and re-emitted by `save()`.
    identity_providers: Vec<IdentityProviderConfig>,
    /// Per-instance `identityProviders` keyed by instance id, captured from the
    /// original multi-instance config. Used to repopulate the field for any
    /// instance that was declared statically rather than created dynamically
    /// via `register_instance`.
    identity_providers_by_instance: IndexMap<String, Vec<IdentityProviderConfig>>,
    /// Top-level `bootstrapProviders` from the original single-instance config.
    ///
    /// Like identity providers, top-level bootstrap providers are config-only
    /// (they have no runtime ComponentGraph representation) so they cannot be
    /// recovered from `snapshot_configuration()`. They must be preserved here
    /// and re-emitted by `save()`.
    bootstrap_providers: Vec<BootstrapProviderConfig>,
    /// Per-instance `bootstrapProviders` keyed by instance id, captured from
    /// the original multi-instance config. See `identity_providers_by_instance`.
    bootstrap_providers_by_instance: IndexMap<String, Vec<BootstrapProviderConfig>>,
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
        // Build per-(instance, component) identity-provider lookup maps so
        // `save()` can re-emit `identityProvider: <id>` references that aren't
        // part of `snapshot_configuration()`. We resolve `ConfigValue` ids by
        // their Static form only; env-var / secret ids cannot be matched
        // reliably here so those components silently fall through to the
        // existing (lossy) behaviour.
        let top_level_instance_id = match &original_config.id {
            ConfigValue::Static(s) => Some(s.clone()),
            _ => None,
        };

        let mut source_identity_provider_by_instance: IndexMap<(String, String), String> =
            IndexMap::new();
        let mut reaction_identity_provider_by_instance: IndexMap<(String, String), String> =
            IndexMap::new();
        let mut source_bootstrap_provider_by_instance: IndexMap<(String, String), BootstrapProviderRef> =
            IndexMap::new();

        if let Some(inst_id) = &top_level_instance_id {
            for src in &original_config.sources {
                if let Some(ip) = src.identity_provider() {
                    source_identity_provider_by_instance
                        .insert((inst_id.clone(), src.id.clone()), ip.to_string());
                }
                if let Some(bp) = src.bootstrap_provider() {
                    source_bootstrap_provider_by_instance
                        .insert((inst_id.clone(), src.id.clone()), bp.clone());
                }
            }
            for r in &original_config.reactions {
                if let Some(ip) = r.identity_provider() {
                    reaction_identity_provider_by_instance
                        .insert((inst_id.clone(), r.id.clone()), ip.to_string());
                }
            }
        }

        for inst in &original_config.instances {
            let ConfigValue::Static(inst_id) = &inst.id else {
                continue;
            };
            for src in &inst.sources {
                if let Some(ip) = src.identity_provider() {
                    source_identity_provider_by_instance
                        .insert((inst_id.clone(), src.id.clone()), ip.to_string());
                }
                if let Some(bp) = src.bootstrap_provider() {
                    source_bootstrap_provider_by_instance
                        .insert((inst_id.clone(), src.id.clone()), bp.clone());
                }
            }
            for r in &inst.reactions {
                if let Some(ip) = r.identity_provider() {
                    reaction_identity_provider_by_instance
                        .insert((inst_id.clone(), r.id.clone()), ip.to_string());
                }
            }
        }

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
                cors_allowed_origins: original_config.cors_allowed_origins.clone(),
                identity_providers: original_config.identity_providers.clone(),
                // Seed per-instance identity providers from the original config so
                // they survive a save in multi-instance format. The top-level
                // `identityProviders` block (single-instance format) is folded in
                // under the top-level instance id so that if a save later emits
                // the multi-instance format the providers migrate into
                // `instances[<top-level>].identityProviders` rather than being
                // silently dropped.
                identity_providers_by_instance: {
                    let mut by_instance: IndexMap<String, Vec<IdentityProviderConfig>> =
                        original_config
                            .instances
                            .iter()
                            .filter_map(|inst| match &inst.id {
                                ConfigValue::Static(id) if !inst.identity_providers.is_empty() => {
                                    Some((id.clone(), inst.identity_providers.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                    if !original_config.identity_providers.is_empty() {
                        if let Some(inst_id) = &top_level_instance_id {
                            by_instance
                                .entry(inst_id.clone())
                                .or_insert_with(|| original_config.identity_providers.clone());
                        }
                    }
                    by_instance
                },
                bootstrap_providers: original_config.bootstrap_providers.clone(),
                // Seed per-instance bootstrap providers from the original config
                // so they survive a save in multi-instance format, folding the
                // top-level single-instance block under the top-level instance
                // id. Mirrors `identity_providers_by_instance`.
                bootstrap_providers_by_instance: {
                    let mut by_instance: IndexMap<String, Vec<BootstrapProviderConfig>> =
                        original_config
                            .instances
                            .iter()
                            .filter_map(|inst| match &inst.id {
                                ConfigValue::Static(id) if !inst.bootstrap_providers.is_empty() => {
                                    Some((id.clone(), inst.bootstrap_providers.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                    if !original_config.bootstrap_providers.is_empty() {
                        if let Some(inst_id) = &top_level_instance_id {
                            by_instance
                                .entry(inst_id.clone())
                                .or_insert_with(|| original_config.bootstrap_providers.clone());
                        }
                    }
                    by_instance
                },
            },
            instance_configs: Arc::new(RwLock::new(IndexMap::new())),
            source_identity_provider: Arc::new(RwLock::new(source_identity_provider_by_instance)),
            reaction_identity_provider: Arc::new(RwLock::new(
                reaction_identity_provider_by_instance,
            )),
            source_bootstrap_provider: Arc::new(RwLock::new(source_bootstrap_provider_by_instance)),
        }
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
        if !self.persist_config {
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
        if !self.persist_config {
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
        if !self.persist_config {
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
        if !self.persist_config {
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
        if !self.persist_config {
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
        if !self.persist_config {
            return;
        }
        let mut map = self.source_bootstrap_provider.write().await;
        map.shift_remove(&(instance_id.to_string(), source_id.to_string()));
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
        let source_identity_provider = self.source_identity_provider.read().await;
        let reaction_identity_provider = self.reaction_identity_provider.read().await;
        let source_bootstrap_provider = self.source_bootstrap_provider.read().await;

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
                                        id: None,
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
                .filter_map(|q| match QueryConfigDto::try_from(q.config.clone()) {
                    Ok(dto) => Some(dto),
                    Err(e) => {
                        log::error!("Failed to serialize query '{}' config: {e}", q.id);
                        None
                    }
                })
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
                        identity_provider: reaction_identity_provider
                            .get(&(id.clone(), r.id.clone()))
                            .cloned(),
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
                    secret_store: dynamic_config.secret_store.clone(),
                    default_priority_queue_capacity: dynamic_config
                        .default_priority_queue_capacity
                        .clone(),
                    default_dispatch_buffer_capacity: dynamic_config
                        .default_dispatch_buffer_capacity
                        .clone(),
                    sources,
                    reactions,
                    queries,
                    // Identity providers are config-only and never appear in
                    // `snapshot_configuration()`. Prefer the dynamic config's
                    // list (set when the instance was registered via the API),
                    // and fall back to what the original config declared for
                    // this instance id so static identityProviders survive a
                    // save triggered by an unrelated mutation.
                    identity_providers: if !dynamic_config.identity_providers.is_empty() {
                        dynamic_config.identity_providers.clone()
                    } else {
                        self.preserved
                            .identity_providers_by_instance
                            .get(&id)
                            .cloned()
                            .unwrap_or_default()
                    },
                    bootstrap_providers: if !dynamic_config.bootstrap_providers.is_empty() {
                        dynamic_config.bootstrap_providers.clone()
                    } else {
                        self.preserved
                            .bootstrap_providers_by_instance
                            .get(&id)
                            .cloned()
                            .unwrap_or_default()
                    },
                }
            } else {
                DrasiLibInstanceConfig {
                    id: ConfigValue::Static(snapshot.instance_id.clone()),
                    persist_index,
                    state_store: None,
                    secret_store: None,
                    default_priority_queue_capacity: None,
                    default_dispatch_buffer_capacity: None,
                    sources,
                    reactions,
                    queries,
                    identity_providers: self
                        .preserved
                        .identity_providers_by_instance
                        .get(&id)
                        .cloned()
                        .unwrap_or_default(),
                    bootstrap_providers: self
                        .preserved
                        .bootstrap_providers_by_instance
                        .get(&id)
                        .cloned()
                        .unwrap_or_default(),
                }
            };
            instance_configs.push(instance_config);
        }

        // Dynamic format selection based on instance count
        let wrapper_config = if instance_configs.len() == 1 {
            // Single instance → use single-instance format (root-level fields)
            let instance = instance_configs.remove(0);
            // In single-instance format, identityProviders move to the top
            // level so they read naturally next to the other root component
            // lists. Prefer the (now-promoted) instance value; if none was
            // captured, fall back to the original top-level value.
            let identity_providers = if !instance.identity_providers.is_empty() {
                instance.identity_providers.clone()
            } else {
                self.preserved.identity_providers.clone()
            };
            // Same promotion for bootstrapProviders in single-instance format.
            let bootstrap_providers = if !instance.bootstrap_providers.is_empty() {
                instance.bootstrap_providers.clone()
            } else {
                self.preserved.bootstrap_providers.clone()
            };
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
                secret_store: instance.secret_store,
                default_priority_queue_capacity: instance.default_priority_queue_capacity,
                default_dispatch_buffer_capacity: instance.default_dispatch_buffer_capacity,
                plugin_registry: self.preserved.plugin_registry.clone(),
                auto_install_plugins: self.preserved.auto_install_plugins,
                plugins: self.preserved.plugins.clone(),
                verify_plugins: self.preserved.verify_plugins,
                trusted_identities: self.preserved.trusted_identities.clone(),
                hot_reload_plugins: self.preserved.hot_reload_plugins,
                hot_reload_debounce_ms: self.preserved.hot_reload_debounce_ms,
                cors_allowed_origins: self.preserved.cors_allowed_origins.clone(),
                sources: instance.sources,
                queries: instance.queries,
                reactions: instance.reactions,
                identity_providers,
                bootstrap_providers,
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
                state_store: None,  // Per-instance setting in multi-instance mode
                secret_store: None, // Per-instance setting in multi-instance mode
                default_priority_queue_capacity: None,
                default_dispatch_buffer_capacity: None,
                plugin_registry: self.preserved.plugin_registry.clone(),
                auto_install_plugins: self.preserved.auto_install_plugins,
                plugins: self.preserved.plugins.clone(),
                verify_plugins: self.preserved.verify_plugins,
                trusted_identities: self.preserved.trusted_identities.clone(),
                hot_reload_plugins: self.preserved.hot_reload_plugins,
                hot_reload_debounce_ms: self.preserved.hot_reload_debounce_ms,
                cors_allowed_origins: self.preserved.cors_allowed_origins.clone(),
                sources: Vec::new(),
                queries: Vec::new(),
                reactions: Vec::new(),
                // In multi-instance format, identityProviders live per-instance
                // (inside the `instances` array). Any top-level providers from
                // the original single-instance config are migrated into the
                // top-level instance's `identityProviders` at construction time
                // (see `new()`), so the top-level field stays empty here.
                identity_providers: Vec::new(),
                // Same as identityProviders: bootstrapProviders live per-instance
                // in multi-instance format.
                bootstrap_providers: Vec::new(),
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
                .from_source("src1")
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
