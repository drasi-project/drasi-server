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
use axum::{routing::get, Router};
use indexmap::IndexMap;
use log::{error, info, warn};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::api;
use crate::api::mappings::{map_server_settings, ConfigMapper, DtoMapper, QueryConfigMapper};
use crate::config::{
    DrasiLibInstanceConfig, DrasiServerConfig, ReactionConfig, ResolvedInstanceConfig, SourceConfig,
};
use crate::factories::{create_reaction, create_source, create_state_store_provider};
use crate::instance_registry::InstanceRegistry;
use crate::load_config_file;
use crate::persistence::ConfigPersistence;
use drasi_index_rocksdb::RocksDbIndexProvider;
use drasi_lib::DrasiLib;

pub struct DrasiServer {
    instances: Vec<PreparedInstance>,
    enable_api: bool,
    host: String,
    port: u16,
    config_file_path: Option<String>,
    read_only: Arc<bool>,
    #[allow(dead_code)]
    config_persistence: Option<Arc<ConfigPersistence>>,
}

struct PreparedInstance {
    id_hint: Option<String>,
    persist_index: bool,
    core: DrasiLib,
}

impl DrasiServer {
    /// Create a new DrasiServer from a configuration file
    pub async fn new(config_path: PathBuf, port: u16) -> Result<Self> {
        let config = load_config_file(&config_path)?;
        config.validate()?;

        // Resolve server settings using the mapper
        let mapper = DtoMapper::new();
        let resolved_settings = map_server_settings(&config, &mapper)?;
        let resolved_instances = config.resolved_instances(&mapper)?;

        // Determine persistence and read-only status
        // Read-only mode is ONLY enabled when the config file is not writable
        // persist_config: false means "don't save changes" but still allows API mutations
        let file_writable = Self::check_write_access(&config_path);
        let persistence_enabled = resolved_settings.persist_config;
        let read_only = !file_writable; // Only read-only if file is not writable

        if !file_writable {
            warn!("Config file is not writable. API in READ-ONLY mode.");
            warn!("Cannot create or delete components via API.");
        } else if !persistence_enabled {
            info!("Persistence disabled by configuration (persist_config: false).");
            warn!("API modifications will not persist across restarts.");
        } else {
            info!("Persistence ENABLED. API modifications will be saved to config file.");
        }

        let mut instances = Vec::new();

        for instance in resolved_instances {
            let mut builder = DrasiLib::builder().with_id(&instance.id);

            // Set capacity defaults if configured (resolve env vars)
            if let Some(capacity) = instance.default_priority_queue_capacity {
                builder = builder.with_priority_queue_capacity(capacity);
            }
            if let Some(capacity) = instance.default_dispatch_buffer_capacity {
                builder = builder.with_dispatch_buffer_capacity(capacity);
            }

            // Create and add RocksDB index provider if persist_index is enabled
            if instance.persist_index {
                let safe_id = instance.id.replace(['/', '\\'], "_").replace("..", "_");
                let index_path = PathBuf::from(format!("./data/{safe_id}/index"));
                info!(
                    "Enabling persistent indexing for instance '{}' with RocksDB at: {}",
                    instance.id,
                    index_path.display()
                );
                let rocksdb_provider = RocksDbIndexProvider::new(
                    index_path, true,  // enable_archive - support for past() function
                    false, // direct_io - use OS page cache
                );
                builder = builder.with_index_provider(Arc::new(rocksdb_provider));
            }

            // Create and add state store provider if configured
            if let Some(state_store_config) = instance.state_store.clone() {
                info!(
                    "Enabling persistent state store for instance '{}' with {} provider",
                    instance.id,
                    state_store_config.kind()
                );
                let state_store_provider = create_state_store_provider(state_store_config)?;
                builder = builder.with_state_store_provider(state_store_provider);
            }

            // Create and add sources from config
            info!(
                "Loading {} source(s) from configuration for instance '{}'",
                instance.sources.len(),
                instance.id
            );
            for source_config in instance.sources.clone() {
                let source = create_source(source_config).await?;
                builder = builder.with_source(source);
            }

            // Add queries from config (already resolved in config/types.rs)
            for query_config in &instance.queries {
                builder = builder.with_query(query_config.clone());
            }

            // Create and add reactions from config
            for reaction_config in instance.reactions.clone() {
                let reaction = create_reaction(reaction_config)?;
                builder = builder.with_reaction(reaction);
            }

            // Build and initialize the core
            let core = builder
                .build()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create DrasiLib: {e}"))?;

            instances.push(PreparedInstance {
                id_hint: Some(instance.id),
                persist_index: instance.persist_index,
                core,
            });
        }

        Ok(Self {
            instances,
            enable_api: true,
            host: resolved_settings.host,
            port,
            config_file_path: Some(config_path.to_string_lossy().to_string()),
            read_only: Arc::new(read_only),
            config_persistence: None, // Will be set after core is started
        })
    }

