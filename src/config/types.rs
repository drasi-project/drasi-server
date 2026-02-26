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

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

// Import the config enums from api::models
use crate::api::mappings::{DtoMapper, QueryConfigMapper};
use crate::api::models::{
    ConfigValue, LogReactionConfigDto, MockSourceConfigDto, QueryConfigDto, ReactionConfig,
    SourceConfig, StateStoreConfig,
};
use drasi_lib::config::QueryConfig;

/// DrasiServer configuration
///
/// This is a self-contained configuration struct that includes all settings
/// needed to run a DrasiServer. The `id`, `default_priority_queue_capacity`,
/// `default_dispatch_buffer_capacity`, and `queries` fields are used to construct
/// a DrasiLibConfig when creating a DrasiLib instance.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = DrasiServerConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrasiServerConfig {
    /// API version marker for file identification (e.g., "drasi.io/v1")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    /// Unique identifier for this server instance (defaults to UUID)
    #[serde(default = "default_id")]
    pub id: ConfigValue<String>,
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
    /// Enable persistent indexing using RocksDB (default: false uses in-memory indexes)
    #[serde(default = "default_persist_index")]
    pub persist_index: bool,
    /// Optional state store provider configuration for plugin state persistence
    ///
    /// When set, plugins (Sources, BootstrapProviders, Reactions) can persist
    /// runtime state that survives restarts. If not set, an in-memory state
    /// store is used (state is lost on restart).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_store: Option<StateStoreConfig>,
    /// Default priority queue capacity for queries and reactions (default: 10000 if not specified)
    /// Supports environment variables: ${PRIORITY_QUEUE_CAPACITY:-10000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_priority_queue_capacity: Option<ConfigValue<usize>>,
    /// Default dispatch buffer capacity for sources and queries (default: 1000 if not specified)
    /// Supports environment variables: ${DISPATCH_BUFFER_CAPACITY:-1000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_dispatch_buffer_capacity: Option<ConfigValue<usize>>,
    /// Identity provider configurations for authentication
    ///
    /// Define reusable identity providers that can be referenced by sources and reactions.
    /// Supports password-based, Azure AD, and AWS IAM authentication.
    #[serde(default)]
    pub identity_providers: Vec<IdentityProviderConfig>,
    /// Source configurations (parsed into plugin instances)
    #[serde(default)]
    #[schema(value_type = Vec<MockSourceConfigDto>)]
    pub sources: Vec<SourceConfig>,
    /// Query configurations
    #[serde(default)]
    pub queries: Vec<QueryConfigDto>,
    /// Reaction configurations (parsed into plugin instances)
    #[serde(default)]
    #[schema(value_type = Vec<LogReactionConfigDto>)]
    pub reactions: Vec<ReactionConfig>,
    /// Optional list of DrasiLib instances when running in multi-tenant mode
    #[serde(default)]
    pub instances: Vec<DrasiLibInstanceConfig>,
}

impl Default for DrasiServerConfig {
    fn default() -> Self {
        Self {
            api_version: None,
            id: default_id(),
            host: ConfigValue::Static("0.0.0.0".to_string()),
            port: ConfigValue::Static(8080),
            log_level: ConfigValue::Static("info".to_string()),
            persist_config: true,
            persist_index: false,
            state_store: None,
            default_priority_queue_capacity: None,
            default_dispatch_buffer_capacity: None,
            identity_providers: Vec::new(),
            sources: Vec::new(),
            reactions: Vec::new(),
            queries: Vec::new(),
            instances: Vec::new(),
        }
    }
}

