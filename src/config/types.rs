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

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

// Import the config enums from api::models
use crate::api::mappings::{DtoMapper, QueryConfigMapper};
use crate::api::models::{
    ConfigValue, IdentityProviderConfig, QueryConfigDto, ReactionConfig, SecretStoreConfig,
    SourceConfig, StateStoreConfig, TopLevelBootstrapProviderConfig,
};
use drasi_lib::config::QueryConfig;

/// An instance ID retains whether it was supplied by configuration or generated
/// for this load, so managed stores can restore their accepted identity.
#[derive(Debug, Clone)]
pub struct InstanceIdConfig {
    value: ConfigValue<String>,
    generated: bool,
}

impl InstanceIdConfig {
    pub fn is_generated(&self) -> bool {
        self.generated
    }
}

impl AsRef<ConfigValue<String>> for InstanceIdConfig {
    fn as_ref(&self) -> &ConfigValue<String> {
        &self.value
    }
}

impl From<ConfigValue<String>> for InstanceIdConfig {
    fn from(value: ConfigValue<String>) -> Self {
        Self {
            value,
            generated: false,
        }
    }
}

impl Default for InstanceIdConfig {
    fn default() -> Self {
        Self {
            value: ConfigValue::Static(uuid::Uuid::new_v4().to_string()),
            generated: true,
        }
    }
}

impl PartialEq for InstanceIdConfig {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Serialize for InstanceIdConfig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for InstanceIdConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ConfigValue::<String>::deserialize(deserializer).map(Self::from)
    }
}

/// Serde helper: returns `true` for use as `#[serde(default = "default_true")]`.
fn default_true() -> bool {
    true
}

/// DrasiServer configuration
///
/// Server-wide settings and explicit, independently configured DrasiLib instances.
/// An empty instance list starts an empty server; it does not create a default.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = DrasiServerConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrasiServerConfig {
    /// API version marker for file identification (e.g., "drasi.io/v1")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    /// Server bind address
    #[serde(default = "default_host")]
    pub host: ConfigValue<String>,
    /// Server port
    #[serde(default = "default_port")]
    pub port: ConfigValue<u16>,
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: ConfigValue<String>,
    /// Enable automatic persistence of API changes to config file
    #[serde(default = "default_persist_config")]
    pub persist_config: bool,
    /// Enable the web UI at /ui (default: true)
    #[serde(default = "default_enable_ui")]
    pub enable_ui: bool,
    /// Directory containing solution template YAML files (default: "./solutions")
    #[serde(
        default = "default_solutions_dir",
        skip_serializing_if = "Option::is_none"
    )]
    pub solutions_dir: Option<String>,
    /// Default OCI registry for short plugin names (e.g., "ghcr.io/drasi-project")
    #[serde(
        default = "default_plugin_registry",
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_registry: Option<String>,
    /// Automatically download missing plugins from registry on startup
    #[serde(default)]
    pub auto_install_plugins: bool,
    /// Plugin dependencies to install from OCI registry
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<PluginDependency>,
    /// Enable cosign signature verification for downloaded plugins (default: true)
    #[serde(default = "default_true")]
    pub verify_plugins: bool,
    /// Trusted identities for plugin signature verification.
    /// When `verify_plugins` is true and this is omitted, defaults to the drasi-project identity.
    /// When provided, only listed identities are trusted (no implicit default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_identities: Vec<TrustedIdentity>,
    /// Enable filesystem watching for plugin changes (default: false, OFF by default for stability)
    #[serde(default)]
    pub hot_reload_plugins: bool,
    /// Debounce window for filesystem events in milliseconds (default: 2000)
    #[serde(default = "default_hot_reload_debounce_ms")]
    pub hot_reload_debounce_ms: u64,
    /// Allowed CORS origins for the REST API.
    ///
    /// When empty (default), all origins are permitted (`CorsLayer::permissive()`).
    /// When set, only the listed origins are allowed.
    /// Example: `["http://localhost:3000", "https://dashboard.example.com"]`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cors_allowed_origins: Vec<String>,
    /// Optional application-owned OpenTelemetry progress metrics export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<crate::telemetry::MetricsConfig>,
    /// Explicit instances. Omitted or empty lists do not create an implicit instance.
    #[serde(default)]
    pub instances: Vec<DrasiLibInstanceConfig>,
}

impl Default for DrasiServerConfig {
    fn default() -> Self {
        Self {
            api_version: None,
            host: ConfigValue::Static("0.0.0.0".to_string()),
            port: ConfigValue::Static(8080),
            log_level: ConfigValue::Static("info".to_string()),
            persist_config: true,
            enable_ui: true,
            solutions_dir: None,
            plugin_registry: default_plugin_registry(),
            auto_install_plugins: false,
            plugins: Vec::new(),
            verify_plugins: true,
            trusted_identities: Vec::new(),
            hot_reload_plugins: false,
            hot_reload_debounce_ms: 2000,
            cors_allowed_origins: Vec::new(),
            metrics: None,
            instances: Vec::new(),
        }
    }
}

