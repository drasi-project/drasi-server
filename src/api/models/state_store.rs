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

//! State store configuration DTOs.

use drasi_plugin_sdk::config_value::ConfigValue;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// State store configuration with kind discriminator.
///
/// State store providers allow plugins (Sources, BootstrapProviders, and Reactions)
/// to persist runtime state that survives restarts of DrasiLib.
///
/// Uses a custom deserializer to handle the `kind` field and validate unknown fields.
/// The inner config DTOs use `#[serde(deny_unknown_fields)]` to catch typos.
///
/// # Example YAML
///
/// ```yaml
/// stateStore:
///   kind: redb
///   path: ./data/state.redb
/// ```
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
#[serde(rename_all = "camelCase")]
pub enum StateStoreConfig {
    /// REDB-based state store for persistent storage
    ///
    /// Uses redb embedded database for file-based persistence.
    /// Data survives restarts and is stored in a single file.
    #[serde(rename = "redb")]
    Redb {
        /// Path to the redb database file
        ///
        /// Supports environment variables: ${STATE_STORE_PATH:-./data/state.redb}
        path: ConfigValue<String>,
    },
}

/// Inner configuration DTO for REDB state store with strict field validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = RedbStateStoreConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RedbStateStoreConfigDto {
    /// Path to the redb database file
    pub path: ConfigValue<String>,
}

// Known state store kinds for error messages
const STATE_STORE_KINDS: &[&str] = &["redb"];

impl<'de> Deserialize<'de> for StateStoreConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StateStoreConfigVisitor;

        impl<'de> Visitor<'de> for StateStoreConfigVisitor {
            type Value = StateStoreConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a state store configuration with 'kind' field")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // Storage for the kind field
                let mut kind: Option<String> = None;

                // Collect remaining fields for the inner config
                let mut remaining = serde_json::Map::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        // Collect all other fields for the inner config
                        other => {
                            let value: serde_json::Value = map.next_value()?;
                            remaining.insert(other.to_string(), value);
                        }
                    }
                }

                // Validate required fields
                let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;

                let remaining_value = serde_json::Value::Object(remaining);

                match kind.as_str() {
                    "redb" => {
                        let config: RedbStateStoreConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!("in stateStore (kind=redb): {e}"))
                            })?;
                        Ok(StateStoreConfig::Redb { path: config.path })
                    }
                    unknown => Err(de::Error::unknown_variant(unknown, STATE_STORE_KINDS)),
                }
            }
        }

        deserializer.deserialize_map(StateStoreConfigVisitor)
    }
}

impl StateStoreConfig {
    /// Create a new REDB state store configuration
    pub fn redb(path: impl Into<String>) -> Self {
        StateStoreConfig::Redb {
            path: ConfigValue::Static(path.into()),
        }
    }

    /// Get a display name for this state store type
    pub fn kind(&self) -> &str {
        match self {
            StateStoreConfig::Redb { .. } => "redb",
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_store_deserialize_redb_valid() {
        let json = r#"{
            "kind": "redb",
            "path": "./data/state.redb"
        }"#;

        let config: StateStoreConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.kind(), "redb");
        let StateStoreConfig::Redb { path } = config;
        assert_eq!(path, ConfigValue::Static("./data/state.redb".to_string()));
    }

    #[test]
    fn test_state_store_deserialize_redb_with_env_var() {
        let json = r#"{
            "kind": "redb",
            "path": "${STATE_STORE_PATH:-./data/default.redb}"
        }"#;

        let config: StateStoreConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.kind(), "redb");
        let StateStoreConfig::Redb { path } = config;
        assert!(
            matches!(
                &path,
                ConfigValue::EnvironmentVariable { name, default }
                if name == "STATE_STORE_PATH" && *default == Some("./data/default.redb".to_string())
            ),
            "Expected EnvironmentVariable variant, got {path:?}"
        );
    }

    #[test]
    fn test_state_store_deserialize_missing_kind() {
        let json = r#"{
            "path": "./data/state.redb"
        }"#;

        let result: Result<StateStoreConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("kind"),
            "Error should mention missing kind field: {err}"
        );
    }

    #[test]
    fn test_state_store_deserialize_unknown_kind() {
        let json = r#"{
            "kind": "unknown",
            "path": "./data/state.redb"
        }"#;

        let result: Result<StateStoreConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown") || err.contains("variant"),
            "Error should mention unknown kind: {err}"
        );
    }

    #[test]
    fn test_state_store_deserialize_unknown_field_rejected() {
        let json = r#"{
            "kind": "redb",
            "path": "./data/state.redb",
            "unknownField": "value"
        }"#;

        let result: Result<StateStoreConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Unknown field should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknownField") || err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_state_store_deserialize_snake_case_rejected() {
        let json = r#"{
            "kind": "redb",
            "file_path": "./data/state.redb"
        }"#;

        let result: Result<StateStoreConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "snake_case field should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("file_path") || err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_state_store_deserialize_error_has_context() {
        let json = r#"{
            "kind": "redb",
            "path": "./data/state.redb",
            "unknownField": "value"
        }"#;

        let result: Result<StateStoreConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("stateStore") || err.contains("redb"),
            "Error should have context about stateStore: {err}"
        );
    }

    #[test]
    fn test_state_store_serialize_deserialize_roundtrip() {
        let original = StateStoreConfig::redb("./data/roundtrip.redb");

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: StateStoreConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(original.kind(), deserialized.kind());
        let StateStoreConfig::Redb { path: p1 } = &original;
        let StateStoreConfig::Redb { path: p2 } = &deserialized;
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_state_store_deserialize_yaml_format() {
        let yaml = r#"
kind: redb
path: ./data/state.redb
"#;

        let config: StateStoreConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.kind(), "redb");
    }

    #[test]
    fn test_state_store_yaml_unknown_field_rejected() {
        let yaml = r#"
kind: redb
path: ./data/state.redb
unknownField: value
"#;

        let result: Result<StateStoreConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "Unknown field in YAML should be rejected");
    }
}
