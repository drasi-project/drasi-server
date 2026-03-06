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
//! Bootstrap providers handle initial data delivery for newly subscribed queries.
//! The generic `BootstrapProviderConfig` struct stores the provider kind and
//! plugin-specific configuration as a JSON value, similar to SourceConfig and
//! ReactionConfig. Typed DTOs and OpenAPI schemas are provided by each plugin's
//! descriptor via the plugin registry.

use serde::{de, de::MapAccess, de::Visitor, Deserialize, Deserializer, Serialize};
use std::fmt;

/// Configuration for bootstrap providers.
///
/// Bootstrap providers handle initial data delivery for newly subscribed queries.
/// This generic struct stores the provider kind and plugin-specific configuration
/// as a JSON value, similar to SourceConfig and ReactionConfig.
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapProviderConfig {
    pub kind: String,
    pub config: serde_json::Value,
}

impl BootstrapProviderConfig {
    /// Get the kind string for this bootstrap provider config.
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

impl Serialize for BootstrapProviderConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.kind)?;
        if let serde_json::Value::Object(config_map) = &self.config {
            for (k, v) in config_map {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }
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

                let kind = provider_kind.ok_or_else(|| de::Error::missing_field("kind"))?;

                let config = serde_json::Value::Object(remaining);

                Ok(BootstrapProviderConfig { kind, config })
            }
        }

        deserializer.deserialize_map(BootstrapProviderConfigVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_kind_field() {
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
        assert_eq!(config.kind(), "scriptfile");
        assert_eq!(config.config["filePaths"][0], "/data/file1.jsonl");
        assert_eq!(config.config["filePaths"][1], "/data/file2.jsonl");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = BootstrapProviderConfig {
            kind: "scriptfile".to_string(),
            config: serde_json::json!({
                "filePaths": ["/test.jsonl"]
            }),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"kind\":\"scriptfile\""));
        assert!(json.contains("\"filePaths\""));

        let deserialized: BootstrapProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }
}
