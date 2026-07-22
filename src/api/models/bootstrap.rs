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
///
/// This type is intentionally **id-free**: it describes an inline bootstrap
/// provider nested under a source. Top-level, referenceable providers are
/// represented by [`TopLevelBootstrapProviderConfig`], which adds a required
/// `id`.
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

/// A top-level, referenceable bootstrap provider: a [`BootstrapProviderConfig`]
/// plus a required `id`. Sources reference these via `bootstrapProvider: <id>`.
///
/// Keeping the `id` in a dedicated wrapper (rather than an optional field on
/// [`BootstrapProviderConfig`]) lets the "top-level entries require an id"
/// constraint be enforced structurally instead of via runtime validation.
#[derive(Debug, Clone, PartialEq)]
pub struct TopLevelBootstrapProviderConfig {
    pub id: String,
    pub inner: BootstrapProviderConfig,
}

impl TopLevelBootstrapProviderConfig {
    /// Get the id of this top-level bootstrap provider.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the kind of this top-level bootstrap provider.
    pub fn kind(&self) -> &str {
        self.inner.kind()
    }
}

/// A source's bootstrap provider: either a reference (by id) to a top-level
/// `bootstrapProviders` entry, or an inline definition nested under the source.
///
/// The reference form (a YAML/JSON string) enables sharing a single top-level
/// bootstrap provider configuration across multiple sources. The inline form
/// (a mapping) preserves the legacy behavior where the bootstrap provider can
/// inherit fields from the owning source's config.
#[derive(Debug, Clone, PartialEq)]
pub enum BootstrapProviderRef {
    /// Reference to a top-level `bootstrapProviders` entry by id.
    Reference(String),
    /// Inline bootstrap provider definition.
    Inline(BootstrapProviderConfig),
}

impl BootstrapProviderRef {
    /// Return the inline config, if this is the inline form.
    pub fn as_inline(&self) -> Option<&BootstrapProviderConfig> {
        match self {
            Self::Inline(config) => Some(config),
            Self::Reference(_) => None,
        }
    }

    /// Return the referenced id, if this is the reference form.
    pub fn as_reference(&self) -> Option<&str> {
        match self {
            Self::Reference(id) => Some(id.as_str()),
            Self::Inline(_) => None,
        }
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

impl Serialize for TopLevelBootstrapProviderConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.inner.kind)?;
        map.serialize_entry("id", &self.id)?;
        if let serde_json::Value::Object(config_map) = &self.inner.config {
            for (k, v) in config_map {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for TopLevelBootstrapProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TopLevelVisitor;

        impl<'de> Visitor<'de> for TopLevelVisitor {
            type Value = TopLevelBootstrapProviderConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a top-level bootstrap provider with 'kind' and 'id' fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut provider_kind: Option<String> = None;
                let mut id: Option<String> = None;
                let mut remaining = serde_json::Map::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if provider_kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            provider_kind = Some(map.next_value()?);
                        }
                        "id" => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        other => {
                            let value: serde_json::Value = map.next_value()?;
                            remaining.insert(other.to_string(), value);
                        }
                    }
                }

                let kind = provider_kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;

                Ok(TopLevelBootstrapProviderConfig {
                    id,
                    inner: BootstrapProviderConfig {
                        kind,
                        config: serde_json::Value::Object(remaining),
                    },
                })
            }
        }

        deserializer.deserialize_map(TopLevelVisitor)
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

    #[test]
    fn test_top_level_roundtrip_and_requires_id() {
        let yaml = r#"
kind: postgres
id: pg-bootstrap
host: localhost
tables:
  - Message
"#;
        let cfg: TopLevelBootstrapProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.id(), "pg-bootstrap");
        assert_eq!(cfg.kind(), "postgres");
        assert_eq!(cfg.inner.config["host"], "localhost");

        // Round-trips through serialize/deserialize.
        let json = serde_json::to_string(&cfg).unwrap();
        let back: TopLevelBootstrapProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);

        // Missing id is a hard error (structurally enforced).
        let err = serde_yaml::from_str::<TopLevelBootstrapProviderConfig>("kind: postgres\n")
            .unwrap_err();
        assert!(
            err.to_string().contains("id"),
            "expected missing id error: {err}"
        );
    }
}
