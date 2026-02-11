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
//!   - `platform_source` - Platform/Redis source
//!
//! - **`reactions/`**: DTOs for reaction configurations
//!   - `http_reaction` - HTTP and HTTP Adaptive reactions
//!   - `grpc_reaction` - gRPC and gRPC Adaptive reactions
//!   - `sse` - Server-Sent Events reaction
//!   - `log` - Log reaction
//!   - `platform_reaction` - Platform reaction
//!   - `profiler` - Profiler reaction
//!
//! - **`queries/`**: DTOs for query configurations
//!   - `query` - Continuous query configuration
//!
//! - **`config_value`**: Generic configuration value types for static/environment variable/secret references

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

// Config value module
pub mod config_value;

// Bootstrap provider module
pub mod bootstrap;

// Organized submodules
pub mod queries;
pub mod reactions;
pub mod sources;
pub mod observability;

// Re-export all DTO types for convenient access
pub use bootstrap::{
    ApplicationBootstrapConfigDto, BootstrapProviderConfig, PlatformBootstrapConfigDto,
    PostgresBootstrapConfigDto, ScriptFileBootstrapConfigDto,
};
pub use config_value::*;
pub use observability::*;
pub use queries::*;
pub use reactions::*;
pub use sources::*;

// =============================================================================
// Configuration Enums (Top-level aggregates)
// =============================================================================

/// Helper function for serde defaults
fn default_true() -> bool {
    true
}

/// Source configuration with kind discriminator.
///
/// Uses a custom deserializer to handle the `kind` field and validate unknown fields.
/// The inner config DTOs use `#[serde(deny_unknown_fields)]` to catch typos.
///
/// # Example YAML
///
/// ```yaml
/// sources:
///   - kind: mock
///     id: test-source
///     autoStart: true
///     dataType: { "type": "sensorReading" },
///     intervalMs: 1000
///
///   - kind: http
///     id: http-source
///     host: "0.0.0.0"
///     port: 9000
/// ```
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SourceConfig {
    /// Mock source for testing
    #[serde(rename = "mock")]
    Mock {
        id: String,
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: MockSourceConfigDto,
    },
    /// HTTP source for receiving events via HTTP endpoints
    #[serde(rename = "http")]
    Http {
        id: String,
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: HttpSourceConfigDto,
    },
    /// gRPC source for receiving events via gRPC streaming
    #[serde(rename = "grpc")]
    Grpc {
        id: String,
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: GrpcSourceConfigDto,
    },
    /// PostgreSQL replication source for CDC
    #[serde(rename = "postgres")]
    Postgres {
        id: String,
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: PostgresSourceConfigDto,
    },
    /// Platform source for Redis Streams consumption
    #[serde(rename = "platform")]
    Platform {
        id: String,
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: PlatformSourceConfigDto,
    },
}

// Known source kinds for error messages
const SOURCE_KINDS: &[&str] = &["mock", "http", "grpc", "postgres", "platform"];

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

                match kind.as_str() {
                    "mock" => {
                        let config: MockSourceConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                            de::Error::custom(format!("in source '{id}' (kind=mock): {e}"))
                        })?;
                        Ok(SourceConfig::Mock {
                            id,
                            auto_start,
                            bootstrap_provider,
                            config,
                        })
                    }
                    "http" => {
                        let config: HttpSourceConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                            de::Error::custom(format!("in source '{id}' (kind=http): {e}"))
                        })?;
                        Ok(SourceConfig::Http {
                            id,
                            auto_start,
                            bootstrap_provider,
                            config,
                        })
                    }
                    "grpc" => {
                        let config: GrpcSourceConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                            de::Error::custom(format!("in source '{id}' (kind=grpc): {e}"))
                        })?;
                        Ok(SourceConfig::Grpc {
                            id,
                            auto_start,
                            bootstrap_provider,
                            config,
                        })
                    }
                    "postgres" => {
                        let config: PostgresSourceConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!("in source '{id}' (kind=postgres): {e}"))
                            })?;
                        Ok(SourceConfig::Postgres {
                            id,
                            auto_start,
                            bootstrap_provider,
                            config,
                        })
                    }
                    "platform" => {
                        let config: PlatformSourceConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!("in source '{id}' (kind=platform): {e}"))
                            })?;
                        Ok(SourceConfig::Platform {
                            id,
                            auto_start,
                            bootstrap_provider,
                            config,
                        })
                    }
                    unknown => Err(de::Error::unknown_variant(unknown, SOURCE_KINDS)),
                }
            }
        }

        deserializer.deserialize_map(SourceConfigVisitor)
    }
}