    /// Create a DrasiServer from a pre-built core (for use with builder)
    pub fn from_core(
        core: DrasiLib,
        enable_api: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
    ) -> Self {
        Self {
            instances: vec![PreparedInstance {
                id_hint: None,
                persist_index: false,
                core,
            }],
            enable_api,
            host,
            port,
            config_file_path,
            read_only: Arc::new(false), // Programmatic mode assumes write access
            config_persistence: None,   // Will be set up if config file is provided
        }
    }

    /// Create a DrasiServer from multiple pre-built cores (for builder multi-instance usage)
    pub fn from_cores(
        cores: Vec<(DrasiLib, Option<String>, bool)>,
        enable_api: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
    ) -> Self {
        let instances = cores
            .into_iter()
            .map(|(core, id_hint, persist_index)| PreparedInstance {
                id_hint,
                persist_index,
                core,
            })
            .collect();

        Self {
            instances,
            enable_api,
            host,
            port,
            config_file_path,
            read_only: Arc::new(false),
            config_persistence: None,
        }
    }

    /// Check if we have write access to the config file
    fn check_write_access(path: &PathBuf) -> bool {
        // Try to open the file with write permissions
        OpenOptions::new().append(true).open(path).is_ok()
    }

    #[allow(clippy::print_stdout)]
    pub async fn run(mut self) -> Result<()> {
        println!("Starting Drasi Server");
        if let Some(config_file) = &self.config_file_path {
            println!("  Config file: {config_file}");
        }
        println!("  API Port: {}", self.port);
        println!(
            "  Log level: {}",
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
        );
        info!("Initializing Drasi Server");

        let mut instance_map: IndexMap<String, Arc<DrasiLib>> = IndexMap::new();
        let mut persist_settings: IndexMap<String, bool> = IndexMap::new();

        // Take ownership of instances to avoid partial move of self
        let instances = std::mem::take(&mut self.instances);
        for instance in instances {
            let mut core = instance.core;
            let id = match instance.id_hint {
                Some(id) => id,
                None => core
                    .get_current_config()
                    .await
                    .map(|c| c.id.clone())
                    .map_err(|e| anyhow::anyhow!("Failed to resolve DrasiLib id: {e}"))?,
            };

            let core = Arc::new(core);
            core.start().await?;
            persist_settings.insert(id.clone(), instance.persist_index);
            instance_map.insert(id, core);
        }

        if instance_map.is_empty() {
            return Err(anyhow::anyhow!(
                "No DrasiLib instances configured for this server"
            ));
        }

        // Wrap the instance map in Arc for sharing
        let instances = Arc::new(instance_map);

        // Create the instance registry from the map
        let registry = InstanceRegistry::from_map((*instances).clone());

        // Initialize persistence if config file is provided and persistence is enabled
        let config_persistence = if let Some(config_file) = &self.config_file_path {
            if !*self.read_only {
                // Need to reload config to check persist_config flag and get initial configs
                let config = load_config_file(PathBuf::from(config_file))?;
                let mapper = DtoMapper::new();
                let resolved_settings = map_server_settings(&config, &mapper)?;
                let persistence_enabled = resolved_settings.persist_config;

                if persistence_enabled {
                    // Extract source, reaction, and query configs from the loaded config
                    let resolved_instances = config.resolved_instances(&mapper)?;
                    let (initial_source_configs, initial_reaction_configs, initial_query_configs) =
                        Self::extract_component_configs(&config, &resolved_instances)?;

                    // Persistence is enabled - create ConfigPersistence instance
                    let persistence = Arc::new(ConfigPersistence::new(
                        PathBuf::from(config_file),
                        registry.clone(),
                        self.host.clone(),
                        self.port,
                        resolved_settings.log_level,
                        true, // persist_config = true
                        persist_settings.clone(),
                        initial_source_configs,
                        initial_reaction_configs,
                        initial_query_configs,
                    ));
                    info!("Configuration persistence enabled");
                    Some(persistence)
                } else {
                    info!("Configuration persistence disabled (persist_config: false)");
                    None
                }
            } else {
                info!("Configuration persistence disabled (read-only mode)");
                None
            }
        } else {
            info!("No config file provided - persistence disabled");
            None
        };

        // Start web API if enabled
        if self.enable_api {
            self.start_api(
                instances.clone(),
                registry.clone(),
                config_persistence.clone(),
            )
            .await?;
            info!(
                "Drasi Server started successfully with API on port {}",
                self.port
            );
        } else {
            info!("Drasi Server started successfully (API disabled)");
        }

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;

        info!("Shutting down Drasi Server");
        for (_id, core) in registry.list().await {
            core.stop().await?;
        }

        Ok(())
    }

