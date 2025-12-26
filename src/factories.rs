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

//! Factory functions for creating source and reaction instances from config.
//!
//! This module provides factory functions that match on the tagged enum config
//! types and use the existing plugin constructors to create instances.

use anyhow::Result;
use drasi_lib::bootstrap::BootstrapProviderConfig;
use drasi_lib::plugin_core::{Reaction, Source, StateStoreProvider};
use drasi_state_store_redb::RedbStateStoreProvider;
use log::info;
use std::path::PathBuf;
use std::sync::Arc;

use crate::api::mappings::{
    ConfigMapper,
    DtoMapper,
    GrpcAdaptiveReactionConfigMapper,
    GrpcReactionConfigMapper,
    GrpcSourceConfigMapper,
    HttpAdaptiveReactionConfigMapper,
    // Reaction mappers
    HttpReactionConfigMapper,
    HttpSourceConfigMapper,
    LogReactionConfigMapper,
    MockSourceConfigMapper,
    PlatformReactionConfigMapper,
    PlatformSourceConfigMapper,
    // Source mappers
    PostgresConfigMapper,
    ProfilerReactionConfigMapper,
    SseReactionConfigMapper,
};
use crate::api::models::StateStoreConfig;
use crate::config::{ReactionConfig, SourceConfig};

/// Sanitize an instance ID for use in file paths.
///
/// This prevents directory traversal attacks by replacing dangerous characters.
/// Uses the same sanitization pattern as persist_index path generation.
fn sanitize_instance_id(id: &str) -> String {
    id.replace(['/', '\\', '\0'], "_")
        .replace("..", "_")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

/// Create a state store provider from configuration.
///
/// This function matches on the state store config variant and creates the appropriate
/// provider. Currently supports:
/// - `redb` - Redb-based persistent state store
///
/// # Arguments
///
/// * `config` - The state store configuration
/// * `instance_id` - The DrasiLib instance ID (used for default path generation)
///
/// # Returns
///
/// An Arc-wrapped StateStoreProvider trait object
///
/// # Example
///
/// ```rust,ignore
/// use drasi_server::factories::create_state_store_provider;
/// use drasi_server::api::models::StateStoreConfig;
///
/// let config = StateStoreConfig::Redb { path: Some("./data/state.redb".to_string()) };
/// let provider = create_state_store_provider(&config, "my-instance")?;
/// ```
pub fn create_state_store_provider(
    config: &StateStoreConfig,
    instance_id: &str,
) -> Result<Arc<dyn StateStoreProvider>> {
    match config {
        StateStoreConfig::Redb { path } => {
            // Use provided path or generate default based on instance_id
            let store_path = path.clone().unwrap_or_else(|| {
                let safe_id = sanitize_instance_id(instance_id);
                format!("./data/{safe_id}/state.redb")
            });

            // Ensure parent directory exists
            if let Some(parent) = PathBuf::from(&store_path).parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to create state store directory '{}': {e}",
                        parent.display()
                    )
                })?;
            }

            info!(
                "Creating Redb state store provider at: {store_path} for instance '{instance_id}'"
            );

            let provider = RedbStateStoreProvider::new(&store_path).map_err(|e| {
                anyhow::anyhow!("Failed to create Redb state store provider at '{store_path}': {e}")
            })?;

            Ok(Arc::new(provider))
        }
    }
}