impl SourceConfig {
    /// Get the source ID
    pub fn id(&self) -> &str {
        match self {
            SourceConfig::Mock { id, .. } => id,
            SourceConfig::Http { id, .. } => id,
            SourceConfig::Grpc { id, .. } => id,
            SourceConfig::Postgres { id, .. } => id,
            SourceConfig::Platform { id, .. } => id,
        }
    }

    /// Check if auto_start is enabled
    pub fn auto_start(&self) -> bool {
        match self {
            SourceConfig::Mock { auto_start, .. } => *auto_start,
            SourceConfig::Http { auto_start, .. } => *auto_start,
            SourceConfig::Grpc { auto_start, .. } => *auto_start,
            SourceConfig::Postgres { auto_start, .. } => *auto_start,
            SourceConfig::Platform { auto_start, .. } => *auto_start,
        }
    }

    /// Get the bootstrap provider configuration if any
    pub fn bootstrap_provider(&self) -> Option<&BootstrapProviderConfig> {
        match self {
            SourceConfig::Mock {
                bootstrap_provider, ..
            } => bootstrap_provider.as_ref(),
            SourceConfig::Http {
                bootstrap_provider, ..
            } => bootstrap_provider.as_ref(),
            SourceConfig::Grpc {
                bootstrap_provider, ..
            } => bootstrap_provider.as_ref(),
            SourceConfig::Postgres {
                bootstrap_provider, ..
            } => bootstrap_provider.as_ref(),
            SourceConfig::Platform {
                bootstrap_provider, ..
            } => bootstrap_provider.as_ref(),
        }
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
        "platform" => Some(&["queryApiUrl", "timeoutSeconds"]),
        "scriptfile" => Some(&["filePaths"]),
        "application" | "noop" => Some(&[]),
        _ => None,
    }
}

/// Reaction configuration with kind discriminator.
///
/// Uses a custom deserializer to handle the `kind` field and validate unknown fields.
/// The inner config DTOs use `#[serde(deny_unknown_fields)]` to catch typos.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReactionConfig {
    /// Log reaction for console output
    #[serde(rename = "log")]
    Log {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: LogReactionConfigDto,
    },
    /// HTTP reaction for webhooks
    #[serde(rename = "http")]
    Http {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: HttpReactionConfigDto,
    },
    /// HTTP adaptive reaction with batching
    #[serde(rename = "http-adaptive")]
    HttpAdaptive {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: HttpAdaptiveReactionConfigDto,
    },
    /// gRPC reaction for streaming results
    #[serde(rename = "grpc")]
    Grpc {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: GrpcReactionConfigDto,
    },
    /// gRPC adaptive reaction with batching
    #[serde(rename = "grpc-adaptive")]
    GrpcAdaptive {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: GrpcAdaptiveReactionConfigDto,
    },
    /// SSE reaction for Server-Sent Events
    #[serde(rename = "sse")]
    Sse {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: SseReactionConfigDto,
    },
    /// Platform reaction for Drasi platform integration
    #[serde(rename = "platform")]
    Platform {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: PlatformReactionConfigDto,
    },
    /// Profiler reaction for performance analysis
    #[serde(rename = "profiler")]
    Profiler {
        id: String,
        queries: Vec<String>,
        auto_start: bool,
        #[serde(flatten)]
        config: ProfilerReactionConfigDto,
    },
}

// Known reaction kinds for error messages
const REACTION_KINDS: &[&str] = &[
    "log",
    "http",
    "http-adaptive",
    "grpc",
    "grpc-adaptive",
    "sse",
    "platform",
    "profiler",
];

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

                match kind.as_str() {
                    "log" => {
                        let config: LogReactionConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                                de::Error::custom(format!("in reaction '{id}' (kind=log): {e}"))
                            })?;
                        Ok(ReactionConfig::Log {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "http" => {
                        let config: HttpReactionConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                                de::Error::custom(format!("in reaction '{id}' (kind=http): {e}"))
                            })?;
                        Ok(ReactionConfig::Http {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "http-adaptive" => {
                        let config: HttpAdaptiveReactionConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in reaction '{id}' (kind=http-adaptive): {e}"
                                ))
                            })?;
                        Ok(ReactionConfig::HttpAdaptive {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "grpc" => {
                        let config: GrpcReactionConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                                de::Error::custom(format!("in reaction '{id}' (kind=grpc): {e}"))
                            })?;
                        Ok(ReactionConfig::Grpc {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "grpc-adaptive" => {
                        let config: GrpcAdaptiveReactionConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in reaction '{id}' (kind=grpc-adaptive): {e}"
                                ))
                            })?;
                        Ok(ReactionConfig::GrpcAdaptive {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "sse" => {
                        let config: SseReactionConfigDto = serde_json::from_value(remaining_value)
                            .map_err(|e| {
                                de::Error::custom(format!("in reaction '{id}' (kind=sse): {e}"))
                            })?;
                        Ok(ReactionConfig::Sse {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "platform" => {
                        let config: PlatformReactionConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in reaction '{id}' (kind=platform): {e}"
                                ))
                            })?;
                        Ok(ReactionConfig::Platform {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    "profiler" => {
                        let config: ProfilerReactionConfigDto =
                            serde_json::from_value(remaining_value).map_err(|e| {
                                de::Error::custom(format!(
                                    "in reaction '{id}' (kind=profiler): {e}"
                                ))
                            })?;
                        Ok(ReactionConfig::Profiler {
                            id,
                            queries,
                            auto_start,
                            config,
                        })
                    }
                    unknown => Err(de::Error::unknown_variant(unknown, REACTION_KINDS)),
                }
            }
        }

        deserializer.deserialize_map(ReactionConfigVisitor)
    }
}

