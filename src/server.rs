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
use axum::{
    extract::Extension,
    routing::{get, post},
    Router,
};
use log::{error, info, warn};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::api;
use crate::api::mappings::{map_server_settings, DtoMapper};
use crate::factories::{create_reaction, create_source};
use crate::load_config_file;
use crate::persistence::ConfigPersistence;
use drasi_index_rocksdb::RocksDbIndexProvider;
use drasi_lib::DrasiLib;

pub struct DrasiServer {
    core: Option<DrasiLib>,
    enable_api: bool,
    host: String,
    port: u16,
    config_file_path: Option<String>,
    read_only: Arc<bool>,
    #[allow(dead_code)]
    config_persistence: Option<Arc<ConfigPersistence>>,
}

impl DrasiServer {
    /// Create a new DrasiServer from a configuration file
    pub async fn new(config_path: PathBuf, port: u16) -> Result<Self> {
        let config = load_config_file(&config_path)?;
        config.validate()?;

        // Resolve server settings using the mapper
        let mapper = DtoMapper::new();
        let resolved_settings = map_server_settings(&config, &mapper)?;

        // Determine persistence and read-only status
        // Read-only mode is ONLY enabled when the config file is not writable
        // disable_persistence just means "don't save changes" but still allows API mutations
        let file_writable = Self::check_write_access(&config_path);
        let persistence_disabled = resolved_settings.disable_persistence;
        let _persistence_enabled = file_writable && !persistence_disabled;
        let read_only = !file_writable; // Only read-only if file is not writable

        if !file_writable {
            warn!("Config file is not writable. API in READ-ONLY mode.");
            warn!("Cannot create or delete components via API.");
        } else if persistence_disabled {
            info!("Persistence disabled by configuration (disable_persistence: true).");
            warn!("API modifications will not persist across restarts.");
        } else {
            info!("Persistence ENABLED. API modifications will be saved to config file.");
        }

        // Build DrasiLib using the builder pattern with factory-created components
        // Resolve the id from ConfigValue (supports env vars)
        let id: String = mapper.resolve_typed(&config.id)?;
        let mut builder = DrasiLib::builder().with_id(&id);

        // Set capacity defaults if configured (resolve env vars)
        if let Some(ref capacity_config) = config.default_priority_queue_capacity {
            let capacity: usize = mapper.resolve_typed(capacity_config)?;
            builder = builder.with_priority_queue_capacity(capacity);
        }
        if let Some(ref capacity_config) = config.default_dispatch_buffer_capacity {
            let capacity: usize = mapper.resolve_typed(capacity_config)?;
            builder = builder.with_dispatch_buffer_capacity(capacity);
        }

        // Create and add RocksDB index provider if persist_index is enabled
        if config.persist_index {
            let index_path = PathBuf::from("./data/index");
            info!(
                "Enabling persistent indexing with RocksDB at: {}",
                index_path.display()
            );
            let rocksdb_provider = RocksDbIndexProvider::new(
                index_path, true,  // enable_archive - support for past() function
                false, // direct_io - use OS page cache
            );
            builder = builder.with_index_provider(Arc::new(rocksdb_provider));
        }

        // Create and add sources from config
        info!(
            "Loading {} source(s) from configuration",
            config.sources.len()
        );
        for source_config in config.sources.clone() {
            let source = create_source(source_config).await?;
            builder = builder.with_source(source);
        }

        // Add queries from config
        for query_config in &config.queries {
            builder = builder.with_query(query_config.clone());
        }

        // Create and add reactions from config
        for reaction_config in config.reactions.clone() {
            let reaction = create_reaction(reaction_config)?;
            builder = builder.with_reaction(reaction);
        }

        // Build and initialize the core
        let core = builder
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create DrasiLib: {e}"))?;

        Ok(Self {
            core: Some(core),
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
            core: Some(core),
            enable_api,
            host,
            port,
            config_file_path,
            read_only: Arc::new(false), // Programmatic mode assumes write access
            config_persistence: None,   // Will be set up if config file is provided
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

        // Take the core out of self
        let core = self.core.take().expect("Core should be initialized");

        // Core is already initialized by from_config_file or builder
        // Convert to Arc for sharing
        let core = Arc::new(core);

        // Start the core server
        core.start().await?;

        // Initialize persistence if config file is provided and persistence is enabled
        let config_persistence = if let Some(config_file) = &self.config_file_path {
            if !*self.read_only {
                // Need to reload config to check disable_persistence flag
                let config = load_config_file(PathBuf::from(config_file))?;
                let mapper = DtoMapper::new();
                let resolved_settings = map_server_settings(&config, &mapper)?;
                let persistence_disabled = resolved_settings.disable_persistence;

                if !persistence_disabled {
                    // Persistence is enabled - create ConfigPersistence instance
                    let persistence = Arc::new(ConfigPersistence::new(
                        PathBuf::from(config_file),
                        core.clone(),
                        self.host.clone(),
                        self.port,
                        resolved_settings.log_level,
                        false,
                        config.persist_index,
                    ));
                    info!("Configuration persistence enabled");
                    Some(persistence)
                } else {
                    info!("Configuration persistence disabled (disable_persistence: true)");
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
            self.start_api(&core, config_persistence.clone()).await?;
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
        core.stop().await?;

        Ok(())
    }

    async fn start_api(
        &self,
        core: &Arc<DrasiLib>,
        config_persistence: Option<Arc<ConfigPersistence>>,
    ) -> Result<()> {
        // Create OpenAPI documentation
        let openapi = api::ApiDoc::openapi();
        let app = Router::new()
            .route("/health", get(api::health_check))
            .route("/sources", get(api::list_sources))
            .route("/sources", post(api::create_source_handler))
            .route("/sources/:id", get(api::get_source))
            .route("/sources/:id", axum::routing::delete(api::delete_source))
            .route("/sources/:id/start", post(api::start_source))
            .route("/sources/:id/stop", post(api::stop_source))
            .route("/queries", get(api::list_queries))
            .route("/queries", post(api::create_query))
            .route("/queries/:id", get(api::get_query))
            .route("/queries/:id", axum::routing::delete(api::delete_query))
            .route("/queries/:id/start", post(api::start_query))
            .route("/queries/:id/stop", post(api::stop_query))
            .route("/queries/:id/results", get(api::get_query_results))
            .route("/reactions", get(api::list_reactions))
            .route("/reactions", post(api::create_reaction_handler))
            .route("/reactions/:id", get(api::get_reaction))
            .route(
                "/reactions/:id",
                axum::routing::delete(api::delete_reaction),
            )
            .route("/reactions/:id/start", post(api::start_reaction))
            .route("/reactions/:id/stop", post(api::stop_reaction))
            .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi.clone()))
            .layer(CorsLayer::permissive())
            // Inject DrasiLib for handlers to use
            .layer(Extension(core.clone()))
            .layer(Extension(self.read_only.clone()))
            .layer(Extension(config_persistence));

        let addr = format!("{}:{}", self.host, self.port);
        info!("Starting web API on {addr}");
        info!("Swagger UI available at http://{addr}/docs/");

        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Web API server error: {e}");
            }
        });

        Ok(())
    }
}
