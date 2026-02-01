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

//! Factory functions for creating source, reaction, and state store instances from config.
//!
//! This module provides factory functions that match on the tagged enum config
//! types and use the existing plugin constructors to create instances.

use anyhow::Result;
use drasi_lib::identity::IdentityProvider;
use drasi_lib::state_store::StateStoreProvider;
use drasi_lib::{Reaction, Source};
use log::info;
use std::collections::HashMap;
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
    MsSqlStoredProcReactionConfigMapper,
    MySqlStoredProcReactionConfigMapper,
    PlatformReactionConfigMapper,
    PlatformSourceConfigMapper,
    PostgresStoredProcReactionConfigMapper,
    // Source mappers
    PostgresConfigMapper,
    ProfilerReactionConfigMapper,
    SseReactionConfigMapper,
};
use crate::api::models::{BootstrapProviderConfig, IdentityProviderConfig};
use crate::config::{ReactionConfig, SourceConfig, StateStoreConfig};

/// Create identity provider instances from configuration.
///
/// This function processes identity provider configurations and creates a map
/// of ID -> IdentityProvider instances that can be looked up by sources and reactions.
///
/// # Arguments
///
/// * `configs` - Vector of identity provider configurations
///
/// # Returns
///
/// A HashMap mapping provider IDs to boxed IdentityProvider trait objects
pub async fn create_identity_providers(
    configs: &[IdentityProviderConfig],
) -> Result<HashMap<String, Box<dyn IdentityProvider>>> {
    let mapper = DtoMapper::new();
    let mut providers = HashMap::new();

    for config in configs {
        match config {
            IdentityProviderConfig::Password { id, config } => {
                use drasi_lib::identity::PasswordIdentityProvider;
                let username = mapper.resolve_string(&config.username)?;
                let password = mapper.resolve_string(&config.password)?;
                let provider = PasswordIdentityProvider::new(&username, &password);
                providers.insert(id.clone(), Box::new(provider) as Box<dyn IdentityProvider>);
            }
            #[cfg(feature = "azure-identity")]
            IdentityProviderConfig::Azure { id, config } => {
                use crate::api::models::AzureAuthenticationMode;
                use drasi_lib::identity::AzureIdentityProvider;

                let username = mapper.resolve_string(&config.username)?;
                let scope = config
                    .scope
                    .as_ref()
                    .map(|s| mapper.resolve_string(s))
                    .transpose()?
                    .unwrap_or_else(|| {
                        "https://ossrdbms-aad.database.windows.net/.default".to_string()
                    });

                let provider_builder = match config.authentication_mode {
                    AzureAuthenticationMode::WorkloadIdentity => {
                        AzureIdentityProvider::with_workload_identity(&username)?
                    }
                    AzureAuthenticationMode::ManagedIdentity => {
                        // Managed identity requires a client ID for user-assigned identities
                        let client_id = config
                            .client_id
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!(
                                "client_id is required for managedIdentity authentication mode"
                            ))?;
                        let client_id_str = mapper.resolve_string(client_id)?;
                        AzureIdentityProvider::with_managed_identity(&username, &client_id_str)?
                    }
                    AzureAuthenticationMode::DefaultCredentials => {
                        AzureIdentityProvider::with_default_credentials(&username)?
                    }
                };

                let provider_builder = provider_builder.with_scope(&scope);

                providers.insert(id.clone(), Box::new(provider_builder) as Box<dyn IdentityProvider>);
            }
            #[cfg(feature = "aws-identity")]
            IdentityProviderConfig::Aws { id, config } => {
                use crate::api::models::AwsAuthenticationMode;
                use drasi_lib::identity::AwsIdentityProvider;

                let username = mapper.resolve_string(&config.username)?;
                let hostname = mapper.resolve_string(&config.hostname)?;
                let port = mapper.resolve_typed(&config.port)?;
                let region = mapper.resolve_string(&config.region)?;

                let provider = match config.authentication_mode {
                    AwsAuthenticationMode::DefaultCredentials => {
                        AwsIdentityProvider::with_region(&username, &hostname, port, &region).await?
                    }
                    AwsAuthenticationMode::AssumeRole => {
                        let role_arn = config
                            .role_arn
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!(
                                "roleArn is required when authenticationMode is assumeRole"
                            ))?;
                        let role_arn_str = mapper.resolve_string(role_arn)?;

                        let session_name = config
                            .session_name
                            .as_ref()
                            .map(|s| mapper.resolve_string(s))
                            .transpose()?;

                        AwsIdentityProvider::with_assumed_role(
                            &username,
                            &hostname,
                            port,
                            &role_arn_str,
                            session_name.as_deref()
                        ).await?
                    }
                };

                providers.insert(id.clone(), Box::new(provider) as Box<dyn IdentityProvider>);
            }
        }
    }

    Ok(providers)
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
                use drasi_bootstrap_postgres::{PostgresBootstrapConfig, PostgresBootstrapProvider};
                let mapper = DtoMapper::new();
                let postgres_mapper = PostgresConfigMapper;
                let source_config = postgres_mapper.map(config, &mapper)?;

                // Convert PostgresSourceConfig to PostgresBootstrapConfig
                // They have identical structures, just different types
                let bootstrap_config = PostgresBootstrapConfig {
                    host: source_config.host,
                    port: source_config.port,
                    database: source_config.database,
                    user: source_config.user,
                    password: source_config.password,
                    tables: source_config.tables,
                    slot_name: source_config.slot_name,
                    publication_name: source_config.publication_name,
                    ssl_mode: match source_config.ssl_mode {
                        drasi_source_postgres::SslMode::Disable => drasi_bootstrap_postgres::SslMode::Disable,
                        drasi_source_postgres::SslMode::Prefer => drasi_bootstrap_postgres::SslMode::Prefer,
                        drasi_source_postgres::SslMode::Require => drasi_bootstrap_postgres::SslMode::Require,
                    },
                    table_keys: source_config.table_keys.into_iter().map(|tk| drasi_bootstrap_postgres::TableKeyConfig {
                        table: tk.table,
                        key_columns: tk.key_columns,
                    }).collect(),
                };

                Ok(Box::new(PostgresBootstrapProvider::new(bootstrap_config)))
            } else {
                Err(anyhow::anyhow!(
                    "Postgres bootstrap provider can only be used with Postgres sources"
                ))
            }
        }
        BootstrapProviderConfig::ScriptFile(script_config) => {
            use drasi_bootstrap_scriptfile::ScriptFileBootstrapProvider;
            // Convert local DTO to drasi-lib type
            let lib_config: drasi_lib::bootstrap::ScriptFileBootstrapConfig = script_config.into();
            Ok(Box::new(ScriptFileBootstrapProvider::new(lib_config)))
        }
        BootstrapProviderConfig::Platform(platform_config) => {
            use drasi_bootstrap_platform::PlatformBootstrapProvider;
            // Convert local DTO to drasi-lib type
            let lib_config: drasi_lib::bootstrap::PlatformBootstrapConfig = platform_config.into();
            Ok(Box::new(PlatformBootstrapProvider::new(lib_config)?))
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
/// let reaction = create_reaction(config, None).await?;
/// ```
pub async fn create_reaction(
    config: ReactionConfig,
    identity_providers: Option<&HashMap<String, Box<dyn IdentityProvider>>>,
) -> Result<Box<dyn Reaction + 'static>> {
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
        ReactionConfig::StoredprocPostgres {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_storedproc_postgres::PostgresStoredProcReaction;
            let postgres_mapper = PostgresStoredProcReactionConfigMapper;
            let mut domain_config = postgres_mapper.map(&config, &mapper)?;

            // Set identity provider if specified
            if let Some(provider_id) = &config.identity_provider_id {
                let providers = identity_providers
                    .ok_or_else(|| anyhow::anyhow!("Identity provider '{}' specified but no providers available", provider_id))?;
                let provider = providers
                    .get(provider_id)
                    .ok_or_else(|| anyhow::anyhow!("Identity provider '{}' not found", provider_id))?;

                // Set the identity provider on the config directly
                domain_config.identity_provider = Some(provider.clone());
            }

            let mut builder = PostgresStoredProcReaction::builder(&id)
                .with_queries(queries)
                .with_auto_start(auto_start)
                .with_config(domain_config);

            Ok(Box::new(builder.build().await?))
        }
        ReactionConfig::StoredprocMysql {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_storedproc_mysql::MySqlStoredProcReaction;
            let mysql_mapper = MySqlStoredProcReactionConfigMapper;
            let mut domain_config = mysql_mapper.map(&config, &mapper)?;

            // Set identity provider if specified
            if let Some(provider_id) = &config.identity_provider_id {
                let providers = identity_providers
                    .ok_or_else(|| anyhow::anyhow!("Identity provider '{}' specified but no providers available", provider_id))?;
                let provider = providers
                    .get(provider_id)
                    .ok_or_else(|| anyhow::anyhow!("Identity provider '{}' not found", provider_id))?;

                // Set the identity provider on the config directly
                domain_config.identity_provider = Some(provider.clone());
            }

            let mut builder = MySqlStoredProcReaction::builder(&id)
                .with_queries(queries)
                .with_auto_start(auto_start)
                .with_config(domain_config);

            Ok(Box::new(builder.build().await?))
        }
        ReactionConfig::StoredprocMssql {
            id,
            queries,
            auto_start,
            config,
        } => {
            use drasi_reaction_storedproc_mssql::MsSqlStoredProcReaction;
            let mssql_mapper = MsSqlStoredProcReactionConfigMapper;
            let mut domain_config = mssql_mapper.map(&config, &mapper)?;

            // Set identity provider if specified
            if let Some(provider_id) = &config.identity_provider_id {
                let providers = identity_providers
                    .ok_or_else(|| anyhow::anyhow!("Identity provider '{}' specified but no providers available", provider_id))?;
                let provider = providers
                    .get(provider_id)
                    .ok_or_else(|| anyhow::anyhow!("Identity provider '{}' not found", provider_id))?;

                // Set the identity provider on the config directly
                domain_config.identity_provider = Some(provider.clone());
            }

            let mut builder = MsSqlStoredProcReaction::builder(&id)
                .with_queries(queries)
                .with_auto_start(auto_start)
                .with_config(domain_config);

            Ok(Box::new(builder.build().await?))
        }
    }
}

