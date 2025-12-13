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

use anyhow::Result;
use drasi_lib::bootstrap::BootstrapProviderConfig;
use drasi_lib::config::DrasiLibConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

// Source plugin configs
use drasi_source_grpc::GrpcSourceConfig;
use drasi_source_http::HttpSourceConfig;
use drasi_source_mock::MockSourceConfig;
use drasi_source_platform::PlatformSourceConfig;
use drasi_source_postgres::PostgresSourceConfig;

// Reaction plugin configs
use drasi_reaction_grpc::GrpcReactionConfig;
use drasi_reaction_grpc_adaptive::GrpcAdaptiveReactionConfig;
use drasi_reaction_http::HttpReactionConfig;
use drasi_reaction_http_adaptive::HttpAdaptiveReactionConfig;
use drasi_reaction_log::LogReactionConfig;
use drasi_reaction_platform::PlatformReactionConfig;
use drasi_reaction_profiler::ProfilerReactionConfig;
use drasi_reaction_sse::SseReactionConfig;

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
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: MockSourceConfig,
    },
    /// HTTP source for receiving events via HTTP endpoints
    #[serde(rename = "http")]
    Http {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: HttpSourceConfig,
    },
    /// gRPC source for receiving events via gRPC streaming
    #[serde(rename = "grpc")]
    Grpc {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: GrpcSourceConfig,
    },
    /// PostgreSQL replication source for CDC
    #[serde(rename = "postgres")]
    Postgres {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: PostgresSourceConfig,
    },
    /// Platform source for Redis Streams consumption
    #[serde(rename = "platform")]
    Platform {
        id: String,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        bootstrap_provider: Option<BootstrapProviderConfig>,
        #[serde(flatten)]
        config: PlatformSourceConfig,
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

/// Reaction configuration with kind discriminator.
///
/// Uses serde tagged enum to automatically deserialize into the correct
/// plugin-specific config struct based on the `kind` field.
///
/// # Example YAML
///
/// ```yaml
/// reactions:
///   - kind: log
///     id: log-reaction
///     queries: [my-query]
///     auto_start: true
///     log_level: info
///
///   - kind: http
///     id: webhook
///     queries: [my-query]
///     base_url: "http://localhost:3000"
/// ```
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
        config: LogReactionConfig,
    },
    /// HTTP reaction for webhooks
    #[serde(rename = "http")]
    Http {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: HttpReactionConfig,
    },
    /// HTTP adaptive reaction with batching
    #[serde(rename = "http-adaptive")]
    HttpAdaptive {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: HttpAdaptiveReactionConfig,
    },
    /// gRPC reaction for streaming results
    #[serde(rename = "grpc")]
    Grpc {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: GrpcReactionConfig,
    },
    /// gRPC adaptive reaction with batching
    #[serde(rename = "grpc-adaptive")]
    GrpcAdaptive {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: GrpcAdaptiveReactionConfig,
    },
    /// SSE reaction for Server-Sent Events
    #[serde(rename = "sse")]
    Sse {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: SseReactionConfig,
    },
    /// Platform reaction for Drasi platform integration
    #[serde(rename = "platform")]
    Platform {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: PlatformReactionConfig,
    },
    /// Profiler reaction for performance analysis
    #[serde(rename = "profiler")]
    Profiler {
        id: String,
        queries: Vec<String>,
        #[serde(default = "default_true")]
        auto_start: bool,
        #[serde(flatten)]
        config: ProfilerReactionConfig,
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

fn default_true() -> bool {
    true
}

/// DrasiServer configuration that composes core config with server settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DrasiServerConfig {
    #[serde(default)]
    pub server: ServerSettings,
    /// Source configurations (DrasiServer-specific, parsed into plugin instances)
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    /// Reaction configurations (DrasiServer-specific, parsed into plugin instances)
    #[serde(default)]
    pub reactions: Vec<ReactionConfig>,
    /// Core configuration (queries, storage backends)
    #[serde(flatten)]
    pub core_config: DrasiLibConfig,
}

/// Server settings for DrasiServer
/// These control DrasiServer's operational behavior including network binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_disable_persistence")]
    pub disable_persistence: bool,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            disable_persistence: false,
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_disable_persistence() -> bool {
    false
}

/// Validate hostname format according to RFC 1123
/// Hostnames can contain:
/// - letters (a-z, A-Z)
/// - digits (0-9)
/// - hyphens (-)
/// - dots (.) as separators
///
/// Each label (part between dots) must:
/// - start and end with alphanumeric character
/// - be 1-63 characters long
///
/// Total length must be <= 253 characters
fn is_valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 253 {
        return false;
    }

    // Special case: wildcard hostname for binding to all interfaces
    if hostname == "*" {
        return true;
    }

    // Split by dots and validate each label
    let labels: Vec<&str> = hostname.split('.').collect();

    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        // Check if label starts and ends with alphanumeric
        let chars: Vec<char> = label.chars().collect();
        if !chars[0].is_ascii_alphanumeric() || !chars[chars.len() - 1].is_ascii_alphanumeric() {
            return false;
        }

        // Check all characters are valid
        for c in chars {
            if !c.is_ascii_alphanumeric() && c != '-' {
                return false;
            }
        }
    }

    true
}

impl DrasiServerConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let content = fs::read_to_string(path_ref).map_err(|e| {
            anyhow::anyhow!("Failed to read config file {}: {}", path_ref.display(), e)
        })?;

        // Try YAML first, then JSON
        match serde_yaml::from_str::<DrasiServerConfig>(&content) {
            Ok(config) => Ok(config),
            Err(yaml_err) => {
                // If YAML fails, try JSON
                match serde_json::from_str::<DrasiServerConfig>(&content) {
                    Ok(config) => Ok(config),
                    Err(json_err) => {
                        // Both failed, return detailed error
                        Err(anyhow::anyhow!(
                            "Failed to parse config file '{}':\n  YAML error: {}\n  JSON error: {}",
                            path_ref.display(),
                            yaml_err,
                            json_err
                        ))
                    }
                }
            }
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // Validate wrapper-specific settings
        if self.server.port == 0 {
            return Err(anyhow::anyhow!(
                "Invalid server port: {} (cannot be 0)",
                self.server.port
            ));
        }

        if self.server.host.is_empty() {
            return Err(anyhow::anyhow!("Server host cannot be empty"));
        }

        // Validate host format (IP address or hostname)
        // Special cases: localhost, 0.0.0.0 (all interfaces)
        if self.server.host != "localhost"
            && self.server.host != "0.0.0.0"
            && IpAddr::from_str(&self.server.host).is_err()
            && !is_valid_hostname(&self.server.host)
        {
            return Err(anyhow::anyhow!(
                "Invalid server host '{}': must be a valid IP address or hostname",
                self.server.host
            ));
        }

        // Delegate core configuration validation to Core
        self.core_config.validate()
    }
}
