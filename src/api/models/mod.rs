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

//! API models module - DTO types for configuration.
//!
//! This module contains all Data Transfer Object (DTO) types used in the API.
//! DTOs are organized into submodules matching the structure of the mappings module.
//!
//! # Organization
//!
//! - **`sources/`**: DTOs for data source configurations
//!   - `postgres` - PostgreSQL source
//!   - `http_source` - HTTP source
//!   - `grpc_source` - gRPC source
//!   - `mock` - Mock source for testing
//!
//! - **Reaction configs**: Provided dynamically by plugin descriptors
//!
//! - **`queries/`**: DTOs for query configurations
//!   - `query` - Continuous query configuration
//!
//! - **`config_value`**: Generic configuration value types for static/environment variable/secret references

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

// Bootstrap provider module
pub mod bootstrap;

// Organized submodules
pub mod observability;
pub mod queries;

// Re-export all DTO types for convenient access
pub use bootstrap::BootstrapProviderConfig;
pub use drasi_plugin_sdk::config_value::*;
pub use observability::*;
pub use queries::*;

// =============================================================================
// Configuration Enums (Top-level aggregates)
// =============================================================================

/// Helper function for serde defaults
fn default_true() -> bool {
    true
}

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

    /// Get the bootstrap provider configuration if any
    pub fn bootstrap_provider(&self) -> Option<&BootstrapProviderConfig> {
        self.bootstrap_provider.as_ref()
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
}

// =============================================================================
// State Store Configuration
// =============================================================================

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
// Tests for Custom Deserializers
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

    // =========================================================================
    // ReactionConfig Deserialization Tests
    // =========================================================================

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

    // =========================================================================
    // Serialization Round-Trip Tests
    // =========================================================================

    #[test]
    fn test_source_serialize_deserialize_roundtrip() {
        let original = SourceConfig {
            kind: "mock".to_string(),
            id: "roundtrip-source".to_string(),
            auto_start: false,
            bootstrap_provider: None,
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

    // =========================================================================
    // Edge Cases
    // =========================================================================

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

    // =========================================================================
    // StateStoreConfig Deserialization Tests
    // =========================================================================

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