    async fn start_api(
        &self,
        instances: Arc<IndexMap<String, Arc<DrasiLib>>>,
        registry: InstanceRegistry,
        config_persistence: Option<Arc<ConfigPersistence>>,
    ) -> Result<()> {
        // Create OpenAPI documentation for v1
        let openapi_v1 = api::ApiDocV1::openapi();

        // Build the v1 API router
        let v1_router =
            api::build_v1_router(registry, self.read_only.clone(), config_persistence.clone());

        // Build the main application router
        let app = Router::new()
            // Health check at root level (operational endpoint, not versioned)
            .route("/health", get(api::health_check))
            // API versions endpoint
            .route("/api/versions", get(api::list_api_versions))
            // Nest v1 API under /api/v1
            .nest("/api/v1", v1_router)
            // Swagger UI and OpenAPI spec for v1
            .merge(SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", openapi_v1.clone()))
            .layer(CorsLayer::permissive());

        let addr = format!("{}:{}", self.host, self.port);
        info!("Starting web API on {addr}");
        info!("API v1 available at http://{addr}/api/v1/");
        info!("Swagger UI available at http://{addr}/api/v1/docs/");

        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Web API server error: {e}");
            }
        });

        Ok(())
    }

    /// Extract source and reaction configs from resolved instances for persistence initialization
    fn extract_component_configs(
        config: &DrasiServerConfig,
        resolved_instances: &[ResolvedInstanceConfig],
    ) -> Result<(
        IndexMap<String, IndexMap<String, SourceConfig>>,
        IndexMap<String, IndexMap<String, ReactionConfig>>,
        IndexMap<String, IndexMap<String, crate::api::models::QueryConfigDto>>,
    )> {
        use crate::api::models::QueryConfigDto;

        let mut source_configs: IndexMap<String, IndexMap<String, SourceConfig>> = IndexMap::new();
        let mut reaction_configs: IndexMap<String, IndexMap<String, ReactionConfig>> =
            IndexMap::new();
        let mut query_configs: IndexMap<String, IndexMap<String, QueryConfigDto>> = IndexMap::new();

        // Get the raw instances (before resolution) to extract QueryConfigDto
        let raw_instances: Vec<&DrasiLibInstanceConfig> = if config.instances.is_empty() {
            // Single instance mode - create a temporary reference
            vec![]
        } else {
            config.instances.iter().collect()
        };

        for (i, instance) in resolved_instances.iter().enumerate() {
            let mut sources = IndexMap::new();
            for source in &instance.sources {
                sources.insert(source.id().to_string(), source.clone());
            }
            source_configs.insert(instance.id.clone(), sources);

            let mut reactions = IndexMap::new();
            for reaction in &instance.reactions {
                reactions.insert(reaction.id().to_string(), reaction.clone());
            }
            reaction_configs.insert(instance.id.clone(), reactions);

            // Extract query configs from the original DTOs
            let query_dtos: &Vec<QueryConfigDto> = if config.instances.is_empty() {
                // Single instance mode - use root-level queries
                &config.queries
            } else {
                // Multi-instance mode - use the corresponding instance's queries
                &raw_instances[i].queries
            };

            let mut queries = IndexMap::new();
            for dto in query_dtos {
                queries.insert(dto.id.clone(), dto.clone());
            }
            query_configs.insert(instance.id.clone(), queries);
        }

        Ok((source_configs, reaction_configs, query_configs))
    }
}