/// Create a state store provider from a StateStoreConfig.
///
/// This function matches on the config variant and creates the appropriate
/// state store provider type using the plugin's constructor.
///
/// # Arguments
///
/// * `config` - The state store configuration
///
/// # Returns
///
/// An Arc-wrapped StateStoreProvider trait object
///
/// # Example
///
/// ```rust,ignore
/// use drasi_server::config::StateStoreConfig;
/// use drasi_server::factories::create_state_store_provider;
///
/// let config = StateStoreConfig::Redb {
///     path: ConfigValue::Static("./data/state.redb".to_string()),
/// };
///
/// let provider = create_state_store_provider(config)?;
/// ```
pub fn create_state_store_provider(
    config: StateStoreConfig,
) -> Result<Arc<dyn StateStoreProvider + Send + Sync + 'static>> {
    let mapper = DtoMapper::new();

    match config {
        StateStoreConfig::Redb { path } => {
            use drasi_state_store_redb::RedbStateStoreProvider;

            let resolved_path: String = mapper.resolve_typed(&path)?;
            info!("Creating REDB state store provider with path: {resolved_path}");

            let provider = RedbStateStoreProvider::new(&resolved_path)?;
            Ok(Arc::new(provider))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConfigValue, LogReactionConfigDto, MockSourceConfigDto};
    use tempfile::TempDir;

    // ==========================================================================
    // Source Factory Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_create_mock_source() {
        let config = SourceConfig::Mock {
            id: "test-mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let source = create_source(config)
            .await
            .expect("Failed to create mock source");
        assert_eq!(source.id(), "test-mock-source");
        assert_eq!(source.type_name(), "mock");
    }

    #[tokio::test]
    async fn test_create_mock_source_auto_start_false() {
        let config = SourceConfig::Mock {
            id: "manual-source".to_string(),
            auto_start: false,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(500),
            },
        };

        let source = create_source(config)
            .await
            .expect("Failed to create mock source");
        assert_eq!(source.id(), "manual-source");
    }

    #[tokio::test]
    async fn test_create_mock_source_with_noop_bootstrap() {
        let config = SourceConfig::Mock {
            id: "bootstrap-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::Noop),
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let source = create_source(config)
            .await
            .expect("Failed to create source with bootstrap");
        assert_eq!(source.id(), "bootstrap-source");
    }

    #[tokio::test]
    async fn test_create_mock_source_with_scriptfile_bootstrap() {
        use crate::api::models::bootstrap::ScriptFileBootstrapConfigDto;

        let config = SourceConfig::Mock {
            id: "script-bootstrap-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::ScriptFile(
                ScriptFileBootstrapConfigDto {
                    file_paths: vec!["test.jsonl".to_string()],
                },
            )),
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let source = create_source(config)
            .await
            .expect("Failed to create source with scriptfile bootstrap");
        assert_eq!(source.id(), "script-bootstrap-source");
    }

    // ==========================================================================
    // Reaction Factory Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_create_log_reaction() {
        let config = ReactionConfig::Log {
            id: "test-log-reaction".to_string(),
            queries: vec!["query1".to_string()],
            auto_start: true,
            config: LogReactionConfigDto::default(),
        };

        let reaction = create_reaction(config, None).await.expect("Failed to create log reaction");
        assert_eq!(reaction.id(), "test-log-reaction");
        assert_eq!(reaction.type_name(), "log");
        assert_eq!(reaction.query_ids(), vec!["query1".to_string()]);
    }

    #[tokio::test]
    async fn test_create_log_reaction_multiple_queries() {
        let config = ReactionConfig::Log {
            id: "multi-query-reaction".to_string(),
            queries: vec![
                "query1".to_string(),
                "query2".to_string(),
                "query3".to_string(),
            ],
            auto_start: false,
            config: LogReactionConfigDto::default(),
        };

        let reaction = create_reaction(config, None).await.expect("Failed to create log reaction");
        assert_eq!(reaction.id(), "multi-query-reaction");
        assert_eq!(reaction.query_ids().len(), 3);
    }

    #[tokio::test]
    async fn test_create_profiler_reaction() {
        use crate::models::ProfilerReactionConfigDto;

        let config = ReactionConfig::Profiler {
            id: "profiler-reaction".to_string(),
            queries: vec!["perf-query".to_string()],
            auto_start: true,
            config: ProfilerReactionConfigDto {
                window_size: ConfigValue::Static(1000),
                report_interval_secs: ConfigValue::Static(60),
            },
        };

        let reaction = create_reaction(config, None).await.expect("Failed to create profiler reaction");
        assert_eq!(reaction.id(), "profiler-reaction");
        assert_eq!(reaction.type_name(), "profiler");
    }

    // ==========================================================================
    // State Store Provider Factory Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_create_redb_state_store_provider() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let path = temp_dir.path().join("state.redb");

        let config = StateStoreConfig::Redb {
            path: ConfigValue::Static(path.to_string_lossy().to_string()),
        };

        let provider = create_state_store_provider(config).expect("Failed to create REDB provider");
        // Provider is created successfully - we can't test much more without internal access
        assert!(std::sync::Arc::strong_count(&provider) >= 1);
    }

    #[tokio::test]
    async fn test_create_redb_state_store_provider_creates_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let path = temp_dir.path().join("test_store.redb");

        let config = StateStoreConfig::Redb {
            path: ConfigValue::Static(path.to_string_lossy().to_string()),
        };

        let _provider = create_state_store_provider(config).expect("Failed to create provider");

        // Verify the file was created
        assert!(path.exists(), "REDB file should be created");
    }

    // ==========================================================================
    // Bootstrap Provider Factory Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_create_noop_bootstrap_provider() {
        let bootstrap_config = BootstrapProviderConfig::Noop;
        let source_config = SourceConfig::Mock {
            id: "test".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let result = create_bootstrap_provider(&bootstrap_config, &source_config);
        assert!(result.is_ok(), "Failed to create noop bootstrap provider");
    }

    #[tokio::test]
    async fn test_create_scriptfile_bootstrap_provider() {
        use crate::api::models::bootstrap::ScriptFileBootstrapConfigDto;

        let bootstrap_config = BootstrapProviderConfig::ScriptFile(ScriptFileBootstrapConfigDto {
            file_paths: vec!["/path/to/data.jsonl".to_string()],
        });
        let source_config = SourceConfig::Mock {
            id: "test".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let provider = create_bootstrap_provider(&bootstrap_config, &source_config)
            .expect("Failed to create scriptfile bootstrap provider");

        // Provider was created successfully
        drop(provider);
    }

    #[tokio::test]
    async fn test_postgres_bootstrap_requires_postgres_source() {
        use crate::api::models::bootstrap::PostgresBootstrapConfigDto;

        let bootstrap_config =
            BootstrapProviderConfig::Postgres(PostgresBootstrapConfigDto::default());
        let source_config = SourceConfig::Mock {
            id: "test".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let result = create_bootstrap_provider(&bootstrap_config, &source_config);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("Postgres bootstrap provider can only be used with Postgres sources"),
            "Unexpected error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_application_bootstrap_returns_error() {
        use crate::api::models::bootstrap::ApplicationBootstrapConfigDto;

        let bootstrap_config =
            BootstrapProviderConfig::Application(ApplicationBootstrapConfigDto::default());
        let source_config = SourceConfig::Mock {
            id: "test".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(1000),
            },
        };

        let result = create_bootstrap_provider(&bootstrap_config, &source_config);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("Application bootstrap provider is managed internally"),
            "Unexpected error: {err_msg}"
        );
    }

    // ==========================================================================
    // Edge Case Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_source_with_custom_interval() {
        let config = SourceConfig::Mock {
            id: "custom-interval".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: ConfigValue::Static("generic".to_string()),
                interval_ms: ConfigValue::Static(60000), // 60 seconds
            },
        };

        let source = create_source(config)
            .await
            .expect("Failed to create source");
        assert_eq!(source.id(), "custom-interval");
    }

    #[tokio::test]
    async fn test_reaction_with_empty_queries_list() {
        let config = ReactionConfig::Log {
            id: "no-queries".to_string(),
            queries: vec![],
            auto_start: true,
            config: LogReactionConfigDto::default(),
        };

        let reaction = create_reaction(config, None).await.expect("Failed to create reaction");
        assert!(reaction.query_ids().is_empty());
    }
}
