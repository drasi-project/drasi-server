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
use std::path::Path;

/// DrasiServer configuration that composes core config with API settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DrasiServerConfig {
    #[serde(default)]
    pub api: ApiSettings,
    #[serde(default)]
    pub server: ServerSettings,
    /// Core configuration (sources, queries, reactions)
    #[serde(flatten)]
    pub core_config: DrasiServerCoreConfig,
}

/// Server settings for DrasiServer
/// These control DrasiServer's operational behavior like logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_disable_persistence")]
    pub disable_persistence: bool,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            disable_persistence: false,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_disable_persistence() -> bool {
    false
}

/// API server settings for Drasi Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
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
        if self.api.port == 0 {
            return Err(anyhow::anyhow!(
                "Invalid API port: {} (cannot be 0)",
                self.api.port
            ));
        }

        if self.api.host.is_empty() {
            return Err(anyhow::anyhow!("API host cannot be empty"));
        }

        // Delegate core configuration validation to Core
        self.core_config.validate()
    }
}
