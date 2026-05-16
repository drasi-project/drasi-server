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

//! Secret store configuration DTOs.

use serde::{Deserialize, Serialize};

/// Secret store configuration with kind discriminator and opaque config.
///
/// Unlike `StateStoreConfig` which uses hardcoded enum variants, secret stores
/// are plugins loaded at runtime so the config is generic: a `kind` string that
/// maps to a registered `SecretStorePluginDescriptor`, plus an opaque JSON object
/// passed to the plugin's `create_secret_store()` method.
///
/// # Example YAML
///
/// ```yaml
/// secretStore:
///   kind: file
///   path: ./secrets.json
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretStoreConfig {
    /// The secret store plugin kind (e.g., "file", "keyring", "azure-keyvault")
    pub kind: String,

    /// Opaque configuration passed to the plugin's `create_secret_store()`.
    /// All fields except `kind` are collected here.
    #[serde(flatten)]
    pub config: serde_json::Value,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_store_deserialize_file() {
        let json = r#"{
            "kind": "file",
            "path": "./secrets.json"
        }"#;

        let config: SecretStoreConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.kind, "file");
        assert_eq!(config.config["path"], "./secrets.json");
    }

    #[test]
    fn test_secret_store_deserialize_yaml() {
        let yaml = r#"
kind: file
path: ./secrets.json
"#;

        let config: SecretStoreConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.kind, "file");
        assert_eq!(config.config["path"], "./secrets.json");
    }

    #[test]
    fn test_secret_store_deserialize_missing_kind() {
        let json = r#"{
            "path": "./secrets.json"
        }"#;

        let result: Result<SecretStoreConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_secret_store_serialize_roundtrip() {
        let original = SecretStoreConfig {
            kind: "file".to_string(),
            config: serde_json::json!({"path": "./secrets.json"}),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SecretStoreConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(original.kind, deserialized.kind);
        assert_eq!(original.config["path"], deserialized.config["path"]);
    }
}
