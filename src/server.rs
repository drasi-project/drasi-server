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

use anyhow::{Context, Result};
use axum::{routing::get, Router};
use log::{debug, info, warn};
use std::collections::HashMap;
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
use crate::api::models::BootstrapProviderConfig;
use crate::api::models::SecretStoreConfig;
use crate::config::DrasiServerConfig;
use crate::factories::{
    build_bootstrap_provider_config_map, build_config_resolver_context,
    build_identity_provider_map, config_resolver_callback, create_secret_store_from_registry,
    create_state_store_provider,
};
use crate::instance_paths::instance_storage_key;
use crate::instance_registry::InstanceRegistry;
use crate::load_config_file;
use crate::persistence::ConfigPersistence;
use crate::plugin_orchestrator::PluginOrchestrator;
use crate::plugin_registry::PluginRegistry;
use crate::{telemetry::StandaloneTelemetry, TelemetryBindings};
use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_lib::observability::{TelemetryProfile, TelemetrySettings};
use drasi_lib::secret_store::SecretStoreProvider;
use drasi_lib::DrasiLib;
use drasi_plugin_sdk::BootstrapPluginDescriptor;
use drasi_wal_redb::RedbWalProvider;

pub(crate) mod lifecycle;
pub use lifecycle::{
    CleanupPhase, CleanupStatus, ServerCleanupError, ServerCleanupHandle, ServerPreparation,
};

#[cfg(test)]
#[path = "server/lifecycle_tests.rs"]
mod lifecycle_tests;

pub struct DrasiServer {
    instances: Vec<PreparedInstance>,
    enable_api: bool,
    enable_ui: bool,
    host: String,
    port: u16,
    config_file_path: Option<String>,
    loaded_configuration: Option<DrasiServerConfig>,
    read_only: Arc<bool>,
    plugin_registry: Arc<RwLock<PluginRegistry>>,
    plugin_orchestrator: Arc<PluginOrchestrator>,
    cors_allowed_origins: Vec<String>,
    configuration_stores: crate::managed_configuration::ConfigurationStores,
    instance_registry: InstanceRegistry,
    metrics_config: Option<crate::telemetry::MetricsConfig>,
    cleanup: ServerCleanupHandle,
    new_telemetry: bool,
    prometheus: Option<crate::telemetry::prometheus::PreparedPrometheus>,
    inspection: Option<Arc<api::v1::telemetry_handlers::TelemetryInspection>>,
}

impl Drop for DrasiServer {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup.request_shutdown() {
            log::error!("Server cleanup remains owned but could not be scheduled: {error}");
        }
    }
}

struct RunningWait(ServerCleanupHandle);

impl Drop for RunningWait {
    fn drop(&mut self) {
        if let Err(error) = self.0.request_shutdown() {
            log::error!("Server run waiter ended; cleanup remains owned: {error}");
        }
    }
}

struct PreparedInstance {
    id_hint: Option<String>,
    core: DrasiLib,
    /// Top-level bootstrap provider configs declared for this instance,
    /// keyed by provider id. Registered into the `InstanceRegistry` so
    /// runtime-created sources can resolve `bootstrapProvider: <id>`
    /// references. Empty for programmatically built instances.
    bootstrap_providers: HashMap<String, BootstrapProviderConfig>,
    published: bool,
}

