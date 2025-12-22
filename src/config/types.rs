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
use drasi_lib::config::QueryConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

// Import the config enums from api::models
use crate::api::models::{ConfigValue, ReactionConfig, SourceConfig};

/// DrasiServer configuration
///
/// This is a self-contained configuration struct that includes all settings
/// needed to run a DrasiServer. The `id`, `default_priority_queue_capacity`,
/// `default_dispatch_buffer_capacity`, and `queries` fields are used to construct
/// a DrasiLibConfig when creating a DrasiLib instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrasiServerConfig {
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
    /// Disable automatic persistence of API changes to config file
    #[serde(default = "default_disable_persistence")]
    pub disable_persistence: bool,
    /// Default priority queue capacity for queries and reactions (default: 10000 if not specified)
    /// Supports environment variables: ${PRIORITY_QUEUE_CAPACITY:-10000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_priority_queue_capacity: Option<ConfigValue<usize>>,
    /// Default dispatch buffer capacity for sources and queries (default: 1000 if not specified)
    /// Supports environment variables: ${DISPATCH_BUFFER_CAPACITY:-1000}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_dispatch_buffer_capacity: Option<ConfigValue<usize>>,
    /// Source configurations (parsed into plugin instances)
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    /// Query configurations
    #[serde(default)]
    pub queries: Vec<QueryConfig>,
    /// Reaction configurations (parsed into plugin instances)
    #[serde(default)]
    pub reactions: Vec<ReactionConfig>,
}

impl Default for DrasiServerConfig {
    fn default() -> Self {
        Self {
            id: default_id(),
            host: ConfigValue::Static("0.0.0.0".to_string()),
            port: ConfigValue::Static(8080),
            log_level: ConfigValue::Static("info".to_string()),
            disable_persistence: false,
            default_priority_queue_capacity: None,
            default_dispatch_buffer_capacity: None,
            sources: Vec::new(),
            reactions: Vec::new(),
            queries: Vec::new(),
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

fn default_disable_persistence() -> bool {
    false
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
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        use crate::api::mappings::{map_server_settings, DtoMapper};

        // Resolve server settings to validate them
        let mapper = DtoMapper::new();
        let resolved_settings = map_server_settings(self, &mapper)?;

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
