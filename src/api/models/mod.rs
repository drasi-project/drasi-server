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

use serde::{Deserialize, Serialize};

// Config value module
pub mod config_value;

// Organized submodules
pub mod queries;
pub mod reactions;
pub mod sources;

// Re-export all DTO types for convenient access
pub use config_value::*;
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
/// Uses serde tagged enum to automatically deserialize into the correct
/// plugin-specific config struct based on the `kind` field.
///
/// # Example YAML
///
/// ```yaml
/// sources:
///   - kind: mock
///     id: test-source
///     auto_start: true
///     data_type: sensor
///     interval_ms: 1000
///
///   - kind: http
///     id: http-source
///     host: "0.0.0.0"
///     port: 9000
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SourceConfig {
    /// Mock source for testing
    #[serde(rename = "mock")]
    Mock {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<drasi_lib::bootstrap::BootstrapProviderConfig>,
        #[serde(flatten)]
        config: MockSourceConfigDto,
    },
    /// HTTP source for receiving events via HTTP endpoints
    #[serde(rename = "http")]
    Http {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<drasi_lib::bootstrap::BootstrapProviderConfig>,
        #[serde(flatten)]
        config: HttpSourceConfigDto,
    },
    /// gRPC source for receiving events via gRPC streaming
    #[serde(rename = "grpc")]
    Grpc {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<drasi_lib::bootstrap::BootstrapProviderConfig>,
        #[serde(flatten)]
        config: GrpcSourceConfigDto,
    },
    /// PostgreSQL replication source for CDC
    #[serde(rename = "postgres")]
    Postgres {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<drasi_lib::bootstrap::BootstrapProviderConfig>,
        #[serde(flatten)]
        config: PostgresSourceConfigDto,
    },
    /// Platform source for Redis Streams consumption
    #[serde(rename = "platform")]
    Platform {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<drasi_lib::bootstrap::BootstrapProviderConfig>,
        #[serde(flatten)]
        config: PlatformSourceConfigDto,
    },
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
    pub fn bootstrap_provider(&self) -> Option<&drasi_lib::bootstrap::BootstrapProviderConfig> {
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

/// Reaction configuration with kind discriminator.
///
/// Similar to SourceConfig, uses serde tagged enum for type-safe deserialization
/// of different reaction types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ReactionConfig {
    /// Log reaction for console output
    #[serde(rename = "log")]
    Log {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: LogReactionConfigDto,
    },
    /// HTTP reaction for webhooks
    #[serde(rename = "http")]
    Http {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: HttpReactionConfigDto,
    },
    /// HTTP adaptive reaction with batching
    #[serde(rename = "http-adaptive")]
    HttpAdaptive {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: HttpAdaptiveReactionConfigDto,
    },
    /// gRPC reaction for streaming results
    #[serde(rename = "grpc")]
    Grpc {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: GrpcReactionConfigDto,
    },
    /// gRPC adaptive reaction with batching
    #[serde(rename = "grpc-adaptive")]
    GrpcAdaptive {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: GrpcAdaptiveReactionConfigDto,
    },
    /// SSE reaction for Server-Sent Events
    #[serde(rename = "sse")]
    Sse {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: SseReactionConfigDto,
    },
    /// Platform reaction for Drasi platform integration
    #[serde(rename = "platform")]
    Platform {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: PlatformReactionConfigDto,
    },
    /// Profiler reaction for performance analysis
    #[serde(rename = "profiler")]
    Profiler {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: ProfilerReactionConfigDto,
    },
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
/// Uses serde tagged enum to automatically deserialize into the correct
/// provider-specific config struct based on the `kind` field.
///
/// # Example YAML
///
/// ```yaml
/// state_store:
///   kind: redb
///   path: ./data/state.redb
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
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