fn default_host() -> ConfigValue<String> {
    ConfigValue::Static("0.0.0.0".to_string())
}

fn default_port() -> ConfigValue<u16> {
    ConfigValue::Static(8080)
}

fn default_log_level() -> ConfigValue<String> {
    ConfigValue::Static("info".to_string())
}

fn default_persist_config() -> bool {
    true
}

fn default_enable_archive() -> bool {
    false
}

fn default_persist_index() -> bool {
    false
}

fn default_hot_reload_debounce_ms() -> u64 {
    2000
}

pub fn default_plugin_registry() -> Option<String> {
    Some("ghcr.io/drasi-project".to_string())
}

/// A plugin dependency declared in the server configuration.
///
/// Specifies a plugin to be installed from an OCI registry.
/// The `ref` field follows OCI image reference format with optional default registry expansion.
///
/// Examples:
/// - `source/postgres` — latest compatible version from default registry
/// - `source/postgres:0.1.8` — exact version
/// - `ghcr.io/acme-corp/custom-source:1.0.0` — third-party registry
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PluginDependency {
    /// OCI image reference (e.g., "source/postgres:0.1.8")
    #[serde(rename = "ref")]
    pub reference: String,
}

/// A trusted identity for cosign plugin signature verification.
///
/// When `verifyPlugins` is enabled, downloaded plugin signatures must match
/// at least one trusted identity. Each identity specifies an OIDC issuer
/// and a subject pattern (glob) to match against the signing certificate.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrustedIdentity {
    /// OIDC issuer URL (must match exactly).
    /// Example: "https://token.actions.githubusercontent.com"
    pub issuer: String,
    /// Glob pattern to match against the certificate subject/SAN.
    /// Example: "https://github.com/drasi-project/*"
    pub subject_pattern: String,
}

fn default_enable_ui() -> bool {
    true
}

fn default_solutions_dir() -> Option<String> {
    None
}

/// Configuration for one explicitly declared DrasiLib instance.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = DrasiLibInstanceConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrasiLibInstanceConfig {
    /// Unique identifier for this DrasiLib instance
    #[serde(default)]
    #[schema(value_type = ConfigValueStringSchema)]
    pub id: InstanceIdConfig,
    /// Enable persistent indexing using RocksDB (default: false uses in-memory indexes)
    #[serde(default = "default_persist_index")]
    pub persist_index: bool,
    /// Enable the archive index for past() functions on this instance's
    /// persistent queries (default: false).
    #[serde(default = "default_enable_archive")]
    pub enable_archive: bool,
    /// Optional RocksDB memory budget for this instance, in MiB
    #[serde(
        default,
        rename = "memoryBudgetMiB",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<ConfigValueUsizeSchema>)]
    pub memory_budget_mib: Option<ConfigValue<usize>>,
    /// Optional state store provider configuration for plugin state persistence
    ///
    /// When set, plugins (Sources, BootstrapProviders, Reactions) can persist
    /// runtime state that survives restarts. If not set, an in-memory state
    /// store is used (state is lost on restart).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_store: Option<StateStoreConfig>,
    /// Optional secret store plugin configuration for resolving ConfigValue::Secret references
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_store: Option<SecretStoreConfig>,
    /// Default priority queue capacity for queries and reactions (default: 10000 if not specified)
    /// Supports environment variables: ${PRIORITY_QUEUE_CAPACITY:-10000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_priority_queue_capacity: Option<ConfigValue<usize>>,
    /// Default dispatch buffer capacity for sources and queries (default: 1000 if not specified)
    /// Supports environment variables: ${DISPATCH_BUFFER_CAPACITY:-1000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_dispatch_buffer_capacity: Option<ConfigValue<usize>>,
    /// Per-component activation deadline in seconds (default: 300).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::api::models::ConfigValueU64Schema>)]
    pub activation_timeout_secs: Option<ConfigValue<u64>>,
    /// Overall instance startup deadline in seconds (default: 600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::api::models::ConfigValueU64Schema>)]
    pub startup_timeout_secs: Option<ConfigValue<u64>>,
    /// Overall stop/shutdown deadline in seconds (default: 60).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::api::models::ConfigValueU64Schema>)]
    pub shutdown_timeout_secs: Option<ConfigValue<u64>>,
    /// Per-component construction/configuration deadline in seconds (default: 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::api::models::ConfigValueU64Schema>)]
    pub construction_timeout_secs: Option<ConfigValue<u64>>,
    /// Per-resource construction deadline in seconds (default: 30).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::api::models::ConfigValueU64Schema>)]
    pub resource_construction_timeout_secs: Option<ConfigValue<u64>>,
    /// Per-owner cleanup deadline in seconds (default: 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<crate::api::models::ConfigValueU64Schema>)]
    pub cleanup_timeout_secs: Option<ConfigValue<u64>>,
    /// Source configurations (parsed into plugin instances)
    #[serde(default)]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub sources: Vec<SourceConfig>,
    /// Query configurations
    #[serde(default)]
    pub queries: Vec<QueryConfigDto>,
    /// Reaction configurations (parsed into plugin instances)
    #[serde(default)]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub reactions: Vec<ReactionConfig>,
    /// Identity provider configurations referenced by sources/reactions via `identityProvider: <id>`.
    #[serde(default)]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub identity_providers: Vec<IdentityProviderConfig>,
    /// Bootstrap provider configurations referenced by sources via `bootstrapProvider: <id>`.
    #[serde(default)]
    #[schema(value_type = Vec<serde_json::Value>)]
    pub bootstrap_providers: Vec<TopLevelBootstrapProviderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computation: Option<crate::computation::ComputationConfigInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_store: Option<crate::managed_configuration::ConfigurationStoreConfig>,
}

