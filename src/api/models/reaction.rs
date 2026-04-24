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

//! Reaction configuration DTO.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Reaction configuration with kind discriminator.
///
/// A generic struct that holds the plugin kind, common fields (id, queries,
/// auto_start), and plugin-specific configuration as a JSON value.
/// The PluginRegistry is used at runtime to create the actual reaction instance.
#[derive(Debug, Clone)]
pub struct ReactionConfig {
    pub kind: String,
    pub id: String,
    pub queries: Vec<String>,
    pub auto_start: bool,
    pub config: serde_json::Value,
}

impl Serialize for ReactionConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.kind)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("queries", &self.queries)?;
        map.serialize_entry("autoStart", &self.auto_start)?;
        if let serde_json::Value::Object(config_map) = &self.config {
            for (k, v) in config_map {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ReactionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ReactionConfigVisitor;

        impl<'de> Visitor<'de> for ReactionConfigVisitor {
            type Value = ReactionConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a reaction configuration with 'kind', 'id', and 'queries' fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // Storage for common fields
                let mut kind: Option<String> = None;
                let mut id: Option<String> = None;
                let mut queries: Option<Vec<String>> = None;
                let mut auto_start: Option<bool> = None;

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
                        "id" => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        "queries" => {
                            if queries.is_some() {
                                return Err(de::Error::duplicate_field("queries"));
                            }
                            queries = Some(map.next_value()?);
                        }
                        "autoStart" => {
                            if auto_start.is_some() {
                                return Err(de::Error::duplicate_field("autoStart"));
                            }
                            auto_start = Some(map.next_value()?);
                        }
                        // Reject common snake_case misspellings of known fields
                        "auto_start" => {
                            return Err(de::Error::custom(
                                "unknown field `auto_start`, did you mean `autoStart`?",
                            ));
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
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                let queries = queries.ok_or_else(|| de::Error::missing_field("queries"))?;
                let auto_start = auto_start.unwrap_or(true);

                let remaining_value = serde_json::Value::Object(remaining);

                Ok(ReactionConfig {
                    kind,
                    id,
                    queries,
                    auto_start,
                    config: remaining_value,
                })
            }
        }

        deserializer.deserialize_map(ReactionConfigVisitor)
    }
}

impl ReactionConfig {
    /// Get the reaction ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the query IDs this reaction subscribes to
    pub fn queries(&self) -> &[String] {
        &self.queries
    }

    /// Check if auto_start is enabled
    pub fn auto_start(&self) -> bool {
        self.auto_start
    }

    /// Get the reaction kind
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Set the auto_start flag
    pub fn set_auto_start(&mut self, value: bool) {
        self.auto_start = value;
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reaction_deserialize_log_valid() {
        let json = r#"{
            "kind": "log",
            "id": "test-log",
            "queries": ["query1", "query2"],
            "autoStart": true
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "test-log");
        assert_eq!(reaction.queries(), &["query1", "query2"]);
        assert!(reaction.auto_start());
        assert_eq!(reaction.kind(), "log");
    }

