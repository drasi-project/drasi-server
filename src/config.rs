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
use drasi_server_core::config::DrasiServerCoreConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

/// DrasiServer configuration that composes core config with server settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DrasiServerConfig {
    #[serde(default)]
    pub server: ServerSettings,
    /// Core configuration (sources, queries, reactions)
    #[serde(flatten)]
    pub core_config: DrasiServerCoreConfig,
}

/// Server settings for DrasiServer
/// These control DrasiServer's operational behavior including network binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_disable_persistence")]
    pub disable_persistence: bool,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            disable_persistence: false,
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_disable_persistence() -> bool {
    false
}

/// Validate hostname format according to RFC 1123
/// Hostnames can contain:
/// - letters (a-z, A-Z)
/// - digits (0-9)
/// - hyphens (-)
/// - dots (.) as separators
///
/// Each label (part between dots) must:
/// - start and end with alphanumeric character
/// - be 1-63 characters long
///
/// Total length must be <= 253 characters
fn is_valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 253 {
        return false;
    }

    // Special case: wildcard hostname for binding to all interfaces
    if hostname == "*" {
        return true;
    }

    // Split by dots and validate each label
    let labels: Vec<&str> = hostname.split('.').collect();

    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        // Check if label starts and ends with alphanumeric
        let chars: Vec<char> = label.chars().collect();
        if !chars[0].is_ascii_alphanumeric() || !chars[chars.len() - 1].is_ascii_alphanumeric() {
            return false;
        }

        // Check all characters are valid
        for c in chars {
            if !c.is_ascii_alphanumeric() && c != '-' {
                return false;
            }
        }
    }

    true
}

impl DrasiServerConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let content = fs::read_to_string(path_ref).map_err(|e| {
            anyhow::anyhow!("Failed to read config file {}: {}", path_ref.display(), e)
        })?;

        // Try YAML first, then JSON
        match serde_yaml::from_str::<DrasiServerConfig>(&content) {
            Ok(config) => Ok(config),
            Err(yaml_err) => {
                // If YAML fails, try JSON
                match serde_json::from_str::<DrasiServerConfig>(&content) {
                    Ok(config) => Ok(config),
                    Err(json_err) => {
                        // Both failed, return detailed error
                        Err(anyhow::anyhow!(
                            "Failed to parse config file '{}':\n  YAML error: {}\n  JSON error: {}",
                            path_ref.display(),
                            yaml_err,
                            json_err
                        ))
                    }
                }
            }
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // Validate wrapper-specific settings
        if self.server.port == 0 {
            return Err(anyhow::anyhow!(
                "Invalid server port: {} (cannot be 0)",
                self.server.port
            ));
        }

        if self.server.host.is_empty() {
            return Err(anyhow::anyhow!("Server host cannot be empty"));
        }

        // Validate host format (IP address or hostname)
        // Special cases: localhost, 0.0.0.0 (all interfaces)
        if self.server.host != "localhost"
            && self.server.host != "0.0.0.0"
            && IpAddr::from_str(&self.server.host).is_err()
            && !is_valid_hostname(&self.server.host)
        {
            return Err(anyhow::anyhow!(
                "Invalid server host '{}': must be a valid IP address or hostname",
                self.server.host
            ));
        }

        // Delegate core configuration validation to Core
        self.core_config.validate()
    }
}
