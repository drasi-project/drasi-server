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

//! Centralized configuration loading with automatic environment variable interpolation.
//!
//! This module provides the primary interface for loading Drasi Server configuration
//! files with transparent environment variable substitution.

use super::env_interpolation;
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

    #[error("Environment variable interpolation failed: {0}")]
    InterpolationError(#[from] env_interpolation::InterpolationError),

    #[error("Failed to parse config file '{path}': YAML error: {yaml_err}, JSON error: {json_err}")]
    ParseError {
        path: String,
        yaml_err: String,
        json_err: String,
    },

    #[error("Validation error: {0}")]
    ValidationError(#[from] anyhow::Error),
}

/// Deserialize YAML with automatic environment variable interpolation.
///
/// This function provides transparent environment variable substitution for any
/// type that implements `Deserialize`. Variables in the format `${VAR_NAME}` or
/// `${VAR_NAME:-default}` are automatically replaced before deserialization.
///
/// # Arguments
///
/// * `s` - YAML string potentially containing environment variable references
///
/// # Returns
///
/// The deserialized value with all environment variables interpolated.
///
/// # Errors
///
/// Returns an error if:
/// - Environment variable interpolation fails (missing required variable)
/// - YAML parsing fails
/// - Deserialization into the target type fails
///
/// # Examples
///
/// ```
/// use drasi_server::config::loader::from_yaml_str;
/// use serde::Deserialize;
/// use std::env;
///
/// #[derive(Deserialize, Debug)]
/// struct Config {
///     host: String,
///     port: u16,
/// }
///
/// env::set_var("APP_HOST", "localhost");
/// env::set_var("APP_PORT", "8080");
///
/// let yaml = r#"
/// host: ${APP_HOST}
/// port: ${APP_PORT}
/// "#;
///
/// let config: Config = from_yaml_str(yaml).unwrap();
/// assert_eq!(config.host, "localhost");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_yaml_str<T: DeserializeOwned>(s: &str) -> Result<T, ConfigError> {
    let interpolated = env_interpolation::interpolate(s)?;
    Ok(serde_yaml::from_str(&interpolated)?)
}

/// Deserialize JSON with automatic environment variable interpolation.
///
/// Similar to `from_yaml_str` but for JSON format.
///
/// # Arguments
///
/// * `s` - JSON string potentially containing environment variable references
///
/// # Returns
///
/// The deserialized value with all environment variables interpolated.
///
/// # Examples
///
/// ```
/// use drasi_server::config::loader::from_json_str;
/// use serde::Deserialize;
/// use std::env;
///
/// #[derive(Deserialize)]
/// struct Config {
///     api_key: String,
/// }
///
/// env::set_var("API_KEY", "secret123");
///
/// let json = r#"{"api_key": "${API_KEY}"}"#;
/// let config: Config = from_json_str(json).unwrap();
/// assert_eq!(config.api_key, "secret123");
/// ```
pub fn from_json_str<T: DeserializeOwned>(s: &str) -> Result<T, ConfigError> {
    let interpolated = env_interpolation::interpolate(s)?;
    Ok(serde_json::from_str(&interpolated)?)
}

