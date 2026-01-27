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

//! Bootstrap provider configuration types for Drasi Server.
//!
//! This module provides configuration types for bootstrap providers with proper
//! validation using `deny_unknown_fields` to catch typos and invalid fields.
//! These types are used for config file parsing and API requests, then converted
//! to drasi-lib types for runtime use.

use serde::{de, de::MapAccess, de::Visitor, Deserialize, Deserializer, Serialize};
use std::fmt;
use utoipa::ToSchema;

// Known bootstrap provider types for error messages
const BOOTSTRAP_PROVIDER_TYPES: &[&str] =
    &["postgres", "application", "scriptfile", "platform", "noop"];

/// PostgreSQL bootstrap provider configuration DTO.
///
/// This provider bootstraps initial data from a PostgreSQL database.
/// No additional configuration is needed - it uses the parent source's connection details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostgresBootstrapConfigDto {
    // Empty - uses parent source config for connection details
    // Struct exists for consistency and future extensibility
}

/// Application bootstrap provider configuration DTO.
///
/// This provider bootstraps data from in-memory storage maintained by application sources.
/// No additional configuration is required.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationBootstrapConfigDto {
    // Empty - uses shared state from application source
    // Struct exists for consistency and future extensibility
}

/// Script file bootstrap provider configuration DTO.
///
/// This provider reads bootstrap data from JSONL (JSON Lines) files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptFileBootstrapConfigDto {
    /// List of JSONL files to read (in order)
    pub file_paths: Vec<String>,
}

/// Platform bootstrap provider configuration DTO.
///
/// This provider bootstraps data from a Query API service in a remote Drasi environment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformBootstrapConfigDto {
    /// URL of the Query API service (e.g., "http://my-source-query-api:8080")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_api_url: Option<String>,

    /// Timeout for HTTP requests in seconds (default: 300)
    #[serde(default = "default_platform_timeout")]
    pub timeout_seconds: u64,
}

fn default_platform_timeout() -> u64 {
    300
}

impl Default for PlatformBootstrapConfigDto {
    fn default() -> Self {
        Self {
            query_api_url: None,
            timeout_seconds: default_platform_timeout(),
        }
    }
}

/// Configuration for bootstrap providers.
///
/// Bootstrap providers handle initial data delivery for newly subscribed queries.
/// This enum uses a custom deserializer to validate unknown fields and provide
/// helpful error messages consistent with other config types.
#[derive(Debug, Clone, Serialize, PartialEq, ToSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BootstrapProviderConfig {
    /// PostgreSQL bootstrap provider - uses parent source's connection config
    Postgres(PostgresBootstrapConfigDto),
    /// Application-based bootstrap provider - uses shared in-memory state
    Application(ApplicationBootstrapConfigDto),
    /// Script file bootstrap provider - reads from JSONL files
    #[serde(rename = "scriptfile")]
    ScriptFile(ScriptFileBootstrapConfigDto),
    /// Platform bootstrap provider - bootstraps from remote Drasi Query API
    Platform(PlatformBootstrapConfigDto),
    /// No-op bootstrap provider - returns no data
    Noop,
}

