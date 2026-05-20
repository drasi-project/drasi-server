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
use log::{debug, error, info, warn};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::api;
use crate::api::mappings::{map_server_settings, DtoMapper};
use crate::factories::{create_reaction_locked, create_source_locked, create_state_store_provider};
use crate::instance_registry::InstanceRegistry;
use crate::load_config_file;
use crate::persistence::ConfigPersistence;
use crate::plugin_orchestrator::PluginOrchestrator;
use crate::plugin_registry::PluginRegistry;
use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_index_rocksdb::RocksDbIndexProvider;
use drasi_lib::DrasiLib;
use drasi_plugin_sdk::{BootstrapPluginDescriptor, ReactionPluginDescriptor};
use drasi_wal_redb::RedbWalProvider;

pub struct DrasiServer {
    instances: Vec<PreparedInstance>,
    enable_api: bool,
    enable_ui: bool,
    host: String,
    port: u16,
    config_file_path: Option<String>,
    read_only: Arc<bool>,
    plugin_registry: Arc<RwLock<PluginRegistry>>,
    plugin_orchestrator: Arc<PluginOrchestrator>,
    cors_allowed_origins: Vec<String>,
    watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

struct PreparedInstance {
    id_hint: Option<String>,
    persist_index: bool,
    core: DrasiLib,
}

impl DrasiServer {
    /// Create a new DrasiServer from a configuration file
    pub async fn new(
        config_path: PathBuf,
        port: u16,
        plugins_dir: PathBuf,
        skip_verification: bool,
        enable_ui: bool,
    ) -> Result<Self> {
        let mut config = load_config_file(&config_path)?;
        config.validate()?;

        // CLI --skip-verification flag overrides config (disables when set)
        if skip_verification {
            config.verify_plugins = false;
        }

        // Create and populate the plugin registry
        let mut plugin_registry = PluginRegistry::new();
        register_core_plugins(&mut plugin_registry);

        // Auto-install plugins from registry if configured
        if config.auto_install_plugins && !config.plugins.is_empty() {
            crate::plugin_install::auto_install_plugins(&config, &plugins_dir, false).await?;
        }

        // When verify_plugins is enabled, re-verify plugin signatures against the
        // OCI registry (not the lockfile cache, which could be tampered with).
        // Verifications run in parallel for speed.
        let mut verified_files = if config.verify_plugins {
            use drasi_host_sdk::registry::{
                matches_trusted_identity, CosignVerifier, RegistryAuth, SignatureStatus,
                TrustedIdentity, VerificationConfig,
            };

            let lockfile = crate::plugin_lockfile::PluginLockfile::read(&plugins_dir)
                .ok()
                .flatten()
                .unwrap_or_default();

            if lockfile.is_empty() {
                warn!("verify_plugins enabled but no lockfile found — no plugins will be loaded");
                Some(std::collections::HashSet::new())
            } else {
                // Build the list of trusted identities from config,
                // defaulting to drasi-project if none configured.
                let trusted: Vec<TrustedIdentity> = if config.trusted_identities.is_empty() {
                    vec![TrustedIdentity {
                        issuer: "https://token.actions.githubusercontent.com".to_string(),
                        subject_pattern: "https://github.com/drasi-project/*".to_string(),
                    }]
                } else {
                    config
                        .trusted_identities
                        .iter()
                        .map(|ti| TrustedIdentity {
                            issuer: ti.issuer.clone(),
                            subject_pattern: ti.subject_pattern.clone(),
                        })
                        .collect()
                };

                let verifier = CosignVerifier::new(VerificationConfig {
                    enabled: true,
                    ..Default::default()
                });

                // Convert registry auth for the verifier
                let host_auth = crate::plugin_operations::PluginOperations::registry_auth();
                let oci_auth = match &host_auth {
                    RegistryAuth::Anonymous => oci_client::secrets::RegistryAuth::Anonymous,
                    RegistryAuth::Basic { username, password } => {
                        oci_client::secrets::RegistryAuth::Basic(username.clone(), password.clone())
                    }
                };

                // Build batch: (oci_reference, filename) from lockfile
                let batch: Vec<(String, String)> = lockfile
                    .plugins
                    .values()
                    .map(|p| (p.reference.clone(), p.filename.clone()))
                    .collect();

                // Verify all plugins in parallel against the registry
                let results = verifier.verify_batch(batch, &oci_auth).await;

                let allowed: std::collections::HashSet<String> = results
                    .into_iter()
                    .filter_map(|(filename, status)| match status {
                        SignatureStatus::Verified(v)
                            if matches_trusted_identity(&v, &trusted) =>
                        {
                            info!(
                                "✓ {filename} — trusted (issuer={}, subject={})",
                                v.issuer, v.subject
                            );
                            Some(filename)
                        }
                        SignatureStatus::Verified(v) => {
                            warn!(
                                "✗ {filename} — signed but not from a trusted identity (issuer={}, subject={})",
                                v.issuer, v.subject
                            );
                            None
                        }
                        SignatureStatus::Tampered(reason) => {
                            log::error!(
                                "⚠ {filename} — TAMPERED: {reason}"
                            );
                            None
                        }
                        SignatureStatus::Unsigned => None,
                    })
                    .collect();

                Some(allowed)
            }
        } else {
            None
        };

        // Verify file integrity against lockfile hashes
        if plugins_dir.exists() {
            let lockfile = crate::plugin_lockfile::PluginLockfile::read(&plugins_dir)
                .ok()
                .flatten()
                .unwrap_or_default();

            if !lockfile.is_empty() {
                use crate::plugin_lockfile::FileIntegrityStatus;
                let integrity = lockfile.verify_file_integrity(&plugins_dir);
                for (filename, status) in &integrity {
                    match status {
                        FileIntegrityStatus::Ok => {
                            debug!("✓ {filename} — integrity verified");
                        }
                        FileIntegrityStatus::Tampered { expected, actual } => {
                            log::error!(
                                "⚠ {filename} — TAMPERED: expected hash {expected}, got {actual}"
                            );
                            // Remove from allowed files if signature verification is enabled
                            if let Some(ref mut allowed) = verified_files {
                                allowed.remove(filename);
                            }
                        }
                        FileIntegrityStatus::NoHash => {
                            debug!("{filename} — no hash in lockfile (legacy entry)");
                        }
                        FileIntegrityStatus::Missing => {
                            debug!("{filename} — file not on disk");
                        }
                        FileIntegrityStatus::Error(e) => {
                            warn!("{filename} — integrity check error: {e}");
                        }
                    }
                }
            }
        }

        // Load dynamic plugins from the plugins directory
        let mut startup_plugin_records = Vec::new();
        if plugins_dir.exists() {
            let callback_ctx = Arc::new(drasi_host_sdk::CallbackContext {
                instance_id: String::new(),
                runtime_handle: tokio::runtime::Handle::current(),
                log_registry: drasi_lib::managers::get_or_init_global_registry(),
                source_event_history: Arc::new(tokio::sync::RwLock::new(
                    drasi_lib::managers::ComponentEventHistory::new(),
                )),
                reaction_event_history: Arc::new(tokio::sync::RwLock::new(
                    drasi_lib::managers::ComponentEventHistory::new(),
                )),
            });
            let load_stats = crate::dynamic_loading::load_plugins(
                &plugins_dir,
                &mut plugin_registry,
                Some(callback_ctx),
                verified_files.as_ref(),
            )?;
            startup_plugin_records = load_stats.loaded_plugins;
        }

        let plugin_registry = Arc::new(RwLock::new(plugin_registry));

        // Create the plugin lifecycle and orchestration layers
        let lifecycle = Arc::new(PluginLifecycleManager::new(plugin_registry.clone()));
        let verification_config =
            crate::plugin_operations::PluginOperations::verification_config(&config);
        let plugin_ops =
            crate::plugin_operations::PluginOperations::from_config(&config, plugins_dir.clone());
        let plugin_orchestrator = Arc::new(PluginOrchestrator::with_ops(
            lifecycle,
            plugins_dir.clone(),
            plugin_ops,
            verification_config,
        ));

        // Register startup-loaded plugins in the orchestrator
        plugin_orchestrator
            .record_startup_plugins(&startup_plugin_records)
            .await;

        // Start plugin hot-reload watcher if configured
        let watcher_handle = if config.hot_reload_plugins {
            use drasi_host_sdk::watcher::{PluginWatcher, PluginWatcherConfig};

            let watcher_config = PluginWatcherConfig {
                plugins_dir: plugins_dir.clone(),
                debounce: std::time::Duration::from_millis(config.hot_reload_debounce_ms),
            };
            let mut watcher = PluginWatcher::new(watcher_config);
            let mut rx = watcher.subscribe();
            let orchestrator_for_watcher = plugin_orchestrator.clone();

            // Start the filesystem watcher
            if let Err(e) = watcher.start() {
                warn!("Failed to start notify-based plugin watcher: {e}. Falling back to polling.");
                if let Err(e) = watcher.start_polling() {
                    warn!("Failed to start polling plugin watcher: {e}");
                }
            }

            // Spawn a task that receives file events and applies the configured policy
            let handle = tokio::spawn(async move {
                // Keep the watcher alive for the duration of this task
                let _watcher = watcher;
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            use drasi_host_sdk::plugin_types::PluginFileEvent;
                            match event {
                                PluginFileEvent::Added(path) | PluginFileEvent::Changed(path) => {
                                    info!("Plugin file change detected: {}", path.display());
                                    match orchestrator_for_watcher
                                        .load_plugin_locked(&path, None)
                                        .await
                                    {
                                        Ok(info) => info!(
                                            "Hot-reloaded plugin: {} ({})",
                                            info.id, info.status
                                        ),
                                        Err(e) => warn!(
                                            "Failed to hot-reload plugin {}: {e}",
                                            path.display()
                                        ),
                                    }
                                }
                                PluginFileEvent::Removed(path) => {
                                    info!("Plugin file removed: {}", path.display());
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Plugin watcher lagged, missed {n} events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            });
            info!(
                "Plugin hot-reload enabled (debounce: {}ms)",
                config.hot_reload_debounce_ms
            );
            Some(handle)
        } else {
            None
        };

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

        // Check upfront that all required plugins are available before creating
        // any components. This reports ALL missing plugins at once rather than
        // failing one at a time during source/reaction creation.
        {
            use crate::config::plugin_validation::{
                check_plugin_availability, extract_plugin_requirements,
            };
            let requirements = extract_plugin_requirements(&config);
            let reg = plugin_registry.read().await;
            let (_found, missing) = check_plugin_availability(&requirements, &reg);
            if !missing.is_empty() {
                let mut msg = String::from("Missing plugins required by configuration:\n");
                for mp in &missing {
                    msg.push_str(&format!(
                        "  - {}/{} (referenced by {})\n",
                        mp.requirement.category, mp.requirement.kind, mp.requirement.referenced_by
                    ));
                    if !mp.available_kinds.is_empty() {
                        msg.push_str(&format!(
                            "    available {} kinds: {}\n",
                            mp.requirement.category,
                            mp.available_kinds.join(", ")
                        ));
                    }
                }
                msg.push_str(
                    "Install the missing plugins or correct the 'kind' values in your config.",
                );
                return Err(anyhow::anyhow!("{msg}"));
            }
        }

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

            // Create WAL provider for durable source event persistence
            {
                let safe_id = instance.id.replace(['/', '\\'], "_").replace("..", "_");
                let wal_path = PathBuf::from(format!("./data/{safe_id}/wal"));
                info!(
                    "Enabling WAL provider for instance '{}' at: {}",
                    instance.id,
                    wal_path.display()
                );
                let wal_provider = Arc::new(RedbWalProvider::new(&wal_path));
                builder = builder.with_wal_provider(wal_provider);
            }

            // Create and add sources from config
            info!(
                "Loading {} source(s) from configuration for instance '{}'",
                instance.sources.len(),
                instance.id
            );
            for source_config in instance.sources.clone() {
                let (source, plugin_meta) =
                    create_source_locked(&plugin_registry, source_config).await?;
                builder = builder.with_source_metadata(source, plugin_meta);
            }

            // Add queries from config (already resolved in config/types.rs)
            for query_config in &instance.queries {
                builder = builder.with_query(query_config.clone());
            }

            // Create and add reactions from config
            for reaction_config in instance.reactions.clone() {
                let (reaction, plugin_meta) =
                    create_reaction_locked(&plugin_registry, reaction_config).await?;
                builder = builder.with_reaction_metadata(reaction, plugin_meta);
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
            enable_ui,
            host: resolved_settings.host,
            port,
            config_file_path: Some(config_path.to_string_lossy().to_string()),
            read_only: Arc::new(read_only),
            plugin_registry,
            plugin_orchestrator,
            cors_allowed_origins: config.cors_allowed_origins.clone(),
            watcher_handle,
        })
    }

    /// Create a DrasiServer from a pre-built core (for use with builder)
    pub fn from_core(
        core: DrasiLib,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
    ) -> Self {
        let mut plugin_registry = PluginRegistry::new();
        register_core_plugins(&mut plugin_registry);
        let plugin_registry = Arc::new(RwLock::new(plugin_registry));
        let lifecycle = Arc::new(PluginLifecycleManager::new(plugin_registry.clone()));
        let plugin_orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));
        Self {
            instances: vec![PreparedInstance {
                id_hint: None,
                persist_index: false,
                core,
            }],
            enable_api,
            enable_ui,
            host,
            port,
            config_file_path,
            read_only: Arc::new(false), // Programmatic mode assumes write access
            plugin_registry,
            plugin_orchestrator,
            cors_allowed_origins: Vec::new(), // Permissive by default for programmatic usage
            watcher_handle: None,
        }
    }