fn default_id() -> ConfigValue<String> {
    ConfigValue::Static(uuid::Uuid::new_v4().to_string())
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

fn default_persist_index() -> bool {
    false
}

/// Configuration for a single DrasiLib instance (multi-instance mode)
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[schema(as = DrasiLibInstanceConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrasiLibInstanceConfig {
    /// Unique identifier for this DrasiLib instance
    #[serde(default = "default_id")]
    pub id: ConfigValue<String>,
    /// Enable persistent indexing using RocksDB (default: false uses in-memory indexes)
    #[serde(default = "default_persist_index")]
    pub persist_index: bool,
    /// Optional state store provider configuration for plugin state persistence
    ///
    /// When set, plugins (Sources, BootstrapProviders, Reactions) can persist
    /// runtime state that survives restarts. If not set, an in-memory state
    /// store is used (state is lost on restart).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_store: Option<StateStoreConfig>,
    /// Default priority queue capacity for queries and reactions (default: 10000 if not specified)
    /// Supports environment variables: ${PRIORITY_QUEUE_CAPACITY:-10000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_priority_queue_capacity: Option<ConfigValue<usize>>,
    /// Default dispatch buffer capacity for sources and queries (default: 1000 if not specified)
    /// Supports environment variables: ${DISPATCH_BUFFER_CAPACITY:-1000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_dispatch_buffer_capacity: Option<ConfigValue<usize>>,
    /// Identity provider configurations for authentication
    ///
    /// Define reusable identity providers that can be referenced by sources and reactions.
    /// Supports password-based, Azure AD, and AWS IAM authentication.
    #[serde(default)]
    pub identity_providers: Vec<IdentityProviderConfig>,
    /// Source configurations (parsed into plugin instances)
    #[serde(default)]
    #[schema(value_type = Vec<MockSourceConfigDto>)]
    pub sources: Vec<SourceConfig>,
    /// Query configurations
    #[serde(default)]
    pub queries: Vec<QueryConfigDto>,
    /// Reaction configurations (parsed into plugin instances)
    #[serde(default)]
    #[schema(value_type = Vec<LogReactionConfigDto>)]
    pub reactions: Vec<ReactionConfig>,
}

/// Resolved instance settings with ConfigValue evaluated
#[derive(Debug, Clone)]
pub struct ResolvedInstanceConfig {
    pub id: String,
    pub persist_index: bool,
    pub state_store: Option<StateStoreConfig>,
    pub default_priority_queue_capacity: Option<usize>,
    pub default_dispatch_buffer_capacity: Option<usize>,
    pub identity_providers: Vec<IdentityProviderConfig>,
    pub sources: Vec<SourceConfig>,
    pub queries: Vec<QueryConfig>,
    pub reactions: Vec<ReactionConfig>,
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
    /// Resolve configured DrasiLib instances, supporting single-instance and multi-instance layout.
    pub fn resolved_instances(&self, mapper: &DtoMapper) -> Result<Vec<ResolvedInstanceConfig>> {
        let raw_instances: Vec<DrasiLibInstanceConfig> = if self.instances.is_empty() {
            vec![DrasiLibInstanceConfig {
                id: self.id.clone(),
                persist_index: self.persist_index,
                state_store: self.state_store.clone(),
                default_priority_queue_capacity: self.default_priority_queue_capacity.clone(),
                default_dispatch_buffer_capacity: self.default_dispatch_buffer_capacity.clone(),
                identity_providers: self.identity_providers.clone(),
                sources: self.sources.clone(),
                queries: self.queries.clone(),
                reactions: self.reactions.clone(),
            }]
        } else {
            self.instances.clone()
        };

        let mut seen = HashSet::new();
        let mut resolved = Vec::with_capacity(raw_instances.len());

        for instance in raw_instances {
            let id: String = mapper.resolve_typed(&instance.id)?;
            if seen.contains(&id) {
                return Err(anyhow::anyhow!(
                    "Duplicate DrasiLib instance id detected: '{id}'"
                ));
            }
            seen.insert(id.clone());

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

            // Map query DTOs to QueryConfig
            let query_mapper = QueryConfigMapper;
            let queries: Vec<QueryConfig> = instance
                .queries
                .iter()
                .map(|dto| mapper.map_with(dto, &query_mapper))
                .collect::<Result<Vec<_>, _>>()?;

            resolved.push(ResolvedInstanceConfig {
                id,
                persist_index: instance.persist_index,
                state_store: instance.state_store.clone(),
                default_priority_queue_capacity,
                default_dispatch_buffer_capacity,
                identity_providers: instance.identity_providers.clone(),
                sources: instance.sources.clone(),
                queries,
                reactions: instance.reactions.clone(),
            });
        }

        if resolved.is_empty() {
            return Err(anyhow::anyhow!(
                "At least one DrasiLib instance must be configured"
            ));
        }

        Ok(resolved)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        use crate::api::mappings::map_server_settings;

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

    // ==================== persist_index tests ====================

    #[test]
    fn test_persist_index_default_is_false() {
        let config = DrasiServerConfig::default();
        assert!(
            !config.persist_index,
            "persist_index should default to false"
        );
    }

    #[test]
    fn test_persist_index_deserialize_true() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            persistIndex: true
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            config.persist_index,
            "persist_index should be true when explicitly set"
        );
    }

    #[test]
    fn test_persist_index_deserialize_false() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            persistIndex: false
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            !config.persist_index,
            "persist_index should be false when explicitly set"
        );
    }

    #[test]
    fn test_persist_index_defaults_when_omitted() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            !config.persist_index,
            "persist_index should default to false when omitted from YAML"
        );
    }

    #[test]
    fn test_persist_index_serialization_roundtrip_true() {
        let config = DrasiServerConfig {
            api_version: None,
            persist_index: true,
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(
            yaml.contains("persistIndex: true"),
            "Serialized YAML should contain 'persistIndex: true'"
        );

        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(
            deserialized.persist_index,
            "Deserialized config should have persist_index = true"
        );
    }

    #[test]
    fn test_persist_index_serialization_roundtrip_false() {
        let config = DrasiServerConfig {
            api_version: None,
            persist_index: false,
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let deserialized: DrasiServerConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(
            !deserialized.persist_index,
            "Deserialized config should have persist_index = false"
        );
    }

    #[test]
    fn test_persist_index_with_other_settings() {
        let yaml = r#"
            id: my-production-server
            host: 192.168.1.100
            port: 9090
            logLevel: debug
            persistConfig: false
            persistIndex: true
            sources: []
            queries: []
            reactions: []
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();

        // Verify persist_index is correctly parsed alongside other settings
        assert!(config.persist_index);
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

    // ==================== DrasiServerConfig validation tests ====================

    #[test]
    fn test_config_validation_succeeds_with_persist_index() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            logLevel: info
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
            persist_index: true,
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
        let config = DrasiServerConfig::default();
        assert!(
            config.state_store.is_none(),
            "state_store should default to None"
        );
    }

    #[test]
    fn test_state_store_deserialize_redb() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            stateStore:
              kind: redb
              path: ./data/state.redb
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.state_store.is_some());
        let state_store = config.state_store.unwrap();
        assert_eq!(state_store.kind(), "redb");
    }

    #[test]
    fn test_state_store_defaults_when_omitted() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(
            config.state_store.is_none(),
            "state_store should default to None when omitted from YAML"
        );
    }

    #[test]
    fn test_state_store_serialization_roundtrip() {
        let config = DrasiServerConfig {
            api_version: None,
            state_store: Some(StateStoreConfig::redb("./data/test.redb")),
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
            deserialized.state_store.is_some(),
            "Deserialized config should have state_store"
        );
        assert_eq!(deserialized.state_store.as_ref().unwrap().kind(), "redb");
    }

    #[test]
    fn test_state_store_with_env_var_path() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            stateStore:
              kind: redb
              path: ${STATE_STORE_PATH:-./data/default.redb}
        "#;

        let config: DrasiServerConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.state_store.is_some());
    }

    #[test]
    fn test_state_store_with_other_settings() {
        let yaml = r#"
            id: my-production-server
            host: 192.168.1.100
            port: 9090
            logLevel: debug
            persistConfig: false
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
        assert!(config.state_store.is_some());
        assert!(config.persist_index);
        assert!(!config.persist_config);
    }

    #[test]
    fn test_state_store_in_instance_config() {
        let yaml = r#"
            id: multi-instance-server
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
            id: test-server
            host: 0.0.0.0
            port: 8080
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
            id: test-server
            host: 0.0.0.0
            port: 8080
            logLevel: info
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
            state_store: Some(StateStoreConfig::redb("./data/saved.redb")),
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
            state_store: None,
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
}