/// Resolved instance settings with ConfigValue evaluated
#[derive(Debug, Clone)]
pub struct ResolvedInstanceConfig {
    pub id: String,
    pub persist_index: bool,
    pub enable_archive: bool,
    pub memory_budget_bytes: Option<usize>,
    pub state_store: Option<StateStoreConfig>,
    pub secret_store: Option<SecretStoreConfig>,
    pub default_priority_queue_capacity: Option<usize>,
    pub default_dispatch_buffer_capacity: Option<usize>,
    pub activation_timeout_secs: Option<u64>,
    pub startup_timeout_secs: Option<u64>,
    pub shutdown_timeout_secs: Option<u64>,
    pub construction_timeout_secs: Option<u64>,
    pub resource_construction_timeout_secs: Option<u64>,
    pub cleanup_timeout_secs: Option<u64>,
    pub sources: Vec<SourceConfig>,
    pub queries: Vec<QueryConfig>,
    pub reactions: Vec<ReactionConfig>,
    pub identity_providers: Vec<IdentityProviderConfig>,
    pub bootstrap_providers: Vec<TopLevelBootstrapProviderConfig>,
    pub computation: Option<crate::computation::ComputationConfigInput>,
    pub configuration_store: Option<crate::managed_configuration::ConfigurationStoreConfig>,
}

/// Validate hostname format according to RFC 1123
fn is_valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 253 {
        return false;
    }

    for label in hostname.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        if !label
            .chars()
            .next()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false)
        {
            return false;
        }

        if !label
            .chars()
            .last()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false)
        {
            return false;
        }

        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
    }

    true
}

