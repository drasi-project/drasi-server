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

//! Source configuration DTO.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::bootstrap::BootstrapProviderConfig;

/// Source configuration with kind discriminator.
///
/// A generic struct that holds the plugin kind, common fields (id, auto_start,
/// bootstrap_provider), and plugin-specific configuration as a JSON value.
/// The PluginRegistry is used at runtime to create the actual source instance.
///
/// # Example YAML
///
/// ```yaml
/// sources:
///   - kind: mock
///     id: test-source
///     autoStart: true
///     dataType:
///       type: sensorReading
///     intervalMs: 1000
///
///   - kind: http
///     id: http-source
///     host: "0.0.0.0"
///     port: 9000
/// ```
#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub kind: String,
    pub id: String,
    pub auto_start: bool,
    pub bootstrap_provider: Option<BootstrapProviderConfig>,
    /// Reference (by `id`) to an entry in the top-level
    /// `identityProviders` block. When set, the resolved provider is
    /// attached to the source via `Source::set_identity_provider` after
    /// construction.
    pub identity_provider: Option<String>,
    pub config: serde_json::Value,
}

impl Serialize for SourceConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.kind)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("autoStart", &self.auto_start)?;
        if let Some(bp) = &self.bootstrap_provider {
            map.serialize_entry("bootstrapProvider", bp)?;
        }
        if let Some(ip) = &self.identity_provider {
            map.serialize_entry("identityProvider", ip)?;
        }
        if let serde_json::Value::Object(config_map) = &self.config {
            for (k, v) in config_map {
                map.serialize_entry(k, v)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SourceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceConfigVisitor;

        impl<'de> Visitor<'de> for SourceConfigVisitor {
            type Value = SourceConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a source configuration with 'kind' and 'id' fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // Storage for common fields
                let mut kind: Option<String> = None;
                let mut id: Option<String> = None;
                let mut auto_start: Option<bool> = None;
                let mut bootstrap_provider: Option<serde_json::Value> = None;
                let mut identity_provider: Option<String> = None;

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
                        "autoStart" => {
                            if auto_start.is_some() {
                                return Err(de::Error::duplicate_field("autoStart"));
                            }
                            auto_start = Some(map.next_value()?);
                        }
                        "bootstrapProvider" => {
                            if bootstrap_provider.is_some() {
                                return Err(de::Error::duplicate_field("bootstrapProvider"));
                            }
                            bootstrap_provider = Some(map.next_value()?);
                        }
                        "identityProvider" => {
                            if identity_provider.is_some() {
                                return Err(de::Error::duplicate_field("identityProvider"));
                            }
                            identity_provider = Some(map.next_value()?);
                        }
                        // Reject common snake_case misspellings of known fields
                        "auto_start" => {
                            return Err(de::Error::custom(
                                "unknown field `auto_start`, did you mean `autoStart`?",
                            ));
                        }
                        "bootstrap_provider" => {
                            return Err(de::Error::custom(
                                "unknown field `bootstrap_provider`, did you mean `bootstrapProvider`?"
                            ));
                        }
                        "identity_provider" => {
                            return Err(de::Error::custom(
                                "unknown field `identity_provider`, did you mean `identityProvider`?"
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
                let auto_start = auto_start.unwrap_or(true);

                let remaining_value = serde_json::Value::Object(remaining);

                // Deserialize bootstrap_provider if present, inheriting from source when applicable.
                let bootstrap_provider: Option<BootstrapProviderConfig> = bootstrap_provider
                    .map(|value| {
                        merge_bootstrap_provider_with_source(&kind, value, &remaining_value)
                    })
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| {
                        de::Error::custom(format!("in source '{id}' bootstrapProvider: {e}"))
                    })?;

                Ok(SourceConfig {
                    kind,
                    id,
                    auto_start,
                    bootstrap_provider,
                    identity_provider,
                    config: remaining_value,
                })
            }
        }

        deserializer.deserialize_map(SourceConfigVisitor)
    }
}

impl SourceConfig {
    /// Get the source ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Check if auto_start is enabled
    pub fn auto_start(&self) -> bool {
        self.auto_start
    }

    /// Set the auto_start flag
    pub fn set_auto_start(&mut self, value: bool) {
        self.auto_start = value;
    }

    /// Get the bootstrap provider configuration if any
    pub fn bootstrap_provider(&self) -> Option<&BootstrapProviderConfig> {
        self.bootstrap_provider.as_ref()
    }

    /// Get the optional identity provider reference (id of an entry in
    /// the top-level `identityProviders` block).
    pub fn identity_provider(&self) -> Option<&str> {
        self.identity_provider.as_deref()
    }

    /// Get the source kind
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

fn merge_bootstrap_provider_with_source(
    source_kind: &str,
    bootstrap_value: serde_json::Value,
    source_config: &serde_json::Value,
) -> serde_json::Value {
    let mut bootstrap_map = match bootstrap_value {
        serde_json::Value::Object(map) => map,
        other => return other,
    };

    let bootstrap_kind = match bootstrap_map.get("kind") {
        Some(serde_json::Value::String(kind)) => kind.as_str(),
        _ => return serde_json::Value::Object(bootstrap_map),
    };

    if bootstrap_kind != source_kind {
        return serde_json::Value::Object(bootstrap_map);
    }

    let Some(allowed_fields) = allowed_bootstrap_provider_fields(bootstrap_kind) else {
        return serde_json::Value::Object(bootstrap_map);
    };

    let serde_json::Value::Object(source_map) = source_config else {
        return serde_json::Value::Object(bootstrap_map);
    };

    for field in allowed_fields {
        if !bootstrap_map.contains_key(*field) {
            if let Some(value) = source_map.get(*field) {
                bootstrap_map.insert((*field).to_string(), value.clone());
            }
        }
    }

    serde_json::Value::Object(bootstrap_map)
}

fn allowed_bootstrap_provider_fields(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "postgres" => Some(&[
            "host",
            "port",
            "database",
            "user",
            "password",
            "tables",
            "slotName",
            "publicationName",
            "sslMode",
            "tableKeys",
        ]),
        "scriptfile" => Some(&["filePaths"]),
        "application" | "noop" => Some(&[]),
        _ => None,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_deserialize_mock_minimal() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert!(source.auto_start()); // default is true
    }

    #[test]
    fn test_source_deserialize_auto_start_defaults_true() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert!(source.auto_start());
    }

    #[test]
    fn test_source_deserialize_auto_start_explicit_false() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "autoStart": false
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert!(!source.auto_start());
    }

    #[test]
    fn test_source_deserialize_http_valid() {
        let json = r#"{
            "kind": "http",
            "id": "http-source",
            "autoStart": true,
            "host": "localhost",
            "port": 8080,
            "timeoutMs": 5000
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "http-source");
        assert_eq!(source.kind(), "http");
    }

    #[test]
    fn test_source_deserialize_grpc_valid() {
        let json = r#"{
            "kind": "grpc",
            "id": "grpc-source",
            "endpoint": "http://localhost:50051"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "grpc-source");
        assert_eq!(source.kind(), "grpc");
    }

    #[test]
    fn test_source_deserialize_postgres_valid() {
        let json = r#"{
            "kind": "postgres",
            "id": "pg-source",
            "host": "localhost",
            "port": 5432,
            "database": "testdb",
            "user": "postgres",
            "password": "secret",
            "slotName": "test_slot",
            "publicationName": "test_pub"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "pg-source");
        assert_eq!(source.kind(), "postgres");
    }

    #[test]
    fn test_source_deserialize_platform_valid() {
        let json = r#"{
            "kind": "platform",
            "id": "platform-source",
            "redisUrl": "redis://localhost:6379",
            "streamKey": "events"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "platform-source");
        assert_eq!(source.kind(), "platform");
    }

    #[test]
    fn test_source_deserialize_missing_kind() {
        let json = r#"{
            "id": "test-source"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("kind"), "Error should mention 'kind': {err}");
    }

    #[test]
    fn test_source_deserialize_missing_id() {
        let json = r#"{
            "kind": "mock"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("id"), "Error should mention 'id': {err}");
    }
    #[test]
    fn test_source_deserialize_unknown_kind_accepted() {
        // With registry-driven approach, unknown kinds are accepted at deserialization
        // and rejected at creation time by the registry.
        let json = r#"{
            "kind": "unknown-source-type",
            "id": "test-source"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.kind(), "unknown-source-type");
        assert_eq!(source.id(), "test-source");
    }

    #[test]
    fn test_source_deserialize_extra_fields_stored_in_config() {
        // Extra fields are stored in the config JSON value for the plugin to validate.
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "extraField": "value"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert_eq!(source.config["extraField"], "value");
    }

    #[test]
    fn test_source_deserialize_unknown_kind_accepted_at_deser() {
        // With generic struct approach, unknown kinds are accepted at deserialization
        // and only validated at creation time via the plugin registry.
        let json = r#"{
            "kind": "unknown-source-type",
            "id": "test-source"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.kind(), "unknown-source-type");
        assert_eq!(source.id(), "test-source");
    }

    #[test]
    fn test_source_deserialize_unknown_field_stored_in_config() {
        // Extra/unknown fields are stored in the config JSON for plugin validation.
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "unknownField": "value"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert_eq!(source.config["unknownField"], "value");
    }

    #[test]
    fn test_source_deserialize_snake_case_auto_start_rejected() {
        // snake_case auto_start is explicitly rejected with a helpful hint.
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "auto_start": true
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
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
    fn test_source_deserialize_snake_case_data_type_stored_in_config() {
        // snake_case fields go into config JSON.
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "data_type": "sensor"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert_eq!(source.config["data_type"], "sensor");
    }

    #[test]
    fn test_source_deserialize_extra_field_stored_with_kind_context() {
        // Extra fields are stored; kind is available for context.
        let json = r#"{
            "kind": "mock",
            "id": "my-unique-source",
            "badField": "value"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "my-unique-source");
        assert_eq!(source.kind(), "mock");
        assert_eq!(source.config["badField"], "value");
    }

    #[test]
    fn test_source_deserialize_kind_preserved() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "badField": "value"
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.kind(), "mock");
    }

    #[test]
    fn test_source_deserialize_with_bootstrap_provider() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "bootstrapProvider": {
                "kind": "noop"
            }
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert!(source.bootstrap_provider().is_some());
    }

    #[test]
    fn test_bootstrap_provider_inherits_postgres_fields() {
        let yaml = r#"
kind: postgres
id: source-with-bootstrap
host: localhost
port: 5432
database: drasi
user: drasi_user
password: drasi_pass
slotName: drasi_slot
publicationName: drasi_pub
bootstrapProvider:
  kind: postgres
"#;

        let source: SourceConfig = serde_yaml::from_str(yaml).unwrap();
        let bp = source
            .bootstrap_provider()
            .expect("Expected bootstrap provider");
        assert_eq!(bp.kind(), "postgres");

        // After merge_bootstrap_provider_with_source, inherited fields should be present
        assert_eq!(bp.config["host"], "localhost");
        assert_eq!(bp.config["port"], 5432);
        assert_eq!(bp.config["database"], "drasi");
        assert_eq!(bp.config["user"], "drasi_user");
        assert_eq!(bp.config["password"], "drasi_pass");
        assert_eq!(bp.config["slotName"], "drasi_slot");
        assert_eq!(bp.config["publicationName"], "drasi_pub");
    }

    #[test]
    fn test_bootstrap_provider_postgres_override() {
        let yaml = r#"
kind: postgres
id: source-with-bootstrap
host: localhost
port: 5432
database: drasi
user: drasi_user
password: drasi_pass
slotName: drasi_slot
publicationName: drasi_pub
bootstrapProvider:
  kind: postgres
  database: bootstrap_db
  user: bootstrap_user
"#;

        let source: SourceConfig = serde_yaml::from_str(yaml).unwrap();
        let bp = source
            .bootstrap_provider()
            .expect("Expected bootstrap provider");
        assert_eq!(bp.kind(), "postgres");

        // Overridden fields
        assert_eq!(bp.config["database"], "bootstrap_db");
        assert_eq!(bp.config["user"], "bootstrap_user");
        // Inherited field
        assert_eq!(bp.config["password"], "drasi_pass");
    }

    #[test]
    fn test_source_deserialize_yaml_format() {
        let yaml = r#"
kind: mock
id: yaml-source
autoStart: true
dataType:
  type: sensorReading
intervalMs: 1000
"#;

        let source: SourceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(source.id(), "yaml-source");
        assert!(source.auto_start());
    }

    #[test]
    fn test_source_serialize_deserialize_roundtrip() {
        let original = SourceConfig {
            kind: "mock".to_string(),
            id: "roundtrip-source".to_string(),
            auto_start: false,
            bootstrap_provider: None,
            identity_provider: None,
            config: serde_json::json!({
                "dataType": { "type": "sensorReading", "sensorCount": 5 },
                "intervalMs": 1000
            }),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SourceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id(), "roundtrip-source");
        assert!(!deserialized.auto_start());
    }

    #[test]
    fn test_source_deserialize_duplicate_field_rejected() {
        // JSON with duplicate fields - serde_json rejects this
        let json = r#"{
            "kind": "mock",
            "id": "first-id",
            "id": "second-id"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("duplicate"),
            "Error should mention duplicate field: {err}"
        );
    }

    #[test]
    fn test_source_deserialize_with_enum_data_type() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "dataType": { "type": "sensorReading", "sensorCount": 10 },
            "intervalMs": 1000
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert_eq!(source.kind(), "mock");
        let config = &source.config;
        assert_eq!(config["dataType"]["type"], "sensorReading");
        assert_eq!(config["dataType"]["sensorCount"], 10);
    }
}