impl<'de> Deserialize<'de> for BootstrapProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BootstrapProviderConfigVisitor;

        impl<'de> Visitor<'de> for BootstrapProviderConfigVisitor {
            type Value = BootstrapProviderConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a bootstrap provider configuration with 'kind' field")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut provider_kind: Option<String> = None;
                let mut remaining = serde_json::Map::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if provider_kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            provider_kind = Some(map.next_value()?);
                        }
                        other => {
                            let value: serde_json::Value = map.next_value()?;
                            remaining.insert(other.to_string(), value);
                        }
                    }
                }

                let provider_kind =
                    provider_kind.ok_or_else(|| de::Error::missing_field("kind"))?;

                let remaining_value = serde_json::Value::Object(remaining);

                match provider_kind.as_str() {
                    "postgres" => {
                        let config: PostgresBootstrapConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in bootstrapProvider (kind=postgres): {e}"
                                ))
                            })?;
                        Ok(BootstrapProviderConfig::Postgres(config))
                    }
                    "application" => {
                        let config: ApplicationBootstrapConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in bootstrapProvider (kind=application): {e}"
                                ))
                            })?;
                        Ok(BootstrapProviderConfig::Application(config))
                    }
                    "scriptfile" => {
                        let config: ScriptFileBootstrapConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in bootstrapProvider (kind=scriptfile): {e}"
                                ))
                            })?;
                        Ok(BootstrapProviderConfig::ScriptFile(config))
                    }
                    "platform" => {
                        let config: PlatformBootstrapConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in bootstrapProvider (kind=platform): {e}"
                                ))
                            })?;
                        Ok(BootstrapProviderConfig::Platform(config))
                    }
                    "noop" => {
                        // Noop should have no extra fields
                        if !remaining_value.as_object().is_none_or(|m| m.is_empty()) {
                            return Err(de::Error::custom(
                                "in bootstrapProvider (kind=noop): noop provider accepts no additional fields"
                            ));
                        }
                        Ok(BootstrapProviderConfig::Noop)
                    }
                    unknown => Err(de::Error::custom(format!(
                        "unknown bootstrap provider kind '{unknown}'. Valid kinds are: {}",
                        BOOTSTRAP_PROVIDER_TYPES.join(", ")
                    ))),
                }
            }
        }

        deserializer.deserialize_map(BootstrapProviderConfigVisitor)
    }
}

// Conversion to drasi-lib types for runtime use

impl From<&ScriptFileBootstrapConfigDto> for drasi_lib::bootstrap::ScriptFileBootstrapConfig {
    fn from(dto: &ScriptFileBootstrapConfigDto) -> Self {
        drasi_lib::bootstrap::ScriptFileBootstrapConfig {
            file_paths: dto.file_paths.clone(),
        }
    }
}

impl From<&PlatformBootstrapConfigDto> for drasi_lib::bootstrap::PlatformBootstrapConfig {
    fn from(dto: &PlatformBootstrapConfigDto) -> Self {
        drasi_lib::bootstrap::PlatformBootstrapConfig {
            query_api_url: dto.query_api_url.clone(),
            timeout_seconds: dto.timeout_seconds,
        }
    }
}

impl From<&PostgresBootstrapConfigDto> for drasi_lib::bootstrap::PostgresBootstrapConfig {
    fn from(_dto: &PostgresBootstrapConfigDto) -> Self {
        drasi_lib::bootstrap::PostgresBootstrapConfig {}
    }
}

impl From<&ApplicationBootstrapConfigDto> for drasi_lib::bootstrap::ApplicationBootstrapConfig {
    fn from(_dto: &ApplicationBootstrapConfigDto) -> Self {
        drasi_lib::bootstrap::ApplicationBootstrapConfig {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_bootstrap_config_valid() {
        let json = r#"{"kind": "postgres"}"#;
        let config: BootstrapProviderConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, BootstrapProviderConfig::Postgres(_)));
    }