    #[test]
    fn test_reaction_deserialize_log_minimal() {
        let json = r#"{
            "kind": "log",
            "id": "test-log",
            "queries": ["query1"]
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "test-log");
        assert!(reaction.auto_start()); // default is true
    }

    #[test]
    fn test_reaction_deserialize_auto_start_defaults_true() {
        let json = r#"{
            "kind": "log",
            "id": "test-log",
            "queries": ["query1"]
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert!(reaction.auto_start());
    }

    #[test]
    fn test_reaction_deserialize_auto_start_explicit_false() {
        let json = r#"{
            "kind": "log",
            "id": "test-log",
            "queries": ["query1"],
            "autoStart": false
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert!(!reaction.auto_start());
    }

    #[test]
    fn test_reaction_deserialize_http_valid() {
        let json = r#"{
            "kind": "http",
            "id": "http-reaction",
            "queries": ["query1"],
            "baseUrl": "http://localhost:8080",
            "timeoutMs": 5000,
            "routes": {}
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "http-reaction");
        assert_eq!(reaction.kind(), "http");
    }

    #[test]
    fn test_reaction_deserialize_http_adaptive_valid() {
        let json = r#"{
            "kind": "http-adaptive",
            "id": "http-adaptive-reaction",
            "queries": ["query1"],
            "baseUrl": "http://localhost:8080",
            "timeoutMs": 5000,
            "routes": {}
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "http-adaptive-reaction");
        assert_eq!(reaction.kind(), "http-adaptive");
    }

    #[test]
    fn test_reaction_deserialize_grpc_valid() {
        let json = r#"{
            "kind": "grpc",
            "id": "grpc-reaction",
            "queries": ["query1"],
            "endpoint": "http://localhost:50051"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "grpc-reaction");
        assert_eq!(reaction.kind(), "grpc");
    }

    #[test]
    fn test_reaction_deserialize_grpc_adaptive_valid() {
        let json = r#"{
            "kind": "grpc-adaptive",
            "id": "grpc-adaptive-reaction",
            "queries": ["query1"],
            "endpoint": "http://localhost:50051"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "grpc-adaptive-reaction");
        assert_eq!(reaction.kind(), "grpc-adaptive");
    }

    #[test]
    fn test_reaction_deserialize_sse_valid() {
        let json = r#"{
            "kind": "sse",
            "id": "sse-reaction",
            "queries": ["query1"]
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "sse-reaction");
        assert_eq!(reaction.kind(), "sse");
    }

    #[test]
    fn test_reaction_deserialize_platform_valid() {
        let json = r#"{
            "kind": "platform",
            "id": "platform-reaction",
            "queries": ["query1"],
            "redisUrl": "redis://localhost:6379"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "platform-reaction");
        assert_eq!(reaction.kind(), "platform");
    }

    #[test]
    fn test_reaction_deserialize_profiler_valid() {
        let json = r#"{
            "kind": "profiler",
            "id": "profiler-reaction",
            "queries": ["query1"]
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "profiler-reaction");
        assert_eq!(reaction.kind(), "profiler");
    }

    #[test]
    fn test_reaction_deserialize_missing_kind() {
        let json = r#"{
            "id": "test-reaction",
            "queries": ["query1"]
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("kind"), "Error should mention 'kind': {err}");
    }

    #[test]
    fn test_reaction_deserialize_missing_id() {
        let json = r#"{
            "kind": "log",
            "queries": ["query1"]
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("id"), "Error should mention 'id': {err}");
    }

    #[test]
    fn test_reaction_deserialize_missing_queries() {
        let json = r#"{
            "kind": "log",
            "id": "test-reaction"
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("queries"),
            "Error should mention 'queries': {err}"
        );
    }
    #[test]
    fn test_reaction_deserialize_unknown_kind_accepted() {
        // With registry-driven approach, unknown kinds are accepted at deserialization
        let json = r#"{
            "kind": "unknown-reaction-type",
            "id": "test-reaction",
            "queries": ["query1"]
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.kind(), "unknown-reaction-type");
    }

    #[test]
    fn test_reaction_deserialize_extra_fields_stored_in_config() {
        // Extra fields are stored in the config JSON value
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": ["query1"],
            "extraField": "value"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.config["extraField"], "value");
    }

    #[test]
    fn test_reaction_deserialize_unknown_kind_accepted_at_deser() {
        // With generic struct approach, unknown kinds are accepted at deserialization.
        let json = r#"{
            "kind": "unknown-reaction-type",
            "id": "test-reaction",
            "queries": ["query1"]
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.kind(), "unknown-reaction-type");
        assert_eq!(reaction.id(), "test-reaction");
    }

    #[test]
    fn test_reaction_deserialize_unknown_field_stored_in_config() {
        // Extra fields stored in config JSON for plugin validation.
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": ["query1"],
            "unknownField": "value"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "test-reaction");
        assert_eq!(reaction.config["unknownField"], "value");
    }

    #[test]
    fn test_reaction_deserialize_snake_case_auto_start_rejected() {
        // snake_case auto_start is explicitly rejected with a helpful hint.
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": ["query1"],
            "auto_start": true
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("auto_start"),
            "Error should mention auto_start: {err}"
        );
        assert!(
            err.contains("autoStart"),
            "Error should suggest autoStart: {err}"
        );
    }

    #[test]
    fn test_reaction_deserialize_extra_field_stored_with_id_context() {
        // Extra fields are stored; id is available for context.
        let json = r#"{
            "kind": "log",
            "id": "my-unique-reaction",
            "queries": ["query1"],
            "badField": "value"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "my-unique-reaction");
        assert_eq!(reaction.config["badField"], "value");
    }

    #[test]
    fn test_reaction_deserialize_kind_preserved() {
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": ["query1"],
            "badField": "value"
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.kind(), "log");
    }

    #[test]
    fn test_reaction_deserialize_yaml_format() {
        let yaml = r#"
kind: log
id: yaml-reaction
queries:
  - query1
  - query2
autoStart: true
"#;

        let reaction: ReactionConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(reaction.id(), "yaml-reaction");
        assert_eq!(reaction.queries(), &["query1", "query2"]);
        assert!(reaction.auto_start());
    }

    #[test]
    fn test_reaction_deserialize_empty_queries() {
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": []
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert!(reaction.queries().is_empty());
    }

    #[test]
    fn test_reaction_serialize_deserialize_roundtrip() {
        let original = ReactionConfig {
            kind: "log".to_string(),
            id: "roundtrip-reaction".to_string(),
            queries: vec!["q1".to_string(), "q2".to_string()],
            auto_start: false,
            config: serde_json::json!({"routes": {}}),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ReactionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id(), "roundtrip-reaction");
        assert_eq!(deserialized.queries(), &["q1", "q2"]);
        assert!(!deserialized.auto_start());
    }

    #[test]
    fn test_reaction_deserialize_with_env_var_syntax() {
        let json = r#"{
            "kind": "http",
            "id": "test-http",
            "queries": ["query1"],
            "baseUrl": "${BASE_URL:-http://localhost:8080}",
            "timeoutMs": 5000,
            "routes": {}
        }"#;

        let reaction: ReactionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.id(), "test-http");
        assert_eq!(reaction.kind(), "http");
        // Verify the env var config is preserved in the raw config
        let base_url = &reaction.config["baseUrl"];
        assert_eq!(
            base_url.as_str().unwrap(),
            "${BASE_URL:-http://localhost:8080}"
        );
    }
}