/// Create a source instance from a SourceConfig.
///
/// This function matches on the config variant and creates the appropriate
/// source type using the plugin's constructor. If a bootstrap provider is
/// configured, it will also be created and attached to the source.
///
/// # Arguments
///
/// * `config` - The source configuration
///
/// # Returns
///
/// A boxed Source trait object
///
/// # Example
///
/// ```rust,ignore
/// use drasi_server::config::SourceConfig;
/// use drasi_server::factories::create_source;
///
/// let config = SourceConfig::Mock {
///     id: "test-source".to_string(),
///     auto_start: true,
///     bootstrap_provider: None,
///     config: MockSourceConfig::default(),
/// };
///
/// let source = create_source(config).await?;
/// ```
pub async fn create_source(config: SourceConfig) -> Result<Box<dyn Source + 'static>> {
    let source: Box<dyn Source + 'static> = match &config {
        SourceConfig::Mock {
            id,
            auto_start,
            config: c,
            ..
        } => {
            use drasi_source_mock::MockSourceBuilder;
            let mapper = DtoMapper::new();
            let mock_mapper = MockSourceConfigMapper;
            let domain_config = mock_mapper.map(c, &mapper)?;
            Box::new(
                MockSourceBuilder::new(id)
                    .with_data_type(&domain_config.data_type)
                    .with_interval_ms(domain_config.interval_ms)
                    .with_auto_start(*auto_start)
                    .build()?,
            )
        }
        SourceConfig::Http {
            id,
            auto_start,
            config: c,
            ..
        } => {
            use drasi_source_http::HttpSourceBuilder;
            let mapper = DtoMapper::new();
            let http_mapper = HttpSourceConfigMapper;
            let domain_config = http_mapper.map(c, &mapper)?;
            Box::new(
                HttpSourceBuilder::new(id)
                    .with_config(domain_config)
                    .with_auto_start(*auto_start)
                    .build()?,
            )
        }
        SourceConfig::Grpc {
            id,
            auto_start,
            config: c,
            ..
        } => {
            use drasi_source_grpc::GrpcSourceBuilder;
            let mapper = DtoMapper::new();
            let grpc_mapper = GrpcSourceConfigMapper;
            let domain_config = grpc_mapper.map(c, &mapper)?;
            Box::new(
                GrpcSourceBuilder::new(id)
                    .with_config(domain_config)
                    .with_auto_start(*auto_start)
                    .build()?,
            )
        }
        SourceConfig::Postgres {
            id,
            auto_start,
            config: c,
            ..
        } => {
            use drasi_source_postgres::PostgresSourceBuilder;
            let mapper = DtoMapper::new();
            let postgres_mapper = PostgresConfigMapper;
            let domain_config = postgres_mapper.map(c, &mapper)?;
            Box::new(
                PostgresSourceBuilder::new(id)
                    .with_config(domain_config)
                    .with_auto_start(*auto_start)
                    .build()?,
            )
        }
        SourceConfig::Platform {
            id,
            auto_start,
            config: c,
            ..
        } => {
            use drasi_source_platform::PlatformSourceBuilder;
            let mapper = DtoMapper::new();
            let platform_mapper = PlatformSourceConfigMapper;
            let domain_config = platform_mapper.map(c, &mapper)?;
            Box::new(
                PlatformSourceBuilder::new(id)
                    .with_config(domain_config)
                    .with_auto_start(*auto_start)
                    .build()?,
            )
        }
    };

    // If a bootstrap provider is configured, create and attach it
    if let Some(bootstrap_config) = config.bootstrap_provider() {
        let provider = create_bootstrap_provider(bootstrap_config, &config)?;
        info!("Setting bootstrap provider for source '{}'", config.id());
        source.set_bootstrap_provider(provider).await;
    }

    Ok(source)
}

/// Create a bootstrap provider from configuration.
///
/// This function creates the appropriate bootstrap provider based on the config type.
fn create_bootstrap_provider(
    bootstrap_config: &BootstrapProviderConfig,
    source_config: &SourceConfig,
) -> Result<Box<dyn drasi_lib::bootstrap::BootstrapProvider + 'static>> {
    match bootstrap_config {
        BootstrapProviderConfig::Postgres(_) => {
            // Postgres bootstrap provider needs the source's postgres config
            if let SourceConfig::Postgres { config, .. } = source_config {
                use drasi_bootstrap_postgres::PostgresBootstrapProvider;
                let mapper = DtoMapper::new();
                let postgres_mapper = PostgresConfigMapper;
                let domain_config = postgres_mapper.map(config, &mapper)?;
                Ok(Box::new(PostgresBootstrapProvider::new(domain_config)))
            } else {
                Err(anyhow::anyhow!(
                    "Postgres bootstrap provider can only be used with Postgres sources"
                ))
            }
        }
        BootstrapProviderConfig::ScriptFile(script_config) => {
            use drasi_bootstrap_scriptfile::ScriptFileBootstrapProvider;
            Ok(Box::new(ScriptFileBootstrapProvider::new(
                script_config.clone(),
            )))
        }
        BootstrapProviderConfig::Platform(platform_config) => {
            use drasi_bootstrap_platform::PlatformBootstrapProvider;
            Ok(Box::new(PlatformBootstrapProvider::new(
                platform_config.clone(),
            )?))
        }
        BootstrapProviderConfig::Application(_) => {
            // Application bootstrap is typically handled internally by application sources
            Err(anyhow::anyhow!(
                "Application bootstrap provider is managed internally by application sources"
            ))
        }
        BootstrapProviderConfig::Noop => {
            use drasi_bootstrap_noop::NoOpBootstrapProvider;
            Ok(Box::new(NoOpBootstrapProvider::new()))
        }
    }
}

