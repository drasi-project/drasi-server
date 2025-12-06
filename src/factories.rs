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
use drasi_lib::plugin_core::{Reaction, Source};
use log::info;

use crate::config::{ReactionConfig, SourceConfig};

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
        SourceConfig::Mock { id, config: c, .. } => {
            use drasi_source_mock::MockSource;
            Box::new(MockSource::new(id, c.clone())?)
        }
        SourceConfig::Http { id, config: c, .. } => {
            use drasi_source_http::HttpSource;
            Box::new(HttpSource::new(id, c.clone())?)
        }
        SourceConfig::Grpc { id, config: c, .. } => {
            use drasi_source_grpc::GrpcSource;
            Box::new(GrpcSource::new(id, c.clone())?)
        }
        SourceConfig::Postgres { id, config: c, .. } => {
            use drasi_source_postgres::PostgresReplicationSource;
            Box::new(PostgresReplicationSource::new(id, c.clone())?)
        }
        SourceConfig::Platform { id, config: c, .. } => {
            use drasi_source_platform::PlatformSource;
            Box::new(PlatformSource::new(id, c.clone())?)
        }
    };

    // If a bootstrap provider is configured, create and attach it
    if let Some(bootstrap_config) = config.bootstrap_provider() {
        let provider = create_bootstrap_provider(bootstrap_config, &config)?;
        info!(
            "Setting bootstrap provider for source '{}'",
            config.id()
        );
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
                Ok(Box::new(PostgresBootstrapProvider::new(config.clone())))
            } else {
                Err(anyhow::anyhow!(
                    "Postgres bootstrap provider can only be used with Postgres sources"
                ))
            }
        }
        BootstrapProviderConfig::ScriptFile(script_config) => {
            use drasi_bootstrap_scriptfile::ScriptFileBootstrapProvider;
            Ok(Box::new(ScriptFileBootstrapProvider::new(script_config.clone())))
        }
        BootstrapProviderConfig::Platform(platform_config) => {
            use drasi_bootstrap_platform::PlatformBootstrapProvider;
            Ok(Box::new(PlatformBootstrapProvider::new(platform_config.clone())?))
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
    match config {
        ReactionConfig::Log {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_log::LogReaction;
            Ok(Box::new(LogReaction::new(&id, queries, config)))
        }
        ReactionConfig::Http {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_http::HttpReaction;
            Ok(Box::new(HttpReaction::new(&id, queries, config)))
        }
        ReactionConfig::HttpAdaptive {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_http_adaptive::AdaptiveHttpReaction;
            Ok(Box::new(AdaptiveHttpReaction::new(&id, queries, config)))
        }
        ReactionConfig::Grpc {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_grpc::GrpcReaction;
            Ok(Box::new(GrpcReaction::new(&id, queries, config)))
        }
        ReactionConfig::GrpcAdaptive {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_grpc_adaptive::AdaptiveGrpcReaction;
            Ok(Box::new(AdaptiveGrpcReaction::new(&id, queries, config)))
        }
        ReactionConfig::Sse {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_sse::SseReaction;
            Ok(Box::new(SseReaction::new(&id, queries, config)))
        }
        ReactionConfig::Platform {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_platform::PlatformReaction;
            Ok(Box::new(PlatformReaction::new(&id, queries, config)?))
        }
        ReactionConfig::Profiler {
            id,
            queries,
            config,
            ..
        } => {
            use drasi_reaction_profiler::ProfilerReaction;
            Ok(Box::new(ProfilerReaction::new(&id, queries, config)))
        }
    }
}
