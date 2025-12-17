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
use drasi_lib::config::DrasiLibConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

// Import the config enums from api::models
use crate::api::models::{ReactionConfig, SourceConfig, ConfigValue};

/// DrasiServer configuration that composes core config with server settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DrasiServerConfig {
    #[serde(default)]
    pub server: ServerSettings,
    /// Source configurations (DrasiServer-specific, parsed into plugin instances)
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    /// Reaction configurations (DrasiServer-specific, parsed into plugin instances)
    #[serde(default)]
    pub reactions: Vec<ReactionConfig>,
    /// Core configuration (queries, storage backends)
    #[serde(flatten)]
    pub core_config: DrasiLibConfig,
}

/// Server settings for DrasiServer
/// These control DrasiServer's operational behavior including network binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: ConfigValue<String>,
    #[serde(default = "default_port")]
    pub port: ConfigValue<u16>,
    #[serde(default = "default_log_level")]
    pub log_level: ConfigValue<String>,
    #[serde(default = "default_disable_persistence")]
    pub disable_persistence: bool,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: ConfigValue::Static("0.0.0.0".to_string()),
            port: ConfigValue::Static(8080),
            log_level: ConfigValue::Static("info".to_string()),
            disable_persistence: false,
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

        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
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
        let resolved_settings = map_server_settings(&self.server, &mapper)?;
        
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