impl DrasiServerConfig {
    /// Resolve only explicitly configured DrasiLib instances.
    pub fn resolved_instances(&self, mapper: &DtoMapper) -> Result<Vec<ResolvedInstanceConfig>> {
        let mut seen = HashSet::new();
        let mut resolved = Vec::with_capacity(self.instances.len());

        for instance in &self.instances {
            let id: String = mapper.resolve_typed(instance.id.as_ref())?;
            if seen.contains(&id) {
                return Err(anyhow::anyhow!(
                    "Duplicate DrasiLib instance id detected: '{id}'"
                ));
            }
            seen.insert(id.clone());
            if let Some(store) = &instance.configuration_store {
                store.validate()?;
                anyhow::ensure!(
                    instance.sources.is_empty() && instance.queries.is_empty() && instance.reactions.is_empty(),
                    "Instance '{id}': configurationStore requires reconstructible computation definitions; ordinary sources/queries/reactions cannot be adopted automatically"
                );
            }
            if let Some(computation) = instance
                .computation
                .as_ref()
                .filter(|_| instance.configuration_store.is_none())
            {
                computation.validate().with_context(|| {
                    format!("Instance '{id}': invalid computation configuration")
                })?;
            }

            let default_priority_queue_capacity =
                if let Some(capacity) = instance.default_priority_queue_capacity.as_ref() {
                    Some(mapper.resolve_typed(capacity)?)
                } else {
                    None
                };

            let default_dispatch_buffer_capacity =
                if let Some(capacity) = instance.default_dispatch_buffer_capacity.as_ref() {
                    Some(mapper.resolve_typed(capacity)?)
                } else {
                    None
                };

            let memory_budget_mib = if let Some(memory_budget) = instance.memory_budget_mib.as_ref()
            {
                Some(mapper.resolve_typed(memory_budget)?)
            } else {
                None
            };
            let resolve_timeout =
                |name: &str, value: Option<&ConfigValue<u64>>| -> anyhow::Result<Option<u64>> {
                    let value = value
                        .map(|value| mapper.resolve_typed::<u64>(value))
                        .transpose()?;
                    if let Some(seconds) = value {
                        anyhow::ensure!(
                            seconds > 0
                                && std::time::Instant::now()
                                    .checked_add(std::time::Duration::from_secs(seconds))
                                    .is_some(),
                            "Instance '{id}': {name} must be positive and representable"
                        );
                    }
                    Ok(value)
                };
            let activation_timeout_secs = resolve_timeout(
                "activationTimeoutSecs",
                instance.activation_timeout_secs.as_ref(),
            )?;
            let startup_timeout_secs =
                resolve_timeout("startupTimeoutSecs", instance.startup_timeout_secs.as_ref())?;
            let shutdown_timeout_secs = resolve_timeout(
                "shutdownTimeoutSecs",
                instance.shutdown_timeout_secs.as_ref(),
            )?;
            let construction_timeout_secs = resolve_timeout(
                "constructionTimeoutSecs",
                instance.construction_timeout_secs.as_ref(),
            )?;
            let resource_construction_timeout_secs = resolve_timeout(
                "resourceConstructionTimeoutSecs",
                instance.resource_construction_timeout_secs.as_ref(),
            )?;
            let cleanup_timeout_secs =
                resolve_timeout("cleanupTimeoutSecs", instance.cleanup_timeout_secs.as_ref())?;
            let memory_budget_bytes = crate::index_provider::memory_budget_bytes(
                instance.persist_index,
                memory_budget_mib,
            )
            .map_err(|error| anyhow::anyhow!("Instance '{id}': {error}"))?;

            // Map query DTOs to QueryConfig
            let query_mapper = QueryConfigMapper;
            let queries: Vec<QueryConfig> = instance
                .queries
                .iter()
                .map(|dto| mapper.map_with(dto, &query_mapper))
                .collect::<Result<Vec<_>, _>>()?;

            // Validate top-level bootstrap providers and any source references
            // to them so dangling references are caught before startup. The
            // required `id` is enforced structurally by
            // `TopLevelBootstrapProviderConfig`, so only duplicate ids and
            // dangling references need checking here.
            let mut bootstrap_ids: HashSet<String> = HashSet::new();
            for bp in &instance.bootstrap_providers {
                if !bootstrap_ids.insert(bp.id().to_string()) {
                    return Err(anyhow::anyhow!(
                        "Instance '{id}': duplicate bootstrapProvider id '{}'",
                        bp.id()
                    ));
                }
            }
            for src in &instance.sources {
                if let Some(bp_ref) = src.bootstrap_provider().and_then(|r| r.as_reference()) {
                    if !bootstrap_ids.contains(bp_ref) {
                        return Err(anyhow::anyhow!(
                            "Instance '{id}': source '{}' references unknown bootstrapProvider '{bp_ref}'",
                            src.id()
                        ));
                    }
                }
            }

            resolved.push(ResolvedInstanceConfig {
                id,
                persist_index: instance.persist_index,
                enable_archive: instance.enable_archive,
                memory_budget_bytes,
                state_store: instance.state_store.clone(),
                secret_store: instance.secret_store.clone(),
                default_priority_queue_capacity,
                default_dispatch_buffer_capacity,
                activation_timeout_secs,
                startup_timeout_secs,
                shutdown_timeout_secs,
                construction_timeout_secs,
                resource_construction_timeout_secs,
                cleanup_timeout_secs,
                sources: instance.sources.clone(),
                queries,
                reactions: instance.reactions.clone(),
                identity_providers: instance.identity_providers.clone(),
                bootstrap_providers: instance.bootstrap_providers.clone(),
                computation: instance.computation.clone(),
                configuration_store: instance.configuration_store.clone(),
            });
        }

        Ok(resolved)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        use crate::api::mappings::map_server_settings;

        if let Some(metrics) = &self.metrics {
            metrics.validate()?;
        }
        // Resolve server settings to validate them
        let mapper = DtoMapper::new();
        let resolved_settings = map_server_settings(self, &mapper)?;
        // Validate instance layout
        let _ = self.resolved_instances(&mapper)?;

        if !resolved_settings.host.is_empty()
            && resolved_settings.host != "0.0.0.0"
            && !is_valid_hostname(&resolved_settings.host)
            && IpAddr::from_str(&resolved_settings.host).is_err()
        {
            return Err(anyhow::anyhow!(
                "Invalid host '{}': must be a valid hostname or IP address",
                resolved_settings.host
            ));
        }

        if resolved_settings.port == 0 {
            return Err(anyhow::anyhow!(
                "Invalid port 0: port must be between 1 and 65535"
            ));
        }

        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&resolved_settings.log_level.to_lowercase().as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid log level '{}': must be one of trace, debug, info, warn, error",
                resolved_settings.log_level
            ));
        }

        Ok(())
    }

    /// Save configuration to a YAML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        fs::write(path, yaml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_timeout_resolves_and_round_trips_per_instance() {
        let config: DrasiServerConfig = serde_yaml::from_str(
            "instances:\n  - id: long-bootstrap\n    activationTimeoutSecs: ${DRASI_TEST_ACTIVATION_MISSING:-900}\n  - id: default\n",
        ).unwrap();
        let resolved = config.resolved_instances(&DtoMapper::new()).unwrap();
        assert_eq!(resolved[0].activation_timeout_secs, Some(900));
        assert_eq!(resolved[1].activation_timeout_secs, None);
        let saved = serde_json::to_value(&config).unwrap();
        let restored: DrasiServerConfig = serde_json::from_value(saved).unwrap();
        assert_eq!(
            restored.resolved_instances(&DtoMapper::new()).unwrap()[0].activation_timeout_secs,
            Some(900)
        );
    }

    #[test]
    fn lifecycle_timeouts_reject_zero_and_unrepresentable_deadlines() {
        for name in [
            "activationTimeoutSecs",
            "startupTimeoutSecs",
            "shutdownTimeoutSecs",
            "constructionTimeoutSecs",
            "resourceConstructionTimeoutSecs",
            "cleanupTimeoutSecs",
        ] {
            for seconds in [0, u64::MAX] {
                let config: DrasiServerConfig = serde_json::from_value(serde_json::json!({
                    "instances": [{"id": "invalid", (name): seconds}]
                }))
                .unwrap();
                assert!(config
                    .resolved_instances(&DtoMapper::new())
                    .unwrap_err()
                    .to_string()
                    .contains(name));
            }
        }
    }

    #[test]
    fn lifecycle_timeouts_resolve_environment_defaults_and_preserve_omission() {
        let config: DrasiServerConfig = serde_yaml::from_str(
            "instances:\n  - id: configured\n    startupTimeoutSecs: ${DRASI_TEST_STARTUP_MISSING:-601}\n    shutdownTimeoutSecs: ${DRASI_TEST_SHUTDOWN_MISSING:-61}\n    constructionTimeoutSecs: ${DRASI_TEST_CONSTRUCTION_MISSING:-6}\n    resourceConstructionTimeoutSecs: ${DRASI_TEST_RESOURCE_MISSING:-31}\n    cleanupTimeoutSecs: ${DRASI_TEST_CLEANUP_MISSING:-7}\n  - id: default\n"
        ).unwrap();
        let restored: DrasiServerConfig =
            serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
        let resolved = restored.resolved_instances(&DtoMapper::new()).unwrap();
        assert_eq!(resolved[0].startup_timeout_secs, Some(601));
        assert_eq!(resolved[0].shutdown_timeout_secs, Some(61));
        assert_eq!(resolved[0].construction_timeout_secs, Some(6));
        assert_eq!(resolved[0].resource_construction_timeout_secs, Some(31));
        assert_eq!(resolved[0].cleanup_timeout_secs, Some(7));
        assert_eq!(resolved[1].startup_timeout_secs, None);
        assert_eq!(resolved[1].shutdown_timeout_secs, None);
        assert_eq!(resolved[1].construction_timeout_secs, None);
        assert_eq!(resolved[1].resource_construction_timeout_secs, None);
        assert_eq!(resolved[1].cleanup_timeout_secs, None);
    }

    #[test]
    fn missing_ids_are_generated_per_load_and_serialized_ids_are_explicit() {
        let first: DrasiServerConfig = serde_json::from_str(r#"{"instances":[{}]}"#).unwrap();
        let second: DrasiServerConfig = serde_json::from_str(r#"{"instances":[{}]}"#).unwrap();
        assert!(first.instances[0].id.is_generated());
        assert!(second.instances[0].id.is_generated());
        assert_ne!(first.instances[0].id, second.instances[0].id);
        let saved = serde_json::to_value(&first).unwrap();
        let restored: DrasiServerConfig = serde_json::from_value(saved).unwrap();
        assert!(!restored.instances[0].id.is_generated());
        assert_eq!(restored.instances[0].id, first.instances[0].id);
    }

    #[test]
    fn absent_and_empty_instance_lists_do_not_create_an_implicit_instance() {
        for json in ["{}", r#"{"instances":[]}"#] {
            let config: DrasiServerConfig = serde_json::from_str(json).unwrap();
            config.validate().unwrap();
            assert!(config
                .resolved_instances(&DtoMapper::new())
                .unwrap()
                .is_empty());
            let saved = serde_json::to_value(config).unwrap();
            assert_eq!(saved["instances"], serde_json::json!([]));
        }
    }

    #[test]
    fn retired_root_instance_fields_are_rejected_instead_of_converted() {
        for (field, value) in [
            ("id", serde_json::json!("old-instance")),
            ("persistIndex", serde_json::json!(false)),
            ("enableArchive", serde_json::json!(false)),
            ("memoryBudgetMiB", serde_json::json!(128)),
            ("stateStore", serde_json::Value::Null),
            ("secretStore", serde_json::Value::Null),
            ("defaultPriorityQueueCapacity", serde_json::json!(16)),
            ("defaultDispatchBufferCapacity", serde_json::json!(16)),
            ("sources", serde_json::json!([])),
            ("queries", serde_json::json!([])),
            ("reactions", serde_json::json!([])),
            ("identityProviders", serde_json::json!([])),
            ("bootstrapProviders", serde_json::json!([])),
            ("computation", serde_json::Value::Null),
            ("configurationStore", serde_json::Value::Null),
        ] {
            let value = serde_json::json!({field: value, "instances": []});
            let error = serde_json::from_value::<DrasiServerConfig>(value).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("unknown field `{field}`")),
                "{error}"
            );
        }
    }

    // ==================== persist_index tests ====================

    #[test]
    fn test_persist_index_default_is_false() {
        let config = DrasiLibInstanceConfig::default();
        assert!(
            !config.persist_index,
            "persist_index should default to false"
        );
    }

    #[test]
    fn test_persist_index_deserialize_true() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
                persistIndex: true
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            config.instances[0].persist_index,
            "persist_index should be true when explicitly set"
        );
    }

    #[test]
    fn test_persist_index_deserialize_false() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
                persistIndex: false
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            !config.instances[0].persist_index,
            "persist_index should be false when explicitly set"
        );
    }

    #[test]
    fn test_persist_index_defaults_when_omitted() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            !config.instances[0].persist_index,
            "persist_index should default to false when omitted from YAML"
        );
    }

    #[test]
    fn test_memory_budget_mib_defaults_to_none() {
        let config = DrasiLibInstanceConfig::default();
        assert_eq!(config.memory_budget_mib, None);
    }

    #[test]
    fn test_memory_budget_mib_resolves_for_persistent_index() {
        let yaml = r#"
            instances:
              - id: test-server
                persistIndex: true
                memoryBudgetMiB: 512
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        config.validate().unwrap();
        let instances = config.resolved_instances(&DtoMapper::new()).unwrap();

        assert_eq!(instances[0].memory_budget_bytes, Some(512 * 1024 * 1024));
    }

    #[test]
    fn test_memory_budget_mib_supports_environment_defaults() {
        let yaml = r#"
            instances:
              - id: test-server
                persistIndex: true
                memoryBudgetMiB: ${INDEX_MEMORY_MIB:-384}
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        let instances = config.resolved_instances(&DtoMapper::new()).unwrap();

        assert_eq!(instances[0].memory_budget_bytes, Some(384 * 1024 * 1024));
    }

    #[test]
    fn test_memory_budget_mib_requires_persistent_index() {
        let yaml = r#"
            instances:
              - id: test-server
                persistIndex: false
                memoryBudgetMiB: 512
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        let error = config.validate().unwrap_err();

        assert!(error
            .to_string()
            .contains("memoryBudgetMiB requires persistIndex: true"));
    }

    #[test]
    fn test_memory_budget_mib_rejects_zero() {
        let yaml = r#"
            instances:
              - id: test-server
                persistIndex: true
                memoryBudgetMiB: 0
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        let error = config.validate().unwrap_err();

        assert!(error
            .to_string()
            .contains("memoryBudgetMiB must be greater than zero"));
    }

    #[test]
    fn test_memory_budget_mib_roundtrip() {
        let config = DrasiServerConfig {
            instances: vec![crate::config::DrasiLibInstanceConfig {
                persist_index: true,
                memory_budget_mib: Some(ConfigValue::Static(768)),
                ..Default::default()
            }],
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();

        assert!(yaml.contains("memoryBudgetMiB: 768"));
        assert_eq!(
            deserialized.instances[0].memory_budget_mib,
            Some(ConfigValue::Static(768))
        );
    }

    #[test]
    fn test_per_instance_memory_budget_mib() {
        let yaml = r#"
            instances:
              - id: small
                persistIndex: true
                memoryBudgetMiB: 128
              - id: default-budget
                persistIndex: true
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        let instances = config.resolved_instances(&DtoMapper::new()).unwrap();

        assert_eq!(instances[0].memory_budget_bytes, Some(128 * 1024 * 1024));
        assert_eq!(instances[1].memory_budget_bytes, None);
    }

    #[test]
    fn test_root_memory_budget_mib_rejected_with_explicit_instances() {
        let yaml = r#"
            memoryBudgetMiB: 512
            instances:
              - id: persistent
                persistIndex: true
        "#;

        let error = serde_yaml::from_str::<DrasiServerConfig>(yaml).unwrap_err();
        assert!(error
            .to_string()
            .contains("unknown field `memoryBudgetMiB`"));
    }

    #[test]
    fn test_persist_index_serialization_roundtrip_true() {
        let config = DrasiServerConfig {
            api_version: None,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                persist_index: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            yaml.contains("persistIndex: true"),
            "Serialized YAML should contain 'persistIndex: true'"
        );

        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(
            deserialized.instances[0].persist_index,
            "Deserialized config should have persist_index = true"
        );
    }

    #[test]
    fn test_persist_index_serialization_roundtrip_false() {
        let config = DrasiServerConfig {
            api_version: None,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                persist_index: false,
                ..Default::default()
            }],
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(
            !deserialized.instances[0].persist_index,
            "Deserialized config should have persist_index = false"
        );
    }

    #[test]
    fn test_persist_index_with_other_settings() {
        let yaml = r#"
            host: 192.168.1.100
            port: 9090
            logLevel: debug
            persistConfig: false
            instances:
              - id: my-production-server
                persistIndex: true
                sources: []
                queries: []
                reactions: []
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();

        // Verify persist_index is correctly parsed alongside other settings
        assert!(config.instances[0].persist_index);
        assert!(!config.persist_config);
        match &config.log_level {
            ConfigValue::Static(level) => assert_eq!(level, "debug"),
            _ => panic!("Expected static log_level"),
        }
    }

    #[test]
    fn test_default_persist_index_function() {
        assert!(
            !default_persist_index(),
            "default_persist_index() should return false"
        );
    }

    // ==================== persist_config tests ====================

    #[test]
    fn test_persist_config_default_is_true() {
        let config = DrasiServerConfig::default();
        assert!(
            config.persist_config,
            "persist_config should default to true"
        );
    }

    // ==================== cors_allowed_origins tests ====================

    #[test]
    fn test_cors_allowed_origins_defaults_to_empty() {
        let config = DrasiServerConfig::default();
        assert!(
            config.cors_allowed_origins.is_empty(),
            "cors_allowed_origins should default to empty (permissive)"
        );
    }

    #[test]
    fn test_cors_allowed_origins_parsed_from_yaml() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            corsAllowedOrigins:
              - "http://localhost:3000"
              - "https://dashboard.example.com"
            instances:
              - id: test-server
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.cors_allowed_origins.len(), 2);
        assert_eq!(config.cors_allowed_origins[0], "http://localhost:3000");
        assert_eq!(
            config.cors_allowed_origins[1],
            "https://dashboard.example.com"
        );
    }

    #[test]
    fn test_cors_allowed_origins_omitted_is_empty() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.cors_allowed_origins.is_empty());
    }

    // ==================== DrasiServerConfig validation tests ====================

    #[test]
    fn test_config_validation_succeeds_with_persist_index() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            logLevel: info
            instances:
              - id: test-server
                persistIndex: true
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            config.validate().is_ok(),
            "Config with persist_index: true should validate successfully"
        );
    }

    #[test]
    fn test_save_to_file_includes_persist_index() {
        use tempfile::NamedTempFile;

        let config = DrasiServerConfig {
            api_version: None,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                persist_index: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        let temp_file = NamedTempFile::new().unwrap();
        config.save_to_file(temp_file.path()).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(
            content.contains("persistIndex: true"),
            "Saved file should contain persistIndex setting"
        );
    }

    // ==================== state_store tests ====================

    #[test]
    fn test_state_store_default_is_none() {
        let config = DrasiLibInstanceConfig::default();
        assert!(
            config.state_store.is_none(),
            "state_store should default to None"
        );
    }

    #[test]
    fn test_state_store_deserialize_redb() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
                stateStore:
                  kind: redb
                  path: ./data/state.redb
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.instances[0].state_store.is_some());
        let state_store = config.instances[0].state_store.as_ref().unwrap();
        assert_eq!(state_store.kind(), "redb");
    }

    #[test]
    fn test_state_store_defaults_when_omitted() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            config.instances[0].state_store.is_none(),
            "state_store should default to None when omitted from YAML"
        );
    }

    #[test]
    fn test_state_store_serialization_roundtrip() {
        let config = DrasiServerConfig {
            api_version: None,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                state_store: Some(StateStoreConfig::redb("./data/test.redb")),
                ..Default::default()
            }],
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            yaml.contains("stateStore:"),
            "Serialized YAML should contain 'stateStore:'"
        );
        assert!(
            yaml.contains("kind: redb"),
            "Serialized YAML should contain 'kind: redb'"
        );

        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(
            deserialized.instances[0].state_store.is_some(),
            "Deserialized config should have state_store"
        );
        assert_eq!(
            deserialized.instances[0]
                .state_store
                .as_ref()
                .unwrap()
                .kind(),
            "redb"
        );
    }

    #[test]
    fn test_state_store_with_env_var_path() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
                stateStore:
                  kind: redb
                  path: ${STATE_STORE_PATH:-./data/default.redb}
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.instances[0].state_store.is_some());
    }

    #[test]
    fn test_state_store_with_other_settings() {
        let yaml = r#"
            host: 192.168.1.100
            port: 9090
            logLevel: debug
            persistConfig: false
            instances:
              - id: my-production-server
                persistIndex: true
                stateStore:
                  kind: redb
                  path: /var/lib/drasi/state.redb
                sources: []
                queries: []
                reactions: []
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();

        // Verify state_store is correctly parsed alongside other settings
        assert!(config.instances[0].state_store.is_some());
        assert!(config.instances[0].persist_index);
        assert!(!config.persist_config);
    }

    #[test]
    fn test_state_store_in_instance_config() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: instance-1
                persistIndex: true
                stateStore:
                  kind: redb
                  path: ./data/instance1.redb
                sources: []
                queries: []
                reactions: []
              - id: instance-2
                persistIndex: false
                sources: []
                queries: []
                reactions: []
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.instances.len(), 2);

        // First instance has state_store
        assert!(config.instances[0].state_store.is_some());
        assert_eq!(
            config.instances[0].state_store.as_ref().unwrap().kind(),
            "redb"
        );

        // Second instance doesn't have state_store
        assert!(config.instances[1].state_store.is_none());
    }

    #[test]
    fn test_resolved_instance_includes_state_store() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
                stateStore:
                  kind: redb
                  path: ./data/state.redb
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        let mapper = DtoMapper::new();
        let resolved = config.resolved_instances(&mapper).unwrap();

        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].state_store.is_some());
        assert_eq!(resolved[0].state_store.as_ref().unwrap().kind(), "redb");
    }

    #[test]
    fn test_config_validation_succeeds_with_state_store() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            logLevel: info
            instances:
              - id: test-server
                stateStore:
                  kind: redb
                  path: ./data/state.redb
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            config.validate().is_ok(),
            "Config with state_store should validate successfully"
        );
    }

    #[test]
    fn test_save_to_file_includes_state_store() {
        use tempfile::NamedTempFile;

        let config = DrasiServerConfig {
            api_version: None,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                state_store: Some(StateStoreConfig::redb("./data/saved.redb")),
                ..Default::default()
            }],
            ..Default::default()
        };

        let temp_file = NamedTempFile::new().unwrap();
        config.save_to_file(temp_file.path()).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(
            content.contains("stateStore:"),
            "Saved file should contain stateStore setting"
        );
        assert!(
            content.contains("kind: redb"),
            "Saved file should contain kind: redb"
        );
    }

    #[test]
    fn test_save_to_file_omits_none_state_store() {
        use tempfile::NamedTempFile;

        let config = DrasiServerConfig {
            api_version: None,

            instances: vec![crate::config::DrasiLibInstanceConfig {
                state_store: None,
                ..Default::default()
            }],
            ..Default::default()
        };

        let temp_file = NamedTempFile::new().unwrap();
        config.save_to_file(temp_file.path()).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(
            !content.contains("stateStore:"),
            "Saved file should not contain stateStore when None"
        );
    }

    // ==================== plugin registry config tests ====================

    #[test]
    fn test_plugin_registry_defaults() {
        let config = DrasiServerConfig::default();
        assert_eq!(
            config.plugin_registry,
            Some("ghcr.io/drasi-project".to_string())
        );
        assert!(!config.auto_install_plugins);
        assert!(config.plugins.is_empty());
    }

    #[test]
    fn test_plugin_registry_deserialization() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            pluginRegistry: ghcr.io/my-org
            autoInstallPlugins: true
            plugins:
              - ref: source/postgres
              - ref: source/http:0.1.7
              - ref: ghcr.io/acme-corp/custom-source:1.0.0
            instances:
              - id: test-server
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.plugin_registry, Some("ghcr.io/my-org".to_string()));
        assert!(config.auto_install_plugins);
        assert_eq!(config.plugins.len(), 3);
        assert_eq!(config.plugins[0].reference, "source/postgres");
        assert_eq!(config.plugins[1].reference, "source/http:0.1.7");
        assert_eq!(
            config.plugins[2].reference,
            "ghcr.io/acme-corp/custom-source:1.0.0"
        );
    }

    #[test]
    fn test_plugin_registry_serialization_roundtrip() {
        let config = DrasiServerConfig {
            api_version: None,
            plugin_registry: Some("ghcr.io/custom".to_string()),
            auto_install_plugins: true,
            plugins: vec![PluginDependency {
                reference: "source/postgres:0.1.8".to_string(),
            }],
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("pluginRegistry:"));
        assert!(yaml.contains("autoInstallPlugins: true"));
        assert!(yaml.contains("source/postgres:0.1.8"));

        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            deserialized.plugin_registry,
            Some("ghcr.io/custom".to_string())
        );
        assert!(deserialized.auto_install_plugins);
        assert_eq!(deserialized.plugins.len(), 1);
    }

    #[test]
    fn test_plugin_registry_omitted_in_yaml_uses_defaults() {
        let yaml = r#"
            host: 0.0.0.0
            port: 8080
            instances:
              - id: test-server
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.plugin_registry,
            Some("ghcr.io/drasi-project".to_string())
        );
        assert!(!config.auto_install_plugins);
        assert!(config.plugins.is_empty());
    }
}