/// Create a reaction instance from a ReactionConfig.
///
/// This function matches on the config variant and creates the appropriate
/// reaction type using the plugin's constructor.
///
/// # Arguments
///
/// * `config` - The reaction configuration
///
/// # Returns
///
/// A boxed Reaction trait object
///
/// # Example
///
/// ```rust,ignore
/// use drasi_server::config::ReactionConfig;
/// use drasi_server::factories::create_reaction;
///
/// let config = ReactionConfig::Log {
///     id: "log-reaction".to_string(),
///     queries: vec!["my-query".to_string()],
///     auto_start: true,
///     config: LogReactionConfig::default(),
/// };
///
/// let reaction = create_reaction(config)?;
/// ```
pub fn create_reaction(config: ReactionConfig) -> Result<Box<dyn Reaction + 'static>> {
    let mapper = DtoMapper::new();

    match config {
        ReactionConfig::Log {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_log::LogReactionBuilder;
            let log_mapper = LogReactionConfigMapper;
            let domain_config = log_mapper.map(&config, &mapper)?;

            let mut builder = LogReactionBuilder::new(&id)
                .with_queries(queries)
                .with_auto_start(auto_start);
            if let Some(template) = domain_config.default_template {
                builder = builder.with_default_template(template);
            }
            for (query_id, route_config) in domain_config.routes {
                builder = builder.with_route(query_id, route_config);
            }
            Ok(Box::new(builder.build()?))
        }
        ReactionConfig::Http {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_http::HttpReactionBuilder;
            let http_mapper = HttpReactionConfigMapper;
            let domain_config = http_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                HttpReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
        ReactionConfig::HttpAdaptive {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_http_adaptive::HttpAdaptiveReactionBuilder;
            let http_adaptive_mapper = HttpAdaptiveReactionConfigMapper;
            let domain_config = http_adaptive_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                HttpAdaptiveReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
        ReactionConfig::Grpc {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_grpc::GrpcReactionBuilder;
            let grpc_mapper = GrpcReactionConfigMapper;
            let domain_config = grpc_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                GrpcReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
        ReactionConfig::GrpcAdaptive {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_grpc_adaptive::GrpcAdaptiveReactionBuilder;
            let grpc_adaptive_mapper = GrpcAdaptiveReactionConfigMapper;
            let domain_config = grpc_adaptive_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                GrpcAdaptiveReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
        ReactionConfig::Sse {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_sse::SseReactionBuilder;
            let sse_mapper = SseReactionConfigMapper;
            let domain_config = sse_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                SseReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
        ReactionConfig::Platform {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_platform::PlatformReactionBuilder;
            let platform_mapper = PlatformReactionConfigMapper;
            let domain_config = platform_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                PlatformReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
        ReactionConfig::Profiler {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_profiler::ProfilerReactionBuilder;
            let profiler_mapper = ProfilerReactionConfigMapper;
            let domain_config = profiler_mapper.map(&config, &mapper)?;
            Ok(Box::new(
                ProfilerReactionBuilder::new(&id)
                    .with_queries(queries)
                    .with_auto_start(auto_start)
                    .with_config(domain_config)
                    .build()?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_instance_id_normal() {
        assert_eq!(sanitize_instance_id("my-instance"), "my-instance");
        assert_eq!(sanitize_instance_id("instance_123"), "instance_123");
    }

    #[test]
    fn test_sanitize_instance_id_path_traversal() {
        // "../etc/passwd" -> replace `/` with `_` -> ".._etc_passwd" -> replace ".." with "_" -> "__etc_passwd"
        assert_eq!(sanitize_instance_id("../etc/passwd"), "__etc_passwd");
        assert_eq!(
            sanitize_instance_id("..\\windows\\system32"),
            "__windows_system32"
        );
        // "foo/../bar" -> replace `/` with `_` -> "foo_.._bar" -> replace ".." with "_" -> "foo___bar"
        assert_eq!(sanitize_instance_id("foo/../bar"), "foo___bar");
    }

    #[test]
    fn test_sanitize_instance_id_slashes() {
        assert_eq!(sanitize_instance_id("foo/bar"), "foo_bar");
        assert_eq!(sanitize_instance_id("foo\\bar"), "foo_bar");
        assert_eq!(sanitize_instance_id("foo/bar\\baz"), "foo_bar_baz");
    }

    #[test]
    fn test_sanitize_instance_id_control_chars() {
        assert_eq!(sanitize_instance_id("foo\0bar"), "foo_bar");
        assert_eq!(sanitize_instance_id("foo\nbar"), "foobar");
        assert_eq!(sanitize_instance_id("foo\tbar"), "foobar");
    }

    #[test]
    fn test_sanitize_instance_id_empty() {
        assert_eq!(sanitize_instance_id(""), "");
    }
}
