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
use log::{info, error, warn};
use std::path::PathBuf;
use std::sync::Arc;
use std::fs::OpenOptions;
use axum::{
    extract::Extension,
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use drasi_server_core::{
    DrasiServerCore, 
    DrasiServerCoreConfig as ServerConfig,
    RuntimeConfig,
    config::ConfigPersistence
};
use crate::api;

pub struct DrasiServer {
    core: Option<DrasiServerCore>,
    enable_api: bool,
    api_host: String,
    api_port: u16,
    #[allow(dead_code)]
    enable_config_persistence: bool,
    config_file_path: Option<String>,
    read_only: Arc<bool>,
}

impl DrasiServer {
    /// Create a new DrasiServer from a configuration file
    pub async fn new(config_path: PathBuf, port: u16) -> Result<Self> {
        let config = ServerConfig::load_from_file(&config_path)?;
        config.validate()?;
        
        // Check if we have write access to the config file
        let read_only = !Self::check_write_access(&config_path);
        if read_only {
            warn!("Config file is not writable. Server running in READ-ONLY mode.");
            warn!("API modifications (create/update/delete) will be disabled.");
        } else if config.server.disable_persistence {
            info!("Config persistence is disabled. Changes will not be saved to the config file.");
            info!("API modifications are allowed but will not persist across restarts.");
        } else {
            info!("Config file is writable. Server running in normal mode.");
        }
        
        // Convert to RuntimeConfig
        let runtime_config = Arc::new(RuntimeConfig::from(config.clone()));
        
        // Create core server
        let core = DrasiServerCore::new(runtime_config);
        
        // Set up config persistence
        let config_persistence = Arc::new(ConfigPersistence::new(
            config_path.clone(),
            config.server.clone(),
            core.source_manager().clone(),
            core.query_manager().clone(),
            core.reaction_manager().clone(),
            read_only || config.server.disable_persistence,  // Don't persist if read-only OR disable_persistence is true
        ));
        
        core.set_config_persistence(config_persistence).await;
        
        Ok(Self {
            core: Some(core),
            enable_api: true,
            api_host: config.server.host,
            api_port: port,
            enable_config_persistence: true,
            config_file_path: Some(config_path.to_string_lossy().to_string()),
            read_only: Arc::new(read_only),
        })
    }
    
    /// Create a DrasiServer from a pre-built core (for use with builder)
    pub fn from_core(
        core: DrasiServerCore,
        enable_api: bool,
        api_host: String,
        api_port: u16,
        enable_config_persistence: bool,
        config_file_path: Option<String>,
    ) -> Self {
        Self {
            core: Some(core),
            enable_api,
            api_host,
            api_port,
            enable_config_persistence,
            config_file_path,
            read_only: Arc::new(false), // Programmatic mode assumes write access
        }
    }
    
    /// Check if we have write access to the config file
    fn check_write_access(path: &PathBuf) -> bool {
        // Try to open the file with write permissions
        OpenOptions::new()
            .append(true)
            .open(path)
            .is_ok()
    }
    
    pub async fn run(mut self) -> Result<()> {
        println!("Starting Drasi Server");
        if let Some(config_file) = &self.config_file_path {
            println!("  Config file: {}", config_file);
        }
        println!("  API Port: {}", self.api_port);
        println!("  Log level: {}", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
        info!("Initializing Drasi Server");
        
        // Take the core out of self
        let mut core = self.core.take().expect("Core should be initialized");
        
        // Initialize the core
        core.initialize().await?;
        
        // Convert to Arc for sharing
        let core = Arc::new(core);
        
        // Start the core server
        core.start().await?;
        
        // Start web API if enabled
        if self.enable_api {
            self.start_api(&core).await?;
            info!("Drasi Server started successfully with API on port {}", self.api_port);
        } else {
            info!("Drasi Server started successfully (API disabled)");
        }
        
        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        
        info!("Shutting down Drasi Server");
        core.stop().await?;
        
        Ok(())
    }
    
    async fn start_api(&self, core: &Arc<DrasiServerCore>) -> Result<()> {
        // Create OpenAPI documentation
        let openapi = api::ApiDoc::openapi();
        let app = Router::new()
            .route("/health", get(api::health_check))
            .route("/sources", get(api::list_sources))
            .route("/sources", post(api::create_source))
            .route("/sources/:id", get(api::get_source))
            .route("/sources/:id", axum::routing::put(api::update_source))
            .route("/sources/:id", axum::routing::delete(api::delete_source))
            .route("/sources/:id/start", post(api::start_source))
            .route("/sources/:id/stop", post(api::stop_source))
            .route("/queries", get(api::list_queries))
            .route("/queries", post(api::create_query))
            .route("/queries/:id", get(api::get_query))
            .route("/queries/:id", axum::routing::put(api::update_query))
            .route("/queries/:id", axum::routing::delete(api::delete_query))
            .route("/queries/:id/start", post(api::start_query))
            .route("/queries/:id/stop", post(api::stop_query))
            .route("/queries/:id/results", get(api::get_query_results))
            .route("/reactions", get(api::list_reactions))
            .route("/reactions", post(api::create_reaction))
            .route("/reactions/:id", get(api::get_reaction))
            .route("/reactions/:id", axum::routing::put(api::update_reaction))
            .route("/reactions/:id", axum::routing::delete(api::delete_reaction))
            .route("/reactions/:id/start", post(api::start_reaction))
            .route("/reactions/:id/stop", post(api::stop_reaction))
            .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi.clone()))
            .layer(CorsLayer::permissive())
            .layer(Extension(core.source_manager().clone()))
            .layer(Extension(core.query_manager().clone()))
            .layer(Extension(core.reaction_manager().clone()))
            .layer(Extension(core.data_router().clone()))
            .layer(Extension(core.subscription_router().clone()))
            .layer(Extension(core.bootstrap_router().clone()))
            .layer(Extension(self.read_only.clone()));
        
        let addr = format!("{}:{}", self.api_host, self.api_port);
        info!("Starting web API on {}", addr);
        info!("Swagger UI available at http://{}/docs/", addr);
        
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Web API server error: {}", e);
            }
        });
        
        Ok(())
    }
    
}