    /// Create a DrasiServer from multiple pre-built cores (for builder multi-instance usage)
    pub fn from_cores(
        cores: Vec<(DrasiLib, Option<String>, bool)>,
        enable_api: bool,
        enable_ui: bool,
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

        let mut plugin_registry = PluginRegistry::new();
        register_core_plugins(&mut plugin_registry);
        let plugin_registry = Arc::new(RwLock::new(plugin_registry));
        let lifecycle = Arc::new(PluginLifecycleManager::new(plugin_registry.clone()));
        let plugin_orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));
        Self {
            instances,
            enable_api,
            enable_ui,
            host,
            port,
            config_file_path,
            read_only: Arc::new(false),
            plugin_registry,
            plugin_orchestrator,
            cors_allowed_origins: Vec::new(), // Permissive by default for programmatic usage
            watcher_handle: None,
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
        println!("  Version: {}", env!("CARGO_PKG_VERSION"));
        println!("  Rust: {}", env!("DRASI_RUSTC_VERSION"));
        println!("  Plugin SDK: {}", env!("DRASI_PLUGIN_SDK_VERSION"));
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
            let core = instance.core;
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

        // Initialize persistence and extract solutions_dir if config file is provided
        let (config_persistence, solutions_dir) = if let Some(config_file) = &self.config_file_path
        {
            if !*self.read_only {
                // Need to reload config to check persist_config flag and get initial configs
                let config = load_config_file(PathBuf::from(config_file))?;
                let solutions_dir = config.solutions_dir.clone();
                let mapper = DtoMapper::new();
                let resolved_settings = map_server_settings(&config, &mapper)?;
                let persistence_enabled = resolved_settings.persist_config;

                if persistence_enabled {
                    // Persistence is enabled - create ConfigPersistence instance
                    let persistence = Arc::new(ConfigPersistence::new(
                        PathBuf::from(config_file),
                        registry.clone(),
                        self.host.clone(),
                        self.port,
                        resolved_settings.log_level,
                        true, // persist_config = true
                        persist_settings.clone(),
                        config.solutions_dir.clone(),
                        &config,
                    ));
                    info!("Configuration persistence enabled");
                    (Some(persistence), solutions_dir)
                } else {
                    info!("Configuration persistence disabled (persist_config: false)");
                    (None, solutions_dir)
                }
            } else {
                info!("Configuration persistence disabled (read-only mode)");
                (None, None)
            }
        } else {
            info!("No config file provided - persistence disabled");
            (None, None)
        };

        // Start web API if enabled
        if self.enable_api {
            self.start_api(
                instances.clone(),
                registry.clone(),
                config_persistence.clone(),
                solutions_dir,
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

        // Cancel the hot-reload watcher task if running
        if let Some(handle) = self.watcher_handle.take() {
            handle.abort();
            let _ = handle.await;
            info!("Plugin hot-reload watcher stopped");
        }

        for (_id, core) in registry.list().await {
            core.stop().await?;
        }

        Ok(())
    }

    async fn start_api(
        &self,
        _instances: Arc<IndexMap<String, Arc<DrasiLib>>>,
        registry: InstanceRegistry,
        config_persistence: Option<Arc<ConfigPersistence>>,
        solutions_dir: Option<String>,
    ) -> Result<()> {
        // Create OpenAPI documentation for v1 with cache
        let mut openapi_v1 = api::ApiDocV1::openapi();
        let registry_version = {
            let reg = self.plugin_registry.read().await;
            api::inject_plugin_schemas(&mut openapi_v1, &reg);
            reg.version()
        };

        // Keep the OpenAPI cache alive for future hot-reload support.
        // Currently the spec is generated once at startup; the cache will
        // auto-regenerate when the plugin registry version changes.
        let _openapi_cache = Arc::new(api::v1::OpenApiCache::new(
            openapi_v1.clone(),
            self.plugin_registry.clone(),
            registry_version,
        ));

        // Build the v1 API router
        let v1_router = api::build_v1_router(
            registry.clone(),
            self.read_only.clone(),
            config_persistence.clone(),
            self.plugin_registry.clone(),
            solutions_dir,
        );

        // Build the plugin management sub-router
        let plugin_router = api::v1::plugin_routes()
            .layer(axum::extract::Extension(self.plugin_orchestrator.clone()))
            .layer(axum::extract::Extension(registry.clone()));

        // Build the main application router
        let mut app = Router::new()
            // Health check at root level (operational endpoint, not versioned)
            .route("/health", get(api::health_check))
            // API versions endpoint
            .route("/api/versions", get(api::list_api_versions))
            // Nest v1 API under /api/v1
            .nest("/api/v1", v1_router)
            // Nest plugin management API under /api/v1/plugins
            .nest("/api/v1/plugins", plugin_router)
            // Swagger UI and OpenAPI spec for v1
            .merge(SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", openapi_v1.clone()));

        // Serve the Drasi Server Admin UI if enabled
        let ui_dir = std::path::Path::new("ui/dist");
        let has_filesystem_ui = ui_dir.join("index.html").exists();
        let has_embedded_ui = crate::ui_assets::has_embedded_ui();

        if self.enable_ui {
            if has_filesystem_ui {
                // Prefer filesystem for development (hot-reload)
                info!("Drasi Server Admin UI found on filesystem, serving at /ui/");
                app = app
                    .nest_service(
                        "/ui",
                        ServeDir::new(ui_dir).append_index_html_on_directories(true),
                    )
                    .route(
                        "/",
                        get(|| async { axum::response::Redirect::temporary("/ui/") }),
                    );
            } else if has_embedded_ui {
                // Fall back to embedded assets (release binary)
                info!("Drasi Server Admin UI (embedded), serving at /ui/");
                app = app.merge(crate::ui_assets::embedded_ui_routes()).route(
                    "/",
                    get(|| async { axum::response::Redirect::temporary("/ui/") }),
                );
            } else {
                warn!(
                    "Web UI is enabled but no UI assets found. \
                     The /ui route will return 404. \
                     To build the UI, run `make build-ui` (or `make build-release`) from the \
                     repository root — `cargo build` alone does not build the UI. \
                     To suppress this warning, start the server with `--disable-ui` or set \
                     `enableUi: false` in the config file."
                );
            }
        } else {
            info!("Web UI is disabled by configuration");
        }

        let cors_layer = if self.cors_allowed_origins.is_empty() {
            CorsLayer::permissive()
        } else {
            use tower_http::cors::AllowOrigin;
            let origins: Vec<axum::http::HeaderValue> = self
                .cors_allowed_origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
        };

        let app = app.layer(cors_layer);

        let addr = format!("{}:{}", self.host, self.port);
        info!("Starting web API on {addr}");
        info!("API v1 available at http://{addr}/api/v1/");
        info!("Swagger UI available at http://{addr}/api/v1/docs/");
        if self.enable_ui && (has_filesystem_ui || has_embedded_ui) {
            info!("Drasi Server Admin UI at http://{addr}/ui/");
        }

        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Web API server error: {e}");
            }
        });

        Ok(())
    }
}

/// Register plugins that are always available regardless of feature flags.
pub fn register_core_plugins(registry: &mut PluginRegistry) {
    use std::sync::Arc;

    info!("Loading core plugins (static)...");

    let desc = drasi_bootstrap_noop::descriptor::NoOpBootstrapDescriptor;
    info!("  [static/core] bootstrap: {}", desc.kind());
    registry.register_bootstrapper(Arc::new(desc));

    let desc = drasi_bootstrap_application::descriptor::ApplicationBootstrapDescriptor;
    info!("  [static/core] bootstrap: {}", desc.kind());
    registry.register_bootstrapper(Arc::new(desc));

    let desc = drasi_reaction_application::descriptor::ApplicationReactionDescriptor;
    info!("  [static/core] reaction: {}", desc.kind());
    registry.register_reaction(Arc::new(desc));
}
