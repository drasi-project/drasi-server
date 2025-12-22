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

//! Centralized configuration loading.
//!
//! This module provides the primary interface for loading Drasi Server configuration files.

use super::types::DrasiServerConfig;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

/// Unified error type for configuration operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse YAML: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("Failed to parse JSON: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error(
        "Failed to parse config file '{path}': YAML error: {yaml_err}, JSON error: {json_err}"
    )]
    ParseError {
        path: String,
        yaml_err: String,
        json_err: String,
    },

    #[error("Validation error: {0}")]
    ValidationError(#[from] anyhow::Error),
}

/// Deserialize YAML.
///
/// # Arguments
///
/// * `s` - YAML string
///
/// # Returns
///
/// The deserialized value.
///
/// # Errors
///
/// Returns an error if YAML parsing or deserialization fails.
pub fn from_yaml_str<T: DeserializeOwned>(s: &str) -> Result<T, ConfigError> {
    Ok(serde_yaml::from_str(s)?)
}

/// Deserialize JSON.
///
/// # Arguments
///
/// * `s` - JSON string
///
/// # Returns
///
/// The deserialized value.
///
/// # Errors
///
/// Returns an error if JSON parsing or deserialization fails.
pub fn from_json_str<T: DeserializeOwned>(s: &str) -> Result<T, ConfigError> {
    Ok(serde_json::from_str(s)?)
}

/// Load DrasiServerConfig from a file.
///
/// This is the primary function for loading Drasi Server configuration. It:
/// 1. Reads the file
/// 2. Tries to parse as YAML, falls back to JSON if that fails
/// 3. Validates the configuration
///
/// # Arguments
///
/// * `path` - Path to the configuration file (YAML or JSON)
///
/// # Returns
///
/// A validated `DrasiServerConfig`.
///
/// # Errors
///
/// Returns an error if:
/// - File cannot be read
/// - File is neither valid YAML nor JSON
/// - Configuration validation fails
///
/// # Examples
///
/// ```no_run
/// use drasi_server::config::loader::load_config_file;
///
/// let config = load_config_file("config.yaml").unwrap();
/// println!("Server configuration loaded successfully");
/// ```
pub fn load_config_file<P: AsRef<Path>>(path: P) -> Result<DrasiServerConfig, ConfigError> {
    let path_ref = path.as_ref();
    let content = fs::read_to_string(path_ref)?;

    // Try YAML first, then JSON
    let config = match serde_yaml::from_str::<DrasiServerConfig>(&content) {
        Ok(config) => config,
        Err(yaml_err) => {
            // If YAML fails, try JSON
            match serde_json::from_str::<DrasiServerConfig>(&content) {
                Ok(config) => config,
                Err(json_err) => {
                    return Err(ConfigError::ParseError {
                        path: path_ref.display().to_string(),
                        yaml_err: yaml_err.to_string(),
                        json_err: json_err.to_string(),
                    });
                }
            }
        }
    };

    // Validate the configuration
    config.validate()?;

    Ok(config)
}

/// Save DrasiServerConfig to a file in YAML format.
///
/// # Arguments
///
/// * `config` - The configuration to save
/// * `path` - Path where the configuration file should be written
///
/// # Errors
///
/// Returns an error if:
/// - YAML serialization fails
/// - File cannot be written
///
/// # Examples
///
/// ```no_run
/// use drasi_server::config::loader::{load_config_file, save_config_file};
///
/// let config = load_config_file("config.yaml").unwrap();
/// save_config_file(&config, "config.yaml").unwrap();
/// ```
pub fn save_config_file<P: AsRef<Path>>(
    config: &DrasiServerConfig,
    path: P,
) -> Result<(), ConfigError> {
    let content = serde_yaml::to_string(config)?;
    Ok(fs::write(path, content)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_save_and_load_config_file() {
        let temp_file = NamedTempFile::new().unwrap();

        // Create a config
        let config = DrasiServerConfig {
            host: crate::api::models::ConfigValue::Static("localhost".to_string()),
            port: crate::api::models::ConfigValue::Static(9090),
            ..DrasiServerConfig::default()
        };

        // Save it
        save_config_file(&config, temp_file.path()).unwrap();

        // Load it back
        let loaded_config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(
            loaded_config.host,
            crate::api::models::ConfigValue::Static("localhost".to_string())
        );
        assert_eq!(
            loaded_config.port,
            crate::api::models::ConfigValue::Static(9090)
        );
    }

    #[test]
    fn test_load_basic_config() {
        let config_content = r#"
host: 0.0.0.0
port: 8080
log_level: info
id: test-server-id
sources: []
queries: []
reactions: []
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), config_content).unwrap();

        let config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(
            config.host,
            crate::api::models::ConfigValue::Static("0.0.0.0".to_string())
        );
        assert_eq!(config.port, crate::api::models::ConfigValue::Static(8080));
    }
}