/// Load DrasiServerConfig from a file with automatic environment variable interpolation.
///
/// This is the primary function for loading Drasi Server configuration. It:
/// 1. Reads the file
/// 2. Interpolates environment variables
/// 3. Tries to parse as YAML, falls back to JSON if that fails
/// 4. Validates the configuration
///
/// # Arguments
///
/// * `path` - Path to the configuration file (YAML or JSON)
///
/// # Returns
///
/// A validated `DrasiServerConfig` with all environment variables interpolated.
///
/// # Errors
///
/// Returns an error if:
/// - File cannot be read
/// - Environment variable interpolation fails
/// - File is neither valid YAML nor JSON
/// - Configuration validation fails
///
/// # Examples
///
/// ```no_run
/// use drasi_server::config::loader::load_config_file;
///
/// let config = load_config_file("config.yaml").unwrap();
/// println!("Server will bind to {}:{}", config.server.host, config.server.port);
/// ```
pub fn load_config_file<P: AsRef<Path>>(path: P) -> Result<DrasiServerConfig, ConfigError> {
    let path_ref = path.as_ref();
    let content = fs::read_to_string(path_ref)?;

    // Interpolate environment variables first
    let interpolated = env_interpolation::interpolate(&content)?;

    // Try YAML first, then JSON
    let config = match serde_yaml::from_str::<DrasiServerConfig>(&interpolated) {
        Ok(config) => config,
        Err(yaml_err) => {
            // If YAML fails, try JSON
            match serde_json::from_str::<DrasiServerConfig>(&interpolated) {
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
/// Note: This saves the current state of the configuration. Environment variable
/// references are NOT preserved - the actual interpolated values are saved.
/// Users can manually edit the saved file to add `${...}` syntax if desired.
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
/// let mut config = load_config_file("config.yaml").unwrap();
/// config.server.port = 9090;
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
    use std::env;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_yaml_str_simple() {
        use serde::Deserialize;

        #[derive(Deserialize, Debug, PartialEq)]
        struct TestConfig {
            name: String,
            value: i32,
        }

        env::set_var("TEST_NAME", "test");
        env::set_var("TEST_VALUE", "42");

        let yaml = r#"
name: ${TEST_NAME}
value: ${TEST_VALUE}
"#;

        let config: TestConfig = from_yaml_str(yaml).unwrap();
        assert_eq!(
            config,
            TestConfig {
                name: "test".to_string(),
                value: 42
            }
        );
    }

    #[test]
    fn test_from_json_str_simple() {
        use serde::Deserialize;

        #[derive(Deserialize, Debug, PartialEq)]
        struct TestConfig {
            api_key: String,
        }

        env::set_var("TEST_API_KEY", "secret");

        let json = r#"{"api_key": "${TEST_API_KEY}"}"#;

        let config: TestConfig = from_json_str(json).unwrap();
        assert_eq!(
            config,
            TestConfig {
                api_key: "secret".to_string()
            }
        );
    }

    #[test]
    fn test_load_config_file_with_env_vars() {
        env::set_var("TEST_SERVER_HOST", "0.0.0.0");
        env::set_var("TEST_SERVER_PORT", "8080");

        let config_content = r#"
server:
  host: ${TEST_SERVER_HOST}
  port: ${TEST_SERVER_PORT}
  log_level: info
  disable_persistence: false
server_core:
  id: test-server-id
sources: []
queries: []
reactions: []
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), config_content).unwrap();

        let config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
    }

    #[test]
    fn test_load_config_file_with_defaults() {
        env::remove_var("TEST_MISSING_HOST");

        let config_content = r#"
server:
  host: ${TEST_MISSING_HOST:-127.0.0.1}
  port: 8080
  log_level: info
server_core:
  id: test-server-id
sources: []
queries: []
reactions: []
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), config_content).unwrap();

        let config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(config.server.host, "127.0.0.1");
    }

    #[test]
    fn test_load_config_file_missing_required_var() {
        env::remove_var("TEST_REQUIRED_VAR");

        let config_content = r#"
server:
  host: ${TEST_REQUIRED_VAR}
  port: 8080
server_core:
  id: test-server-id
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), config_content).unwrap();

        let result = load_config_file(temp_file.path());

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ConfigError::InterpolationError(_))
        ));
    }

    #[test]
    fn test_save_and_load_config_file() {
        let temp_file = NamedTempFile::new().unwrap();

        // Create a config
        let mut config = DrasiServerConfig::default();
        config.server.host = "localhost".to_string();
        config.server.port = 9090;

        // Save it
        save_config_file(&config, temp_file.path()).unwrap();

        // Load it back
        let loaded_config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(loaded_config.server.host, "localhost");
        assert_eq!(loaded_config.server.port, 9090);
    }

    #[test]
    fn test_transparent_deserialization_with_complex_types() {
        env::set_var("TEST_DB_PASSWORD", "secret123");
        env::set_var("TEST_DB_PORT", "5432");
        // Don't set TEST_DB_HOST to test the default value

        let yaml = r#"
server:
  host: ${TEST_SERVER_HOST:-127.0.0.1}
  port: ${TEST_DB_PORT}
server_core:
  id: test-id
sources:
  - kind: mock
    id: test-source
    data_type: sensor
    interval_ms: 1000
queries: []
reactions: []
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), yaml).unwrap();

        let config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(config.server.host, "0.0.0.0"); // Uses default from ServerSettings
        assert_eq!(config.server.port, 5432); // Uses env var
        assert_eq!(config.sources.len(), 1);
    }

    #[test]
    fn test_backward_compatibility_without_env_vars() {
        // Config without any environment variables should work unchanged
        let config_content = r#"
server:
  host: 0.0.0.0
  port: 8080
  log_level: info
server_core:
  id: test-server-id
sources: []
queries: []
reactions: []
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), config_content).unwrap();

        let config = load_config_file(temp_file.path()).unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
    }
}