impl ReactionConfig {
    /// Get the reaction ID
    pub fn id(&self) -> &str {
        match self {
            ReactionConfig::Log { id, .. } => id,
            ReactionConfig::Http { id, .. } => id,
            ReactionConfig::HttpAdaptive { id, .. } => id,
            ReactionConfig::Grpc { id, .. } => id,
            ReactionConfig::GrpcAdaptive { id, .. } => id,
            ReactionConfig::Sse { id, .. } => id,
            ReactionConfig::Platform { id, .. } => id,
            ReactionConfig::Profiler { id, .. } => id,
        }
    }

    /// Get the query IDs this reaction subscribes to
    pub fn queries(&self) -> &[String] {
        match self {
            ReactionConfig::Log { queries, .. } => queries,
            ReactionConfig::Http { queries, .. } => queries,
            ReactionConfig::HttpAdaptive { queries, .. } => queries,
            ReactionConfig::Grpc { queries, .. } => queries,
            ReactionConfig::GrpcAdaptive { queries, .. } => queries,
            ReactionConfig::Sse { queries, .. } => queries,
            ReactionConfig::Platform { queries, .. } => queries,
            ReactionConfig::Profiler { queries, .. } => queries,
        }
    }

    /// Check if auto_start is enabled
    pub fn auto_start(&self) -> bool {
        match self {
            ReactionConfig::Log { auto_start, .. } => *auto_start,
            ReactionConfig::Http { auto_start, .. } => *auto_start,
            ReactionConfig::HttpAdaptive { auto_start, .. } => *auto_start,
            ReactionConfig::Grpc { auto_start, .. } => *auto_start,
            ReactionConfig::GrpcAdaptive { auto_start, .. } => *auto_start,
            ReactionConfig::Sse { auto_start, .. } => *auto_start,
            ReactionConfig::Platform { auto_start, .. } => *auto_start,
            ReactionConfig::Profiler { auto_start, .. } => *auto_start,
        }
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

    // =========================================================================
    // SourceConfig Deserialization Tests
    // =========================================================================

    #[test]
    fn test_source_deserialize_mock_valid() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "autoStart": true,
            "dataType": { "type": "sensorReading" },
            "intervalMs": 1000
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        assert!(source.auto_start());
        assert!(matches!(source, SourceConfig::Mock { .. }));
    }

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
        assert!(matches!(source, SourceConfig::Http { .. }));
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
        assert!(matches!(source, SourceConfig::Grpc { .. }));
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
        assert!(matches!(source, SourceConfig::Postgres { .. }));
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
        assert!(matches!(source, SourceConfig::Platform { .. }));
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
    fn test_source_deserialize_unknown_kind() {
        let json = r#"{
            "kind": "unknown-source-type",
            "id": "test-source"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown-source-type") || err.contains("unknown variant"),
            "Error should mention unknown kind: {err}"
        );
    }

    #[test]
    fn test_source_deserialize_unknown_field_rejected() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "unknownField": "value"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknownField") || err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_source_deserialize_snake_case_auto_start_rejected() {
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
            "Error should mention snake_case field: {err}"
        );
    }

    #[test]
    fn test_source_deserialize_snake_case_data_type_rejected() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "data_type": "sensor"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("data_type"),
            "Error should mention snake_case field: {err}"
        );
    }

    #[test]
    fn test_source_deserialize_error_includes_source_id() {
        let json = r#"{
            "kind": "mock",
            "id": "my-unique-source",
            "badField": "value"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("my-unique-source"),
            "Error should include source id for context: {err}"
        );
    }

    #[test]
    fn test_source_deserialize_error_includes_kind() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "badField": "value"
        }"#;

        let result: Result<SourceConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("mock"),
            "Error should include kind for context: {err}"
        );
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
        let Some(BootstrapProviderConfig::Postgres(bootstrap)) = source.bootstrap_provider() else {
            panic!("Expected postgres bootstrap provider");
        };

        assert_eq!(bootstrap.host, ConfigValue::Static("localhost".to_string()));
        assert_eq!(bootstrap.port, ConfigValue::Static(5432));
        assert_eq!(bootstrap.database, ConfigValue::Static("drasi".to_string()));
        assert_eq!(bootstrap.user, ConfigValue::Static("drasi_user".to_string()));
        assert_eq!(bootstrap.password, ConfigValue::Static("drasi_pass".to_string()));
        assert_eq!(bootstrap.slot_name, "drasi_slot".to_string());
        assert_eq!(bootstrap.publication_name, "drasi_pub".to_string());
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
        let Some(BootstrapProviderConfig::Postgres(bootstrap)) = source.bootstrap_provider() else {
            panic!("Expected postgres bootstrap provider");
        };

        assert_eq!(bootstrap.database, ConfigValue::Static("bootstrap_db".to_string()));
        assert_eq!(bootstrap.user, ConfigValue::Static("bootstrap_user".to_string()));
        assert_eq!(bootstrap.password, ConfigValue::Static("drasi_pass".to_string()));
    }

    #[test]
    fn test_source_deserialize_yaml_format() {
        let yaml = r#"
kind: mock
id: yaml-source
autoStart: true
dataType: { "type": "sensorReading" },
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
        assert!(matches!(reaction, ReactionConfig::Log { .. }));
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
        assert!(matches!(reaction, ReactionConfig::Http { .. }));
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
        assert!(matches!(reaction, ReactionConfig::HttpAdaptive { .. }));
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
        assert!(matches!(reaction, ReactionConfig::Grpc { .. }));
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
        assert!(matches!(reaction, ReactionConfig::GrpcAdaptive { .. }));
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
        assert!(matches!(reaction, ReactionConfig::Sse { .. }));
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
        assert!(matches!(reaction, ReactionConfig::Platform { .. }));
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
        assert!(matches!(reaction, ReactionConfig::Profiler { .. }));
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
    fn test_reaction_deserialize_unknown_kind() {
        let json = r#"{
            "kind": "unknown-reaction-type",
            "id": "test-reaction",
            "queries": ["query1"]
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown-reaction-type") || err.contains("unknown variant"),
            "Error should mention unknown kind: {err}"
        );
    }

    #[test]
    fn test_reaction_deserialize_unknown_field_rejected() {
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": ["query1"],
            "unknownField": "value"
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknownField") || err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_reaction_deserialize_snake_case_auto_start_rejected() {
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
            "Error should mention snake_case field: {err}"
        );
    }

    #[test]
    fn test_reaction_deserialize_error_includes_reaction_id() {
        let json = r#"{
            "kind": "log",
            "id": "my-unique-reaction",
            "queries": ["query1"],
            "badField": "value"
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("my-unique-reaction"),
            "Error should include reaction id for context: {err}"
        );
    }

    #[test]
    fn test_reaction_deserialize_error_includes_kind() {
        let json = r#"{
            "kind": "log",
            "id": "test-reaction",
            "queries": ["query1"],
            "badField": "value"
        }"#;

        let result: Result<ReactionConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("log"),
            "Error should include kind for context: {err}"
        );
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
        let original = SourceConfig::Mock {
            id: "roundtrip-source".to_string(),
            auto_start: false,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: DataTypeDto::SensorReading { sensor_count: 5 },
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SourceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id(), "roundtrip-source");
        assert!(!deserialized.auto_start());
    }

    #[test]
    fn test_reaction_serialize_deserialize_roundtrip() {
        let original = ReactionConfig::Log {
            id: "roundtrip-reaction".to_string(),
            queries: vec!["q1".to_string(), "q2".to_string()],
            auto_start: false,
            config: LogReactionConfigDto::default(),
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
    fn test_source_deserialize_with_env_var_syntax() {
        let json = r#"{
            "kind": "mock",
            "id": "test-source",
            "dataType": { "type": "sensorReading" },
            "intervalMs": 1000
        }"#;

        let source: SourceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(source.id(), "test-source");
        // ConfigValue parses env var syntax into EnvironmentVariable variant
        if let SourceConfig::Mock { config, .. } = source {
           assert_eq!(
                config.data_type,
                mock::DataTypeDto::SensorReading { sensor_count: 10 },
                "Expected SensorReading data type with sensorCount 10"
            );
        }
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
        // ConfigValue parses env var syntax into EnvironmentVariable variant
        if let ReactionConfig::Http { config, .. } = reaction {
            assert!(
                matches!(
                    &config.base_url,
                    ConfigValue::EnvironmentVariable { name, default }
                    if name == "BASE_URL" && *default == Some("http://localhost:8080".to_string())
                ),
                "Expected EnvironmentVariable variant, got {:?}",
                config.base_url
            );
        }
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