    #[test]
    fn test_postgres_bootstrap_config_unknown_field() {
        let json = r#"{"kind": "postgres", "unknownField": "value"}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Expected 'unknown field' error, got: {err}"
        );
    }

    #[test]
    fn test_application_bootstrap_config_valid() {
        let json = r#"{"kind": "application"}"#;
        let config: BootstrapProviderConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, BootstrapProviderConfig::Application(_)));
    }

    #[test]
    fn test_application_bootstrap_config_unknown_field() {
        let json = r#"{"kind": "application", "extraField": 123}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Expected 'unknown field' error, got: {err}"
        );
    }

    #[test]
    fn test_scriptfile_bootstrap_config_valid() {
        let json = r#"{"kind": "scriptfile", "filePaths": ["/data/test.jsonl"]}"#;
        let config: BootstrapProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            BootstrapProviderConfig::ScriptFile(cfg) => {
                assert_eq!(cfg.file_paths, vec!["/data/test.jsonl"]);
            }
            _ => panic!("Expected ScriptFile variant"),
        }
    }

    #[test]
    fn test_scriptfile_bootstrap_config_unknown_field() {
        let json =
            r#"{"kind": "scriptfile", "filePaths": ["/test.jsonl"], "unknownField": "value"}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Expected 'unknown field' error, got: {err}"
        );
    }

    #[test]
    fn test_scriptfile_bootstrap_config_typo_file_path() {
        // Common typo: file_path instead of filePaths
        let json = r#"{"kind": "scriptfile", "file_path": "/test.jsonl"}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // Should catch the typo
        assert!(
            err.contains("unknown field") || err.contains("missing field"),
            "Expected field error, got: {err}"
        );
    }

    #[test]
    fn test_platform_bootstrap_config_valid() {
        let json =
            r#"{"kind": "platform", "queryApiUrl": "http://test:8080", "timeoutSeconds": 600}"#;
        let config: BootstrapProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            BootstrapProviderConfig::Platform(cfg) => {
                assert_eq!(cfg.query_api_url, Some("http://test:8080".to_string()));
                assert_eq!(cfg.timeout_seconds, 600);
            }
            _ => panic!("Expected Platform variant"),
        }
    }

    #[test]
    fn test_platform_bootstrap_config_defaults() {
        let json = r#"{"kind": "platform"}"#;
        let config: BootstrapProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            BootstrapProviderConfig::Platform(cfg) => {
                assert_eq!(cfg.query_api_url, None);
                assert_eq!(cfg.timeout_seconds, 300); // default
            }
            _ => panic!("Expected Platform variant"),
        }
    }

    #[test]
    fn test_platform_bootstrap_config_unknown_field() {
        let json = r#"{"kind": "platform", "queryApiUrl": "http://test:8080", "typoField": 123}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Expected 'unknown field' error, got: {err}"
        );
    }

    #[test]
    fn test_noop_bootstrap_config_valid() {
        let json = r#"{"kind": "noop"}"#;
        let config: BootstrapProviderConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, BootstrapProviderConfig::Noop));
    }

    #[test]
    fn test_noop_bootstrap_config_with_extra_fields() {
        let json = r#"{"kind": "noop", "unexpectedField": "value"}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("noop provider accepts no additional fields"),
            "Expected noop field error, got: {err}"
        );
    }

    #[test]
    fn test_unknown_bootstrap_type() {
        let json = r#"{"kind": "unknown"}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown bootstrap provider kind 'unknown'"),
            "Expected unknown type error, got: {err}"
        );
        assert!(
            err.contains("postgres"),
            "Error should list valid types, got: {err}"
        );
    }

    #[test]
    fn test_missing_type_field() {
        let json = r#"{"filePaths": ["/test.jsonl"]}"#;
        let result: Result<BootstrapProviderConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing field `kind`"),
            "Expected missing field error, got: {err}"
        );
    }

    #[test]
    fn test_yaml_deserialization() {
        let yaml = r#"
kind: scriptfile
filePaths:
  - /data/file1.jsonl
  - /data/file2.jsonl
"#;
        let config: BootstrapProviderConfig = serde_yaml::from_str(yaml).unwrap();
        match config {
            BootstrapProviderConfig::ScriptFile(cfg) => {
                assert_eq!(cfg.file_paths.len(), 2);
                assert_eq!(cfg.file_paths[0], "/data/file1.jsonl");
            }
            _ => panic!("Expected ScriptFile variant"),
        }
    }

    #[test]
    fn test_conversion_to_drasi_lib_scriptfile() {
        let dto = ScriptFileBootstrapConfigDto {
            file_paths: vec!["/test1.jsonl".to_string(), "/test2.jsonl".to_string()],
        };
        let lib_config: drasi_lib::bootstrap::ScriptFileBootstrapConfig = (&dto).into();
        assert_eq!(lib_config.file_paths, dto.file_paths);
    }

    #[test]
    fn test_conversion_to_drasi_lib_platform() {
        let dto = PlatformBootstrapConfigDto {
            query_api_url: Some("http://test:8080".to_string()),
            timeout_seconds: 600,
        };
        let lib_config: drasi_lib::bootstrap::PlatformBootstrapConfig = (&dto).into();
        assert_eq!(lib_config.query_api_url, dto.query_api_url);
        assert_eq!(lib_config.timeout_seconds, dto.timeout_seconds);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = BootstrapProviderConfig::ScriptFile(ScriptFileBootstrapConfigDto {
            file_paths: vec!["/test.jsonl".to_string()],
        });
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"kind\":\"scriptfile\""));
        assert!(json.contains("\"filePaths\""));

        let deserialized: BootstrapProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }
}