enum FileTelemetry {
    Standalone,
    Borrowed(TelemetryBindings),
    Initialized(TelemetryBindings),
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
        Self::prepare_new(config_path, port, plugins_dir, skip_verification, enable_ui)
            .finish()
            .await
    }

    /// Prepare configured startup with owned logging before plugin construction.
    pub fn prepare_new(
        config_path: PathBuf,
        port: u16,
        plugins_dir: PathBuf,
        skip_verification: bool,
        enable_ui: bool,
    ) -> ServerPreparation<Self> {
        Self::prepare_file(
            config_path,
            port,
            plugins_dir,
            skip_verification,
            enable_ui,
            ServerCleanupHandle::bootstrap(),
            FileTelemetry::Standalone,
        )
    }

    /// Borrow host integrations; a simultaneous YAML telemetry section rejects.
    pub fn prepare_new_with_telemetry(
        config_path: PathBuf,
        port: u16,
        plugins_dir: PathBuf,
        skip_verification: bool,
        enable_ui: bool,
        bindings: TelemetryBindings,
    ) -> ServerPreparation<Self> {
        Self::prepare_file(
            config_path,
            port,
            plugins_dir,
            skip_verification,
            enable_ui,
            ServerCleanupHandle::bootstrap(),
            FileTelemetry::Borrowed(bindings),
        )
    }

    /// Transfer an already initialized standalone owner without reinstalling globals.
    pub fn prepare_new_with_runtime(
        config_path: PathBuf,
        port: u16,
        plugins_dir: PathBuf,
        skip_verification: bool,
        enable_ui: bool,
        runtime: StandaloneTelemetry,
    ) -> ServerPreparation<Self> {
        let (bindings, cleanup) = runtime.into_server_parts();
        Self::prepare_file(
            config_path,
            port,
            plugins_dir,
            skip_verification,
            enable_ui,
            cleanup,
            FileTelemetry::Initialized(bindings),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_file(
        config_path: PathBuf,
        port: u16,
        plugins_dir: PathBuf,
        skip_verification: bool,
        enable_ui: bool,
        cleanup: ServerCleanupHandle,
        telemetry: FileTelemetry,
    ) -> ServerPreparation<Self> {
        let submitted = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start_blocking(admission, move |owner| {
                tokio::runtime::Handle::try_current()?.block_on(Self::prepare_file_inner(
                    config_path,
                    port,
                    plugins_dir,
                    skip_verification,
                    enable_ui,
                    owner,
                    telemetry,
                ))
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_file_inner(
        config_path: PathBuf,
        port: u16,
        plugins_dir: PathBuf,
        skip_verification: bool,
        enable_ui: bool,
        cleanup: ServerCleanupHandle,
        telemetry: FileTelemetry,
    ) -> Result<Self> {
        let mut config = crate::config::loader::load_config_file_with_overrides(
            &config_path,
            Some(port),
            Some(enable_ui),
        )?;
        let resolved_settings = map_server_settings(&config, &DtoMapper::new())?;
        let new_telemetry =
            config.telemetry.is_some() || matches!(&telemetry, FileTelemetry::Borrowed(_));
        anyhow::ensure!(
            !matches!(&telemetry, FileTelemetry::Borrowed(_)) || config.telemetry.is_none(),
            "Configured telemetry and borrowed telemetry bindings are mutually exclusive"
        );
        let effective = config.resolved_telemetry(&DtoMapper::new(), port, enable_ui)?;
        if let Some(effective) = &effective {
            effective.ensure_runtime_available()?;
        }
        let runtime_registry = InstanceRegistry::new();
        cleanup.retain_registry(runtime_registry.clone())?;
        let mut prometheus = None;
        let mut inspection = None;
        let bindings = match telemetry {
            FileTelemetry::Borrowed(bindings) => {
                cleanup.retain_bindings(bindings.clone())?;
                bindings
            }
            owned => {
                let bindings = match owned {
                    FileTelemetry::Initialized(bindings) => bindings,
                    _ => StandaloneTelemetry::initialize_logging(
                        &cleanup,
                        resolved_settings.log_level.clone(),
                    )?,
                };
                if let Some(effective) = effective {
                    let collection = crate::telemetry::runtime::prepare_collection(
                        &cleanup,
                        effective,
                        &runtime_registry,
                        bindings,
                    )
                    .await?;
                    prometheus = collection.prometheus;
                    inspection = collection.inspection;
                    collection.bindings
                } else {
                    // Legacy root `metrics` keeps owned instances Off, replacing
                    // the default policy on the same owner before any Core.
                    let bindings =
                        bindings.with_settings(TelemetrySettings::new(TelemetryProfile::Off));
                    cleanup.configure_logging_bindings(bindings.clone())?;
                    // Logged after owned logging is installed so standalone startup emits it.
                    info!("Root `metrics` keeps the legacy query-progress export and Server telemetry stays off; replace it with a `telemetry` section to migrate");
                    bindings
                }
            }
        };
        let runtime_registry =
            runtime_registry.with_runtime_context(bindings.clone(), cleanup.clone());

        // CLI --skip-verification flag overrides config (disables when set)
        if skip_verification {
            config.verify_plugins = false;
        }

        // Create and populate the plugin registry
        let mut plugin_registry = PluginRegistry::new();
        register_core_plugins(&mut plugin_registry);

        // Auto-install plugins from registry if configured
        if config.auto_install_plugins && !config.plugins.is_empty() {
            if config.verify_plugins {
                let registry = config
                    .plugin_registry
                    .as_deref()
                    .unwrap_or("ghcr.io/drasi-project");
                anyhow::ensure!(
                    !matches!(
                        drasi_host_sdk::registry::PluginSourceKind::parse(registry),
                        drasi_host_sdk::registry::PluginSourceKind::LocalDir(_)
                    ),
                    "Local plugin discovery executes metadata before signature verification; use signed OCI plugins or explicitly disable verification for trusted local builds"
                );
            }
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
                            warn!("{filename} — no integrity hash in lockfile; cannot verify the local binary");
                            if let Some(ref mut allowed) = verified_files {
                                allowed.remove(filename);
                            }
                        }
                        FileIntegrityStatus::Missing => {
                            debug!("{filename} — file not on disk");
                        }
                        FileIntegrityStatus::Error(e) => {
                            warn!("{filename} — integrity check error: {e}");
                            if let Some(ref mut allowed) = verified_files {
                                allowed.remove(filename);
                            }
                        }
                    }
                }
            }
        }

        // Load dynamic plugins from the plugins directory
        let mut startup_plugin_records = Vec::new();
        let mut plugin_load_stats: Option<crate::dynamic_loading::PluginLoadStats> = None;
        if plugins_dir.exists() {
            let callback_ctx = bindings.plugin_callback_context()?;
            let load_stats = crate::dynamic_loading::load_plugins(
                &plugins_dir,
                &mut plugin_registry,
                Some(callback_ctx),
                verified_files.as_ref(),
            )?;
            startup_plugin_records = load_stats.loaded_plugins.clone();
            plugin_load_stats = Some(load_stats);
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
        plugin_orchestrator
            .bind_instances(runtime_registry.clone())
            .await?;
        cleanup.retain_runtime(
            runtime_registry.clone(),
            plugin_registry.clone(),
            plugin_orchestrator.clone(),
        )?;

        // Register startup-loaded plugins in the orchestrator
        plugin_orchestrator
            .record_startup_plugins(&startup_plugin_records)
            .await;

        let configuration_stores = crate::managed_configuration::ConfigurationStores::default();
        configuration_stores
            .resolve_instance_ids(&mut config)
            .await?;

        // Start plugin hot-reload watcher if configured
        let start_watcher = || async {
            if config.hot_reload_plugins {
                use drasi_host_sdk::watcher::{PluginWatcher, PluginWatcherConfig};

                let watcher_config = PluginWatcherConfig {
                    plugins_dir: plugins_dir.clone(),
                    debounce: std::time::Duration::from_millis(config.hot_reload_debounce_ms),
                };
                let watcher = PluginWatcher::new(watcher_config);
                let mut rx = watcher.subscribe();
                let orchestrator_for_watcher = plugin_orchestrator.clone();
                let watcher_admission = cleanup.admit()?;
                let cleanup_for_watcher = cleanup.clone();

                let backend = watcher.prepare_owned_polling();
                cleanup.retain_watcher(backend.clone())?;
                backend.start().await.map_err(anyhow::Error::new)?;

                // Spawn a task that receives file events and applies the configured policy
                cleanup.start_watcher(watcher_admission, async move {
                    loop {
                        let event = tokio::select! {
                            biased;
                            _ = cleanup_for_watcher.admission_closed() => break,
                            event = rx.recv() => event,
                        };
                        match event {
                            Ok(event) => {
                                use drasi_host_sdk::plugin_types::PluginFileEvent;
                                match event {
                                    PluginFileEvent::Added(path)
                                    | PluginFileEvent::Changed(path) => {
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
                })?;
                info!(
                    "Plugin hot-reload enabled (owned polling, debounce/scan interval: {}ms)",
                    config.hot_reload_debounce_ms
                );
                Ok::<_, anyhow::Error>(())
            } else {
                Ok(())
            }
        };

        // Resolve server settings using the mapper
        let mapper = DtoMapper::new();
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
            info!("YAML persistence disabled by configuration (persist_config: false).");
            warn!("Instance provisioning and ordinary API modifications will not persist to YAML. Configured managed stores still persist accepted desired-state requests.");
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

        // Resolve the process-wide secret store provider (if any instance configures one).
        // The FFI config resolver is a single global callback — only one secret store
        // provider can be active. Validate that all instances agree on the same config.
        let process_secret_store: Option<Arc<dyn SecretStoreProvider>> = {
            let mut chosen: Option<(&str, &SecretStoreConfig)> = None;
            for inst in &resolved_instances {
                if let Some(ref ss) = inst.secret_store {
                    if let Some((prev_id, prev_cfg)) = chosen {
                        if prev_cfg != ss {
                            return Err(anyhow::anyhow!(
                                "Conflicting secret store configurations: instance '{}' and '{}' \
                                 specify different secret store configs. Only one process-wide \
                                 secret store is supported because the FFI config resolver is global.",
                                prev_id,
                                inst.id,
                            ));
                        }
                    } else {
                        chosen = Some((&inst.id, ss));
                    }
                }
            }
            if let Some((inst_id, ss_config)) = chosen {
                info!(
                    "Enabling process-wide secret store with '{}' provider (from instance '{}')",
                    ss_config.kind, inst_id
                );
                let provider =
                    create_secret_store_from_registry(&plugin_registry, ss_config).await?;

                // Inject the config resolver callback into all loaded plugins so their
                // DtoMapper can resolve ConfigValue::Secret references through the host.
                if let Some(ref stats) = plugin_load_stats {
                    let resolver_ctx = build_config_resolver_context(
                        provider.clone(),
                        tokio::runtime::Handle::current(),
                    );
                    stats.inject_config_resolver_into_all(resolver_ctx, config_resolver_callback());
                    info!(
                        "Injected config resolver into {} loaded plugin(s)",
                        stats.plugins_loaded,
                    );
                }
                Some(provider)
            } else {
                None
            }
        };

        let preparation = async {
            for instance in resolved_instances {
                let mut builder = DrasiLib::builder()
                    .with_id(&instance.id)
                    .with_allow_external_component_ids(true);
                builder = runtime_registry.bind_builder(builder)?;

                // Set capacity defaults if configured (resolve env vars)
                if let Some(capacity) = instance.default_priority_queue_capacity {
                    builder = builder.with_priority_queue_capacity(capacity);
                }
                if let Some(capacity) = instance.default_dispatch_buffer_capacity {
                    builder = builder.with_dispatch_buffer_capacity(capacity);
                }
                if let Some(seconds) = instance.activation_timeout_secs {
                    builder = builder.with_activation_timeout_secs(seconds);
                }
                if let Some(seconds) = instance.startup_timeout_secs {
                    builder = builder.with_startup_timeout_secs(seconds);
                }
                if let Some(seconds) = instance.shutdown_timeout_secs {
                    builder = builder.with_shutdown_timeout_secs(seconds);
                }
                if let Some(seconds) = instance.construction_timeout_secs {
                    builder = builder.with_construction_timeout_secs(seconds);
                }
                if let Some(seconds) = instance.resource_construction_timeout_secs {
                    builder = builder.with_resource_construction_timeout_secs(seconds);
                }
                if let Some(seconds) = instance.cleanup_timeout_secs {
                    builder = builder.with_cleanup_timeout_secs(seconds);
                }

                // Filesystem-safe key shared by the persistent index and WAL paths.
                let safe_id = instance_storage_key(&instance.id);

                // Register the persistent RocksDB index provider as the instance
                // default when persist_index is enabled.
                if instance.persist_index {
                    builder = crate::index_provider::apply_rocksdb_index(
                        builder,
                        &instance.id,
                        instance.enable_archive,
                        instance.memory_budget_bytes,
                    )?;
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
                    let wal_path = PathBuf::from(format!("./data/{safe_id}/wal"));
                    info!(
                        "Enabling WAL provider for instance '{}' at: {}",
                        instance.id,
                        wal_path.display()
                    );
                    let wal_provider = Arc::new(RedbWalProvider::new(&wal_path));
                    builder = builder.with_wal_provider(wal_provider);
                }
                // Attach the process-wide secret store provider to this instance's builder
                if instance.secret_store.is_some() {
                    if let Some(ref provider) = process_secret_store {
                        builder = builder.with_secret_store_provider(provider.clone());
                    }
                }

                // Build the identity-provider map for this instance. Sources and
                // reactions can reference entries here via `identityProvider: <id>`.
                let identity_providers =
                    build_identity_provider_map(&plugin_registry, &instance.identity_providers)
                        .await?;
                // Build the bootstrap-provider config map for this instance. Sources
                // can reference entries here via `bootstrapProvider: <id>`; each
                // referencing source instantiates its own provider from the config.
                let bootstrap_providers =
                    build_bootstrap_provider_config_map(&instance.bootstrap_providers)?;
                let ordinary = crate::ordinary_resolver::OrdinarySeed {
                    sources: &instance.sources,
                    queries: &instance.queries,
                    reactions: &instance.reactions,
                };
                let settings = crate::ordinary_resolver::InstanceComponentSettings {
                    identity_providers: identity_providers.clone(),
                    bootstrap_providers: bootstrap_providers.clone(),
                };
                let secrets = process_secret_store
                    .clone()
                    .filter(|_| instance.secret_store.is_some());
                info!(
                    "Loading {} source(s), {} query(ies) and {} reaction(s) from configuration for instance '{}'",
                    instance.sources.len(),
                    instance.queries.len(),
                    instance.reactions.len(),
                    instance.id
                );
                // Components are managed definitions in both modes; a store only
                // makes them durable.
                let in_memory = if let Some(store) = &instance.configuration_store {
                    builder = configuration_stores
                        .configure_with_components(
                            builder,
                            &instance.id,
                            store,
                            instance.computation.as_ref(),
                            ordinary,
                            settings,
                            plugin_registry.clone(),
                            secrets,
                        )
                        .await?;
                    None
                } else {
                    let (prepared, seed) = crate::managed_configuration::configure_in_memory(
                        builder,
                        &instance.id,
                        ordinary,
                        settings,
                        plugin_registry.clone(),
                        secrets,
                    )
                    .await?;
                    builder = prepared;
                    Some(seed)
                };
                let core = builder.build().await.context("Failed to create DrasiLib")?;
                cleanup.retain_core(Arc::new(core.clone()))?;
                runtime_registry
                    .publish_instance(instance.id.clone(), Arc::new(core.clone()))
                    .await?;

                let instance_id = instance.id.clone();
                instances.push(PreparedInstance {
                    id_hint: Some(instance.id),
                    core: core.clone(),
                    bootstrap_providers,
                    published: true,
                });
                if let Some(seed) = in_memory {
                    crate::managed_configuration::seed_in_memory(&core, seed)
                        .await
                        .with_context(|| {
                            format!("Failed to configure components of instance '{instance_id}'")
                        })?;
                    if let Some(computation) = instance.computation.as_ref() {
                        let registry = plugin_registry.read().await;
                        let factories = crate::computation::native_factory_registry(&registry)?;
                        let computation = computation.resolve(&factories)?;
                        let registration =
                            crate::computation::register_components(&computation, &core, &registry);
                        drop(registry);
                        registration.await.with_context(|| {
                            "Failed to configure instance computation components"
                        })?;
                    }
                    // Acceptance is asynchronous; declare everything before startup.
                    core.reconcile_desired_state().await.with_context(|| {
                        format!("Failed to declare components of instance '{instance_id}'")
                    })?;
                }
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        preparation?;
        start_watcher().await?;

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
            configuration_stores,
            instance_registry: runtime_registry,
            metrics_config: config.metrics.clone(),
            loaded_configuration: Some(config),
            cleanup,
            new_telemetry,
            prometheus,
            inspection,
        })
    }

    /// Create a DrasiServer from a pre-built core (for use with builder)
    ///
    /// Server owns no telemetry for this core. API-created instances follow
    /// DrasiLib's default (Basic, with metrics forwarded to the current
    /// `metrics` recorder), and no telemetry inspection route is mounted.
    pub fn from_core(
        core: DrasiLib,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
    ) -> Self {
        Self::from_cores(
            vec![(core, None)],
            enable_api,
            enable_ui,
            host,
            port,
            config_file_path,
        )
    }

    /// Create a DrasiServer from multiple pre-built cores (for builder multi-instance usage)
    ///
    /// Server owns no telemetry for these cores. API-created instances follow
    /// DrasiLib's default (Basic, with metrics forwarded to the current
    /// `metrics` recorder), and no telemetry inspection route is mounted.
    pub fn from_cores(
        cores: Vec<(DrasiLib, Option<String>)>,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
    ) -> Self {
        let cleanup = ServerCleanupHandle::prepared(
            cores
                .iter()
                .map(|(core, _)| Arc::new(core.clone()))
                .collect(),
        );
        Self::assemble(
            cores,
            enable_api,
            enable_ui,
            host,
            port,
            config_file_path,
            cleanup,
            false,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_prepared_cores(
        cores: Vec<(DrasiLib, Option<String>)>,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
        cleanup: ServerCleanupHandle,
        new_telemetry: bool,
        registry: InstanceRegistry,
    ) -> Self {
        let cores = cores
            .into_iter()
            .map(|(core, _)| {
                let id = core.get_config().id.clone();
                (core, Some(id))
            })
            .collect();
        Self::assemble(
            cores,
            enable_api,
            enable_ui,
            host,
            port,
            config_file_path,
            cleanup,
            new_telemetry,
            Some(registry),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        cores: Vec<(DrasiLib, Option<String>)>,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
        cleanup: ServerCleanupHandle,
        new_telemetry: bool,
        registry: Option<InstanceRegistry>,
    ) -> Self {
        let instances = cores
            .into_iter()
            .map(|(core, id_hint)| PreparedInstance {
                id_hint,
                core,
                bootstrap_providers: HashMap::new(),
                published: registry.is_some(),
            })
            .collect();

        let mut plugin_registry = PluginRegistry::new();
        register_core_plugins(&mut plugin_registry);
        let plugin_registry = Arc::new(RwLock::new(plugin_registry));
        let lifecycle = Arc::new(PluginLifecycleManager::new(plugin_registry.clone()));
        let plugin_orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));
        let instance_registry = registry.unwrap_or_else(|| match cleanup.bindings() {
            Some(bindings) => {
                InstanceRegistry::new().with_runtime_context(bindings, cleanup.clone())
            }
            None => InstanceRegistry::new().with_cleanup_context(cleanup.clone()),
        });
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
            configuration_stores: Default::default(),
            instance_registry,
            loaded_configuration: None,
            metrics_config: None,
            cleanup,
            new_telemetry,
            prometheus: None,
            inspection: None,
        }
    }

    pub(crate) fn with_prepared_prometheus(
        mut self,
        prometheus: Option<crate::telemetry::prometheus::PreparedPrometheus>,
    ) -> Self {
        self.prometheus = prometheus;
        self
    }

    pub(crate) fn with_prepared_inspection(
        mut self,
        inspection: Option<Arc<api::v1::telemetry_handlers::TelemetryInspection>>,
    ) -> Self {
        self.inspection = inspection;
        self
    }

    /// Address retained instance cleanup independently of Server serving.
    pub fn cleanup_handle(&self) -> ServerCleanupHandle {
        self.cleanup.clone()
    }

    #[cfg(test)]
    pub(crate) fn instance_registry(&self) -> &InstanceRegistry {
        &self.instance_registry
    }

    /// Check an already-configured Core without injecting or replacing handles.
    ///
    /// The caller owns borrowed providers. Exposed policy, scope, capability
    /// and logging identity are checked; opaque collector/recorder identity
    /// and exporter health remain the caller's preconfiguration obligations.
    ///
    /// ```no_run
    /// use drasi_server::{DrasiLib, DrasiServer, TelemetryBindings};
    /// use std::time::Duration;
    /// use tokio::time::Instant;
    ///
    /// # async fn example(core: DrasiLib, bindings: TelemetryBindings) -> anyhow::Result<()> {
    /// let server = DrasiServer::from_core_with_telemetry(
    ///     core, false, false, "127.0.0.1".into(), 8080, None, bindings,
    /// ).await?;
    /// server.cleanup_handle()
    ///     .wait_until(Instant::now() + Duration::from_secs(30)).await?;
    /// // Borrowed providers are still open and remain application-owned.
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_core_with_telemetry(
        core: DrasiLib,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
        bindings: crate::TelemetryBindings,
    ) -> Result<Self> {
        Self::from_cores_with_telemetry(
            vec![(core, None)],
            enable_api,
            enable_ui,
            host,
            port,
            config_file_path,
            bindings,
        )
        .await
    }

    /// Retain all supplied Cores through checked validation and any rollback.
    pub async fn from_cores_with_telemetry(
        cores: Vec<(DrasiLib, Option<String>)>,
        enable_api: bool,
        enable_ui: bool,
        host: String,
        port: u16,
        config_file_path: Option<String>,
        bindings: crate::TelemetryBindings,
    ) -> Result<Self> {
        let cleanup = ServerCleanupHandle::prepared(
            cores
                .iter()
                .map(|(core, _)| Arc::new(core.clone()))
                .collect(),
        );
        let submitted = (|| {
            let admission = cleanup.admit()?;
            ServerPreparation::start(admission, move |owner| async move {
                owner.retain_bindings(bindings.clone())?;
                let registry =
                    InstanceRegistry::new().with_runtime_context(bindings.clone(), owner.clone());
                owner.retain_registry(registry.clone())?;
                let mut ids = std::collections::HashSet::new();
                for (core, _) in &cores {
                    bindings.validate_prebuilt(core).await?;
                    anyhow::ensure!(
                        ids.insert(core.get_config().id.clone()),
                        "Duplicate prebuilt Core instance ID"
                    );
                    registry
                        .publish_instance(core.get_config().id.clone(), Arc::new(core.clone()))
                        .await?;
                }
                Ok(Self::from_prepared_cores(
                    cores,
                    enable_api,
                    enable_ui,
                    host,
                    port,
                    config_file_path,
                    owner,
                    true,
                    registry,
                ))
            })
        })();
        match submitted {
            Ok(preparation) => preparation,
            Err(error) => ServerPreparation::failed(cleanup, error),
        }
        .finish()
        .await
    }

    /// Check if we have write access to the config file
    fn check_write_access(path: &PathBuf) -> bool {
        // Try to open the file with write permissions
        OpenOptions::new().append(true).open(path).is_ok()
    }

    /// Configure optional OTLP progress metrics for a programmatically built server.
    pub fn with_metrics(mut self, config: crate::telemetry::MetricsConfig) -> Result<Self> {
        anyhow::ensure!(
            !self.new_telemetry,
            "Legacy progress metrics cannot be combined with configured telemetry or borrowed telemetry bindings"
        );
        config.validate()?;
        self.metrics_config = Some(config);
        Ok(self)
    }

    /// Request the same cleanup attempt as signals, cancellation and startup
    /// rollback. The first request anchors the overall 30-second wait.
    pub async fn shutdown(&self) -> Result<()> {
        self.cleanup.shutdown().await
    }

    /// Consume the Server, retaining a cleanup address before calling this if
    /// the embedding application may cancel its wait or keep Tokio alive later.
    pub async fn run(self) -> Result<()> {
        let cleanup = self.cleanup.clone();
        let _waiter = RunningWait(cleanup.clone());
        let work = cleanup.start_management(move || async move {
            let mut server = self;
            if let Err(error) = server.run_until_shutdown().await {
                server.cleanup.record_failure(error);
            }
        })?;
        cleanup.admission_closed().await;
        let outcome = cleanup.shutdown().await;
        if outcome.is_ok() {
            work.finish().await?;
            info!("Drasi Server shutdown complete");
        }
        outcome
    }

    #[allow(clippy::print_stdout)]
    async fn run_until_shutdown(&mut self) -> Result<()> {
        let startup = self.cleanup.admit()?;
        #[cfg(unix)]
        let (mut interrupt, mut terminate) = {
            use tokio::signal::unix::{signal, SignalKind};
            (
                signal(SignalKind::interrupt()).context("Failed to register SIGINT handler")?,
                signal(SignalKind::terminate()).context("Failed to register SIGTERM handler")?,
            )
        };
        let signal_cleanup = self.cleanup.clone();
        self.cleanup.start_signal(async move {
            #[cfg(unix)]
            {
                tokio::select! {
                    _ = signal_cleanup.admission_closed() => Ok(()),
                    received = interrupt.recv() => received.context("SIGINT signal stream closed"),
                    received = terminate.recv() => received.context("SIGTERM signal stream closed"),
                }
            }
            #[cfg(not(unix))]
            {
                tokio::select! {
                    _ = signal_cleanup.admission_closed() => Ok(()),
                    result = tokio::signal::ctrl_c() => result.context("Failed to listen for Ctrl+C"),
                }
            }
        })?;

        println!("Starting Drasi Server");
        println!("  Version: {}", env!("CARGO_PKG_VERSION"));
        println!("  Rust: {}", env!("DRASI_RUSTC_VERSION"));
        println!("  Plugin SDK crate: {}", env!("DRASI_PLUGIN_SDK_VERSION"));
        println!(
            "  Plugin ABI: {}",
            drasi_plugin_sdk::ffi::metadata::FFI_SDK_VERSION
        );
        if let Some(config_file) = &self.config_file_path {
            println!("  Config file: {config_file}");
        }
        println!("  API Port: {}", self.port);
        println!(
            "  Log level: {}",
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
        );
        info!("Initializing Drasi Server");

        let registry = self
            .instance_registry
            .clone()
            .with_configuration_stores(self.configuration_stores.clone());
        self.cleanup.retain_runtime(
            registry.clone(),
            self.plugin_registry.clone(),
            self.plugin_orchestrator.clone(),
        )?;
        let mut bootstrap_providers_by_id: Vec<(String, HashMap<String, BootstrapProviderConfig>)> =
            Vec::new();

        // Take ownership of instances to avoid partial move of self
        let instances = std::mem::take(&mut self.instances);
        for instance in instances {
            let core = instance.core;
            let bootstrap_providers = instance.bootstrap_providers;
            let id = match instance.id_hint {
                Some(id) => id,
                None => core
                    .get_current_config()
                    .await
                    .map(|c| c.id.clone())
                    .map_err(|e| anyhow::anyhow!("Failed to resolve DrasiLib id: {e}"))?,
            };

            let core = Arc::new(core);
            info!("Starting ComputationGraph instance '{id}'");
            println!("  Instance '{id}': ComputationGraph");
            // A stalled start must still observe shutdown requests. Server
            // cleanup owns every prepared core and shuts them down.
            let started = tokio::select! {
                result = crate::managed_configuration::start_instance(&core) => Some(result),
                _ = self.cleanup.admission_closed() => None,
            };
            match started {
                Some(result) => result?,
                None => {
                    warn!("Shutdown requested while starting instance '{id}'");
                    anyhow::bail!("Drasi Server shutdown requested during startup");
                }
            }
            bootstrap_providers_by_id.push((id.clone(), bootstrap_providers));
            if !instance.published {
                registry.publish_instance(id, core).await?;
            }
        }

        if let Some(config) = &self.metrics_config {
            let metrics = Arc::new(tokio::sync::Mutex::new(
                crate::telemetry::MetricsRuntime::default(),
            ));
            self.cleanup.retain_metrics(metrics.clone())?;
            metrics
                .lock()
                .await
                .start(config.clone(), &registry)
                .await?;
        }
        self.plugin_orchestrator
            .bind_instances(registry.clone())
            .await?;

        // Record each instance's top-level bootstrap provider configs so the
        // source create/upsert handlers can resolve `bootstrapProvider: <id>`
        // references for sources created at runtime.
        for (id, providers) in bootstrap_providers_by_id {
            registry.set_bootstrap_providers(id, providers).await;
        }

        // Initialize persistence and extract solutions_dir if config file is provided
        let (config_persistence, solutions_dir) = if let Some(config_file) = &self.config_file_path
        {
            if !*self.read_only {
                let config = match &self.loaded_configuration {
                    Some(config) => config.clone(),
                    None => load_config_file(PathBuf::from(config_file))?,
                };
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
                        &config,
                    )?);

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

        let management = if self.enable_api {
            let app = self
                .management_router(registry.clone(), config_persistence.clone(), solutions_dir)
                .await?;
            let socket = tokio::net::TcpListener::bind((self.host.as_str(), self.port)).await?;
            Some((socket, app))
        } else {
            None
        };
        let prometheus = match &self.prometheus {
            Some(prometheus) => prometheus.bind_separate(&self.host).await?,
            None => None,
        };
        // All required sockets are bound before either listener can serve or
        // collection is activated. Failure above drops every partial binding.
        if let Some((socket, app)) = management {
            self.cleanup
                .serve_listener(lifecycle::ApiListener::Management, socket, app)?;
        }
        if let Some((socket, app)) = prometheus {
            self.cleanup
                .serve_listener(lifecycle::ApiListener::Prometheus, socket, app)?;
        }
        if self.metrics_config.is_none() {
            self.cleanup.activate_collection().await?;
        }
        if self.cleanup.admission_is_open() {
            self.cleanup.running()?;
            if self.enable_api {
                info!(
                    "Drasi Server started successfully with API on port {}",
                    self.port
                );
            } else {
                info!("Drasi Server started successfully (API disabled)");
            }
        }
        drop(startup);

        self.cleanup.admission_closed().await;

        info!("Shutting down Drasi Server");
        Ok(())
    }

    async fn management_router(
        &self,
        registry: InstanceRegistry,
        config_persistence: Option<Arc<ConfigPersistence>>,
        solutions_dir: Option<String>,
    ) -> Result<Router> {
        // Create OpenAPI documentation for v1 with cache
        let mut openapi_v1 = api::ApiDocV1::openapi();
        let registry_version = {
            let reg = self.plugin_registry.read().await;
            api::inject_plugin_schemas(&mut openapi_v1, &reg);
            reg.version()
        };

        let openapi_cache = Arc::new(api::v1::OpenApiCache::new(
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
        let plugin_router = api::v1::build_plugin_router(
            self.plugin_orchestrator.clone(),
            registry.clone(),
            self.read_only.clone(),
        );

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
            .route(
                "/api/v1/openapi.json",
                get(move || {
                    let cache = openapi_cache.clone();
                    async move { axum::Json(cache.get_spec().await) }
                }),
            )
            .merge(
                SwaggerUi::new("/api/v1/docs")
                    .config(utoipa_swagger_ui::Config::from("/api/v1/openapi.json")),
            );

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

        if let Some(prometheus) = &self.prometheus {
            if prometheus.config.port.is_none() {
                app = app.merge(prometheus.router()?);
            }
        }
        if let Some(inspection) = &self.inspection {
            // route_layer applies management admission to the mounted inspection
            // routes, including their 405 responses, without wrapping the
            // listener-wide 404 fallback.
            app = app.merge(
                api::v1::telemetry_handlers::telemetry_router(inspection.clone()).route_layer(
                    axum::middleware::from_fn_with_state(
                        registry.clone(),
                        api::v1::routes::management_admission,
                    ),
                ),
            );
        }
        let app = app.layer(cors_layer);

        let addr = format!("{}:{}", self.host, self.port);
        info!("Starting web API on {addr}");
        info!("API v1 available at http://{addr}/api/v1/");
        info!("Swagger UI available at http://{addr}/api/v1/docs/");
        if self.enable_ui && (has_filesystem_ui || has_embedded_ui) {
            info!("Drasi Server Admin UI at http://{addr}/ui/");
        }

        Ok(app)
    }
}

/// Register plugins that are always available regardless of feature flags.
pub fn register_core_plugins(registry: &mut PluginRegistry) {
    use std::sync::Arc;

    info!("Loading core plugins (static)...");

    let desc = drasi_bootstrap_noop::descriptor::NoOpBootstrapDescriptor;
    info!("  [static/core] bootstrap: {}", desc.kind());
    registry.register_bootstrapper(Arc::new(desc));
}

#[cfg(test)]
#[path = "server/managed_tests.rs"]
mod managed_tests;

#[cfg(test)]
async fn configured_test_server(
    path: PathBuf,
    port: u16,
    plugins: PathBuf,
    skip_verification: bool,
    enable_ui: bool,
) -> Result<DrasiServer> {
    let logging =
        tokio::task::spawn_blocking(drasi_lib::observability::init_default_subscriber).await??;
    DrasiServer::prepare_new_with_telemetry(
        path,
        port,
        plugins,
        skip_verification,
        enable_ui,
        TelemetryBindings::new(
            drasi_lib::observability::TelemetrySettings::default(),
            logging,
        ),
    )
    .finish()
    .await
}

#[cfg(test)]
mod single_runtime_tests {
    use super::*;

    #[test]
    fn core_registry_does_not_invent_application_owned_channels_or_snapshot_data() {
        let mut registry = PluginRegistry::new();
        register_core_plugins(&mut registry);
        assert!(registry.get_bootstrapper("noop").is_some());
        assert!(registry.get_bootstrapper("application").is_none());
        assert!(registry.get_reaction("application").is_none());
    }

    #[tokio::test]
    async fn native_components_are_registered_before_instance_start() {
        use drasi_lib::computation::v1::*;
        let resource = ResourceId::try_new("middleware").unwrap();
        let definition = MiddlewareTransformerDefinition {
            id: ComponentId::from_uuid(uuid::Uuid::from_u128(1)),
            output_stream: StreamId::try_new("native/out").unwrap(),
            middleware: Vec::new(),
            pipeline: Vec::new(),
        };
        let specification = definition.specification(resource.clone()).unwrap();
        let topology: DesiredTopology = serde_json::from_value(serde_json::json!({
            "version":DESIRED_TOPOLOGY_VERSION, "revision":0,
            "components":[{
                "name":"native-transformer",
                "descriptor":specification.descriptor, "role":specification.role, "completion":null,
                "streams":{}, "lifecycle":LifecyclePolicy { auto_start: false }, "input_merge":InputMergePolicy::default(),
                "construction":ComponentConstruction::Factory(specification.clone()),
            }],
            "resources":[{
                "id":resource, "role":ResourceRole::Middleware,
                "ownership":ResourceOwnership::Borrowed, "binding":"instance-middleware",
            }],
            "resource_configurations":{"middleware":{"kind":"middleware"}},
            "relationships":[], "boundary_relationships":[], "requirements":PipeRequirements::default(),
        })).unwrap();
        let graph_config = crate::computation::ComputationConfig {
            definition: topology,
        };
        let config = crate::config::DrasiServerConfig {
            instances: vec![crate::config::DrasiLibInstanceConfig {
                id: crate::api::models::ConfigValue::Static("native-startup".into()).into(),
                computation: Some(graph_config.clone().into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("server.yaml");
        config.save_to_file(&path).unwrap();
        let server = configured_test_server(
            path.clone(),
            8080,
            directory.path().join("plugins"),
            false,
            false,
        )
        .await
        .unwrap();
        let core = &server.instances[0].core;
        assert!(!core.is_running().await);
        let handle = core.computation_control().unwrap();
        let component = core
            .component_by_name(&ComponentName::try_new("native-transformer").unwrap())
            .unwrap();
        assert_eq!(component.id(), &definition.id);
        assert_eq!(
            handle.desired_snapshot().specifications[&definition.id],
            specification
        );
        assert_eq!(
            specification
                .descriptor
                .ports()
                .iter()
                .map(|port| (port.id().as_str(), port.direction()))
                .collect::<Vec<_>>(),
            vec![("in", PortDirection::Input), ("out", PortDirection::Output)]
        );
        // This disconnected declaration tests registration, not completed port binding.
        let expected_waits = [
            DependencyWait::Connection {
                endpoint: Endpoint::new(definition.id.clone(), PortId::try_new("in").unwrap()),
            },
            DependencyWait::Connection {
                endpoint: Endpoint::new(definition.id.clone(), PortId::try_new("out").unwrap()),
            },
            DependencyWait::Stream {
                endpoint: Endpoint::new(definition.id.clone(), PortId::try_new("out").unwrap()),
            },
        ];
        let mut observations = handle.subscribe_observed();
        let observed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            observations
                .wait_for(|graph| {
                    let observed = &graph.components[&definition.id];
                    observed.failure.is_some()
                        || observed.realization == RealizationState::CreationFailed
                        || (observed.realization == RealizationState::Blocked
                            && observed.waiting_for.len() == expected_waits.len()
                            && expected_waits
                                .iter()
                                .all(|wait| observed.waiting_for.contains(wait)))
                })
                .await
                .expect("component observations must remain available")
                .components[&definition.id]
                .clone()
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "native declaration did not settle on its missing bindings: {:?}",
                component.observed()
            )
        });
        assert_eq!(observed.realization, RealizationState::Blocked);
        assert!(observed.failure.is_none(), "{observed:?}");
        assert_eq!(observed.lifecycle, ComponentLifecycle::Stopped);
        assert!(!observed.lifecycle_requested);
        assert!(!observed.started);
        let snapshot = core.snapshot_computation_configuration().await.unwrap();
        assert_eq!(
            serde_json::to_value(
                crate::computation::configuration_from_snapshot(&snapshot).unwrap()
            )
            .unwrap(),
            serde_json::to_value(Some(graph_config)).unwrap()
        );
        core.start().await.unwrap();
        assert!(core.is_running().await);
        assert!(!handle.desired_snapshot().lifecycle_policies[&definition.id].auto_start);
        core.shutdown().await.unwrap();
        drop(handle);
        drop(server);

        let mut config = config;
        let crate::computation::ComputationConfigInput::Definition(graph) =
            config.instances[0].computation.as_mut().unwrap()
        else {
            panic!("expanded test definition");
        };
        graph.definition.components[0].lifecycle.auto_start = true;
        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        config.host = crate::api::models::ConfigValue::Static("127.0.0.1".into());
        config.port = crate::api::models::ConfigValue::Static(port);
        config.save_to_file(&path).unwrap();
        let failed =
            configured_test_server(path, port, directory.path().join("plugins"), false, false)
                .await
                .unwrap();
        let core = failed.instances[0].core.clone();
        let error = tokio::time::timeout(std::time::Duration::from_secs(10), failed.run())
            .await
            .expect("occupied API address must fail promptly")
            .unwrap_err();
        assert_eq!(
            error
                .chain()
                .find_map(|cause| cause.downcast_ref::<std::io::Error>())
                .expect("startup failure must retain its original IO cause")
                .kind(),
            std::io::ErrorKind::AddrInUse
        );
        assert!(!core.is_running().await);
        assert!(
            core.start().await.is_err(),
            "failed server startup must permanently shut down its owned core"
        );
    }

    #[tokio::test]
    async fn configured_instances_use_the_graph_without_selectors() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("server.yaml");
        let plugins = directory.path().join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        std::fs::write(
            &config_path,
            "instances:\n  - id: analytics\n  - id: monitoring\n",
        )
        .unwrap();

        let configured =
            configured_test_server(config_path.clone(), 8080, plugins.clone(), false, false)
                .await
                .unwrap();
        assert_eq!(configured.instances.len(), 2);
        for (instance, id) in configured.instances.iter().zip(["analytics", "monitoring"]) {
            assert_eq!(instance.core.get_current_config().await.unwrap().id, id);
            assert!(instance.core.query_factory_implementation().is_none());
            instance
                .core
                .register_query_factory(crate::query_test_support::factory())
                .unwrap();
            assert_graph_runtime(&instance.core).await;
        }
    }

    #[tokio::test]
    async fn prebuilt_instances_and_server_builder_use_the_graph() {
        let core = crate::query_test_support::builder().build().await.unwrap();
        let server = DrasiServer::from_core(core, false, false, "127.0.0.1".into(), 8080, None);
        assert_graph_runtime(&server.instances[0].core).await;

        let server = crate::DrasiServerBuilder::new()
            .with_query_factory(crate::query_test_support::factory())
            .with_id("first")
            .add_instance_builder(crate::query_test_support::builder().with_id("second"))
            .build()
            .await
            .unwrap();
        assert_eq!(server.instances.len(), 2);
        for (instance, id) in server.instances.iter().zip(["first", "second"]) {
            assert_eq!(instance.core.get_current_config().await.unwrap().id, id);
            assert_graph_runtime(&instance.core).await;
        }
    }

    async fn assert_graph_runtime(core: &DrasiLib) {
        core.add_query(
            drasi_lib::Query::cypher(drasi_lib::ComponentName::try_new("graph-node").unwrap())
                .query("MATCH (n) RETURN n")
                .auto_start(false)
                .build()
                .unwrap(),
        )
        .await
        .unwrap();
        let handle = core
            .component_by_name(&drasi_lib::ComponentName::try_new("graph-node").unwrap())
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), handle.wait_created())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            core.snapshot_configuration().await.unwrap().queries[0]
                .name
                .as_str(),
            "graph-node"
        );
        core.shutdown().await.unwrap();
    }
}
