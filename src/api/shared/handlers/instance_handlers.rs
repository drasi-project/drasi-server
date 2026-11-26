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

use axum::{extract::Extension, response::Json};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{persist_after_operation, wait_for_computation_creation};
use crate::api::models::ConfigValue;
use crate::api::models::{BootstrapProviderConfig, BootstrapProviderRef};
use crate::api::models::{ReactionConfig, SourceConfig};
use crate::api::shared::error::{error_codes, ErrorDetail, ErrorResponse};
use crate::api::shared::extractor::ConfigBody;
use crate::api::shared::responses::{ApiResponse, StatusResponse};
use crate::config::DrasiLibInstanceConfig;
use crate::factories::{create_reaction_locked, create_source_locked};
use crate::instance_paths::instance_storage_key;
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;
use drasi_lib::management::{OrdinaryComponent, OrdinaryComponentDefinition};
use drasi_lib::DrasiLib;

/// Request body for creating a new instance
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schema(as = CreateInstanceRequest)]
pub struct CreateInstanceRequest {
    /// Unique identifier for the new instance
    pub id: String,

    /// Whether to use persistent indexing (RocksDB). Default: false (in-memory)
    #[serde(default)]
    pub persist_index: Option<bool>,

    /// Whether persistent queries also maintain the archive index for past()
    /// functions. Default: false.
    #[serde(default)]
    pub enable_archive: Option<bool>,

    /// RocksDB memory budget for this instance, in MiB
    #[serde(default, rename = "memoryBudgetMiB")]
    #[schema(minimum = 1)]
    pub memory_budget_mib: Option<usize>,

    /// Default capacity for priority queues (cascades to queries/reactions)
    #[serde(default)]
    pub default_priority_queue_capacity: Option<usize>,

    /// Default capacity for dispatch buffers (cascades to queries/reactions)
    #[serde(default)]
    pub default_dispatch_buffer_capacity: Option<usize>,
    /// Per-component activation deadline in seconds (default: 300).
    #[serde(default)]
    #[schema(minimum = 1)]
    pub activation_timeout_secs: Option<u64>,
    /// Overall instance startup deadline in seconds (default: 600).
    #[serde(default)]
    #[schema(minimum = 1)]
    pub startup_timeout_secs: Option<u64>,
    /// Overall stop/shutdown deadline in seconds (default: 60).
    #[serde(default)]
    #[schema(minimum = 1)]
    pub shutdown_timeout_secs: Option<u64>,
    /// Per-component construction/configuration deadline in seconds (default: 5).
    #[serde(default)]
    #[schema(minimum = 1)]
    pub construction_timeout_secs: Option<u64>,
    /// Per-resource construction deadline in seconds (default: 30).
    #[serde(default)]
    #[schema(minimum = 1)]
    pub resource_construction_timeout_secs: Option<u64>,
    /// Per-owner cleanup deadline in seconds (default: 5).
    #[serde(default)]
    #[schema(minimum = 1)]
    pub cleanup_timeout_secs: Option<u64>,
    #[serde(default)]
    pub configuration_store: Option<crate::managed_configuration::ConfigurationStoreConfig>,
}

fn invalid_memory_budget_error(instance_id: &str, error: impl std::fmt::Display) -> ErrorResponse {
    ErrorResponse::new(
        error_codes::INVALID_REQUEST,
        "Invalid instance memory budget configuration",
    )
    .with_details(ErrorDetail {
        component_type: Some("instance".to_string()),
        component_id: Some(instance_id.to_string()),
        technical_details: Some(error.to_string()),
    })
}

/// Create a new DrasiLib instance
pub async fn create_instance(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(plugins): Extension<Arc<RwLock<PluginRegistry>>>,
    ConfigBody(request): ConfigBody<CreateInstanceRequest>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if let Some(cleanup) = registry.cleanup_handle() {
        let already_owned = crate::server::lifecycle::ManagementRequest::current()
            .is_some_and(|request| request.belongs_to(&cleanup));
        if !already_owned {
            return cleanup
                .start_management(move || {
                    create_instance_body(registry, read_only, config_persistence, plugins, request)
                })
                .map_err(ErrorResponse::from)?
                .finish()
                .await
                .map_err(ErrorResponse::from)?;
        }
    }
    create_instance_body(registry, read_only, config_persistence, plugins, request).await
}

fn instance_operation_error(instance: &str, message: &str, error: anyhow::Error) -> ErrorResponse {
    if error.chain().any(|cause| {
        cause.is::<crate::server::lifecycle::ServerShuttingDown>()
            || cause.is::<crate::server::lifecycle::RequestAbandoned>()
    }) {
        return ErrorResponse::from(error);
    }
    ErrorResponse::new(error_codes::INSTANCE_CREATE_FAILED, message).with_details(ErrorDetail {
        component_type: Some("instance".into()),
        component_id: Some(instance.into()),
        technical_details: Some(format!("{error:#}")),
    })
}

async fn retire_failed_instance(
    registry: &InstanceRegistry,
    core: &Arc<DrasiLib>,
    error: anyhow::Error,
) -> anyhow::Error {
    if let Some(cleanup) = registry.cleanup_handle() {
        match cleanup.retire_core(core).await {
            Ok(()) => error,
            Err(cleanup) => drasi_lib::error::OperationFailures::new(
                "Instance operation and cleanup failures",
                vec![error, cleanup],
            )
            .into(),
        }
    } else {
        crate::computation::cleanup_failed_preparation(vec![core.as_ref().clone()], error).await
    }
}

async fn create_instance_body(
    registry: InstanceRegistry,
    read_only: Arc<bool>,
    config_persistence: Option<Arc<ConfigPersistence>>,
    plugins: Arc<RwLock<PluginRegistry>>,
    request: CreateInstanceRequest,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create instances.",
        ));
    }

    let instance_id = request.id.clone();
    registry
        .validate_new_instance_id(&instance_id)
        .map_err(|error| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Instance ID is not valid while telemetry is enabled; choose an ID without whitespace or control characters, or disable telemetry (for example telemetry.profile: off)",
            )
            .with_details(ErrorDetail {
                component_type: Some("instance".to_string()),
                component_id: Some(instance_id.clone()),
                technical_details: Some(error.to_string()),
            })
        })?;
    let persist_index = request.persist_index.unwrap_or(false);
    let enable_archive = request.enable_archive.unwrap_or(false);
    let memory_budget_mib = request.memory_budget_mib;

    let memory_budget_bytes =
        crate::index_provider::memory_budget_bytes(persist_index, memory_budget_mib)
            .map_err(|error| invalid_memory_budget_error(&instance_id, error))?;

    // Check if instance already exists
    if registry.contains(&instance_id).await {
        log::info!("Instance '{instance_id}' already exists");
        return Err(ErrorResponse::new(
            error_codes::DUPLICATE_RESOURCE,
            "Resource already exists",
        ));
    }

    // Create a new DrasiLib instance with optional configuration
    let mut builder = DrasiLib::builder()
        .with_id(&instance_id)
        .with_allow_external_component_ids(true);
    builder = registry.bind_builder(builder).map_err(|error| {
        instance_operation_error(&instance_id, "Instance runtime binding failed", error)
    })?;

    if let Some(capacity) = request.default_priority_queue_capacity {
        builder = builder.with_priority_queue_capacity(capacity);
    }

    if let Some(capacity) = request.default_dispatch_buffer_capacity {
        builder = builder.with_dispatch_buffer_capacity(capacity);
    }
    for (name, value) in [
        ("activationTimeoutSecs", request.activation_timeout_secs),
        ("startupTimeoutSecs", request.startup_timeout_secs),
        ("shutdownTimeoutSecs", request.shutdown_timeout_secs),
        ("constructionTimeoutSecs", request.construction_timeout_secs),
        (
            "resourceConstructionTimeoutSecs",
            request.resource_construction_timeout_secs,
        ),
        ("cleanupTimeoutSecs", request.cleanup_timeout_secs),
    ] {
        if let Some(seconds) = value {
            if seconds == 0
                || std::time::Instant::now()
                    .checked_add(std::time::Duration::from_secs(seconds))
                    .is_none()
            {
                return Err(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    format!("{name} must be positive and representable"),
                ));
            }
        }
    }
    if let Some(seconds) = request.activation_timeout_secs {
        builder = builder.with_activation_timeout_secs(seconds);
    }
    if let Some(seconds) = request.startup_timeout_secs {
        builder = builder.with_startup_timeout_secs(seconds);
    }
    if let Some(seconds) = request.shutdown_timeout_secs {
        builder = builder.with_shutdown_timeout_secs(seconds);
    }
    if let Some(seconds) = request.construction_timeout_secs {
        builder = builder.with_construction_timeout_secs(seconds);
    }
    if let Some(seconds) = request.resource_construction_timeout_secs {
        builder = builder.with_resource_construction_timeout_secs(seconds);
    }
    if let Some(seconds) = request.cleanup_timeout_secs {
        builder = builder.with_cleanup_timeout_secs(seconds);
    }

    // Filesystem-safe key shared by the persistent index and WAL paths.
    let safe_id = instance_storage_key(&instance_id);

    // Register the persistent RocksDB index provider as the instance default
    // when requested.
    if persist_index {
        builder = crate::index_provider::apply_rocksdb_index(
            builder,
            &instance_id,
            enable_archive,
            memory_budget_bytes,
        )
        .map_err(|error| invalid_memory_budget_error(&instance_id, error))?;
    }

    // WAL provider for durable source event persistence
    {
        let wal_path = PathBuf::from(format!("./data/{safe_id}/wal"));
        log::info!(
            "Enabling WAL provider for instance '{}' at: {}",
            instance_id,
            wal_path.display()
        );
        let wal_provider = Arc::new(drasi_wal_redb::RedbWalProvider::new(&wal_path));
        builder = builder.with_wal_provider(wal_provider);
    }

    if let Some(store) = &request.configuration_store {
        builder = registry
            .configuration_stores()
            .configure(builder, &instance_id, store, None, plugins, None)
            .await
            .map_err(|error| {
                log::error!(
                    "Configuration store preparation failed for '{instance_id}': {error:#}"
                );
                instance_operation_error(
                    &instance_id,
                    "Configuration store preparation failed",
                    error,
                )
            })?;
    } else {
        // No declared components yet, so there is nothing to seed.
        builder = crate::managed_configuration::configure_in_memory(
            builder,
            &instance_id,
            Default::default(),
            Default::default(),
            plugins,
            None,
        )
        .await
        .map_err(|error| {
            log::error!("Component preparation failed for '{instance_id}': {error:#}");
            ErrorResponse::new(
                error_codes::INSTANCE_CREATE_FAILED,
                "Component preparation failed",
            )
        })?
        .0;
    }
    let core = builder.build().await.map_err(|e| {
        log::error!("Failed to create instance: {e}");
        instance_operation_error(&instance_id, "Instance construction failed", e.into())
    })?;

    let core = Arc::new(core);
    if let Some(cleanup) = registry.cleanup_handle() {
        cleanup.retain_core(core.clone()).map_err(|error| {
            instance_operation_error(
                &instance_id,
                "Instance ownership registration failed",
                error,
            )
        })?;
    }

    // Start the instance
    if let Err(e) = crate::managed_configuration::start_instance(&core).await {
        let e = retire_failed_instance(&registry, &core, e).await;
        log::error!("Failed to start instance '{instance_id}': {e}");
        return Err(instance_operation_error(
            &instance_id,
            "Instance startup failed",
            e,
        ));
    }

    // Add to registry
    if let Err(e) = registry
        .publish_instance(instance_id.clone(), core.clone())
        .await
    {
        let e = retire_failed_instance(&registry, &core, e).await;
        log::error!("Failed to register instance: {e}");
        return Err(instance_operation_error(
            &instance_id,
            "Instance publication failed",
            e,
        ));
    }

    log::info!("Instance '{instance_id}' created successfully");

    // Persist configuration if enabled
    if let Some(persistence) = &config_persistence {
        let instance_config = DrasiLibInstanceConfig {
            id: ConfigValue::Static(instance_id.clone()).into(),
            persist_index,
            enable_archive,
            memory_budget_mib: memory_budget_mib.map(ConfigValue::Static),
            state_store: None,
            secret_store: None,
            default_priority_queue_capacity: request
                .default_priority_queue_capacity
                .map(ConfigValue::Static),
            default_dispatch_buffer_capacity: request
                .default_dispatch_buffer_capacity
                .map(ConfigValue::Static),
            activation_timeout_secs: request.activation_timeout_secs.map(ConfigValue::Static),
            startup_timeout_secs: request.startup_timeout_secs.map(ConfigValue::Static),
            shutdown_timeout_secs: request.shutdown_timeout_secs.map(ConfigValue::Static),
            construction_timeout_secs: request.construction_timeout_secs.map(ConfigValue::Static),
            resource_construction_timeout_secs: request
                .resource_construction_timeout_secs
                .map(ConfigValue::Static),
            cleanup_timeout_secs: request.cleanup_timeout_secs.map(ConfigValue::Static),
            sources: Vec::new(),
            reactions: Vec::new(),
            queries: Vec::new(),
            identity_providers: Vec::new(),
            bootstrap_providers: Vec::new(),
            computation: None,
            configuration_store: request.configuration_store,
        };
        persistence
            .register_instance(instance_config)
            .await
            .map_err(|error| super::persistence_error("registering instance settings", error))?;
        persist_after_operation(&Some(persistence.clone()), "creating instance").await?;
    }

    Ok(Json(ApiResponse::success(StatusResponse {
        message: format!("Instance '{instance_id}' created successfully"),
        configuration: None,
    })))
}

// =============================================================================
// Instance Clone
// =============================================================================

/// Request body for cloning an instance's configuration into another instance.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schema(as = CloneInstanceRequest)]
pub struct CloneInstanceRequest {
    /// ID of the instance whose configuration will be copied
    pub source_instance_id: String,
}

/// Response body for a clone operation.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CloneInstanceResponse)]
pub struct CloneInstanceResponse {
    /// Whether the clone completed without errors
    pub success: bool,
    /// IDs of sources created in the target instance
    pub sources_created: Vec<String>,
    /// IDs of queries created in the target instance
    pub queries_created: Vec<String>,
    /// IDs of reactions created in the target instance
    pub reactions_created: Vec<String>,
    /// IDs of native components created in the target instance.
    pub components_created: Vec<String>,
    /// Any errors encountered during the clone
    pub errors: Vec<String>,
}

/// Clone an instance's configuration into an existing target instance.
///
/// Takes an atomic snapshot of the source instance and recreates all
/// components (sources, queries, reactions) in the target instance.
/// All cloned components have auto-start disabled.
/// Added nodes are retained, with creation failures reported in the response.
pub async fn clone_instance(
    registry: InstanceRegistry,
    read_only: Arc<bool>,
    plugin_registry: Arc<RwLock<PluginRegistry>>,
    config_persistence: Option<Arc<ConfigPersistence>>,
    target_instance_id: &str,
    source_instance_id: &str,
) -> Result<Json<ApiResponse<CloneInstanceResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot clone instances.",
        ));
    }

    // Get source instance and take snapshot
    let source_core = registry.get(source_instance_id).await.ok_or_else(|| {
        ErrorResponse::new(
            error_codes::INSTANCE_NOT_FOUND,
            format!("Source instance '{source_instance_id}' not found"),
        )
    })?;

    let snapshot = source_core
        .snapshot_computation_configuration()
        .await
        .map_err(|e| {
            ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                "Failed to capture source instance configuration",
            )
            .with_details(ErrorDetail {
                component_type: Some("instance".to_string()),
                component_id: Some(source_instance_id.to_string()),
                technical_details: Some(e.to_string()),
            })
        })?;
    // Reject unsupported bindings before adding any ordinary or native nodes.
    let mut computation =
        crate::computation::configuration_from_snapshot(&snapshot).map_err(|error| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Source instance contains native bindings that cannot be cloned",
            )
            .with_details(ErrorDetail {
                component_type: Some("instance".to_string()),
                component_id: Some(source_instance_id.to_string()),
                technical_details: Some(format!("{error:#}")),
            })
        })?;
    if let Some(computation) = &mut computation {
        for component in &mut computation.definition.components {
            component.lifecycle.auto_start = false;
        }
    }
    let snapshot = snapshot.instance;

    // Get target instance (must already exist)
    let target_core = registry.get(target_instance_id).await.ok_or_else(|| {
        ErrorResponse::new(
            error_codes::INSTANCE_NOT_FOUND,
            format!("Target instance '{target_instance_id}' not found"),
        )
    })?;
    if target_core.constructs_ordinary_components() {
        if computation.is_some() && target_core.configuration_is_persistent() {
            return Err(ErrorResponse::new(
                error_codes::CONFIGURATION_TRANSITION_REQUIRED,
                "Native components cannot be cloned into a persisted instance; submit them to computation/desired",
            ));
        }
        let definitions = if source_core.constructs_ordinary_components() {
            persisted_definitions(&source_core)
        } else {
            snapshot_definitions(&snapshot)
        }
        .map_err(|error| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Source instance configuration cannot be cloned",
            )
            .with_details(ErrorDetail {
                component_type: Some("instance".to_string()),
                component_id: Some(source_instance_id.to_string()),
                technical_details: Some(format!("{error:#}")),
            })
        })?;
        if definitions
            .iter()
            .any(|definition| matches!(definition.component, OrdinaryComponent::Query { .. }))
        {
            crate::query_plugin::ensure_registered(&target_core, &plugin_registry)
                .await
                .map_err(|error| {
                    ErrorResponse::new(error_codes::PLUGIN_NOT_FOUND, format!("{error:#}"))
                })?;
        }
        let mut response = CloneInstanceResponse {
            success: true,
            sources_created: Vec::new(),
            queries_created: Vec::new(),
            reactions_created: Vec::new(),
            components_created: Vec::new(),
            errors: Vec::new(),
        };
        for definition in &definitions {
            let name = definition.name.to_string();
            match definition.component {
                OrdinaryComponent::Source { .. } => response.sources_created.push(name),
                OrdinaryComponent::Query { .. } => response.queries_created.push(name),
                _ => response.reactions_created.push(name),
            }
        }
        if !definitions.is_empty() {
            let status = crate::api::shared::managed_components::add_components(
                &target_core,
                &config_persistence,
                "clone",
                definitions,
            )
            .await?;
            if let Some(error) = status
                .configuration
                .and_then(|change| change.write_back_error)
            {
                response
                    .errors
                    .push(format!("Configuration file write-back failed: {error}"));
                response.success = false;
            }
        }
        if let Some(config) = computation {
            let registry = plugin_registry.read().await;
            let registration =
                crate::computation::register_components(&config, &target_core, &registry);
            drop(registry);
            match registration.await {
                Ok(_) => response.components_created.extend(
                    config
                        .definition
                        .components
                        .iter()
                        .map(|component| component.descriptor.id().to_string()),
                ),
                Err(error) => {
                    log::error!("Clone: failed to add native components: {error:#}");
                    response
                        .errors
                        .push(format!("Failed to add native components: {error:#}"));
                    response.success = false;
                }
            }
            persist_after_operation(&config_persistence, "cloning instance").await?;
        }
        return Ok(Json(ApiResponse::success(response)));
    }

    let mut sources_created: Vec<String> = Vec::new();
    let mut queries_created: Vec<String> = Vec::new();
    let mut reactions_created: Vec<String> = Vec::new();
    let mut components_created = Vec::new();
    let mut errors = Vec::new();
    let source_names_by_id: std::collections::HashMap<_, _> = snapshot
        .sources
        .iter()
        .map(|source| (source.id.clone(), source.name.clone()))
        .collect();
    let query_names_by_id: std::collections::HashMap<_, _> = snapshot
        .queries
        .iter()
        .map(|query| (query.id.clone(), query.name.clone()))
        .collect();

    // Phase 1: Create sources
    for src_snap in &snapshot.sources {
        // Skip internal sources
        if src_snap.name.as_str().starts_with("__") {
            continue;
        }

        let properties_json = serde_json::to_value(&src_snap.properties)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let bootstrap_provider = src_snap.bootstrap_provider.as_ref().map(|bp| {
            let bp_config_json = serde_json::to_value(&bp.properties)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            BootstrapProviderRef::Inline(BootstrapProviderConfig {
                kind: bp.kind.clone(),
                config: bp_config_json,
            })
        });

        let source_config = SourceConfig {
            component_id: None,
            kind: src_snap.source_type.clone(),
            id: src_snap.name.to_string(),
            auto_start: false,
            bootstrap_provider,
            identity_provider: None,
            config: properties_json,
        };

        let (source, plugin_meta) = match create_source_locked(
            &plugin_registry,
            src_snap.id.clone(),
            source_config.clone(),
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                log::error!("Clone: failed to create source '{}': {e}", src_snap.id);
                errors.push(format!("Failed to create source '{}': {e}", src_snap.id));
                continue;
            }
        };

        if let Err(e) = target_core
            .add_source_with_metadata(src_snap.name.clone(), source, plugin_meta)
            .await
        {
            log::error!("Clone: failed to add source '{}': {e}", src_snap.id);
            errors.push(format!("Failed to add source '{}': {e}", src_snap.id));
            continue;
        }

        // Track the cloned source's bootstrap provider so it survives the next
        // save(). Without this the source_bootstrap_provider map has no entry
        // and save() would fall back to the unreliable snapshot reconstruction
        // path, silently dropping the provider (issue #105).
        if let Some(p) = &config_persistence {
            p.register_source_bootstrap_provider(
                target_instance_id,
                src_snap.name.as_str(),
                source_config.bootstrap_provider(),
            )
            .await;
        }

        sources_created.push(src_snap.name.to_string());
        let source_name = src_snap.name.clone();
        if let Err(e) = wait_for_computation_creation(&target_core, "source", &source_name).await {
            log::warn!(
                "Clone: source '{}' was added but creation failed: {e}",
                src_snap.id
            );
            errors.push(format!(
                "Source '{}' was added but creation failed: {e}",
                src_snap.id
            ));
        }
    }

    // Phase 2: Create queries
    for q_snap in &snapshot.queries {
        if q_snap.name.as_str().starts_with("__") {
            continue;
        }

        let mut query_config = q_snap.config.clone().into_authoring();
        query_config.auto_start = false;
        for input in &mut query_config.inputs {
            if let Some(id) = input.component.as_id() {
                if let Some(name) = source_names_by_id.get(id) {
                    input.component = drasi_lib::config::ComponentReference::by_name(name.clone());
                }
            }
        }

        if let Err(e) = target_core.add_query(query_config).await {
            log::error!("Clone: failed to add query '{}': {e}", q_snap.id);
            errors.push(format!("Failed to add query '{}': {e}", q_snap.id));
            continue;
        }

        queries_created.push(q_snap.name.to_string());
        let query_name = q_snap.name.clone();
        if let Err(e) = wait_for_computation_creation(&target_core, "query", &query_name).await {
            log::warn!(
                "Clone: query '{}' was added but creation failed: {e}",
                q_snap.id
            );
            errors.push(format!(
                "Query '{}' was added but creation failed: {e}",
                q_snap.id
            ));
        }
    }

    // Phase 3: Create reactions
    for rx_snap in &snapshot.reactions {
        if rx_snap.name.as_str().starts_with("__") {
            continue;
        }

        let properties_json = serde_json::to_value(&rx_snap.properties)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let queries = rx_snap
            .queries
            .iter()
            .map(|query| {
                query
                    .as_id()
                    .and_then(|id| query_names_by_id.get(id))
                    .map(|name| drasi_lib::config::ComponentReference::by_name(name.clone()))
                    .unwrap_or_else(|| query.clone())
            })
            .collect();
        let reaction_config = ReactionConfig {
            component_id: None,
            kind: rx_snap.reaction_type.clone(),
            id: rx_snap.name.to_string(),
            queries,
            auto_start: false,
            identity_provider: None,
            config: properties_json,
        };

        let (reaction, plugin_meta) = match create_reaction_locked(
            &plugin_registry,
            rx_snap.id.clone(),
            reaction_config.clone(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                log::error!("Clone: failed to create reaction '{}': {e}", rx_snap.id);
                errors.push(format!("Failed to create reaction '{}': {e}", rx_snap.id));
                continue;
            }
        };

        if let Err(e) = target_core
            .add_reaction_with_metadata(rx_snap.name.clone(), reaction, plugin_meta)
            .await
        {
            log::error!("Clone: failed to add reaction '{}': {e}", rx_snap.id);
            errors.push(format!("Failed to add reaction '{}': {e}", rx_snap.id));
            continue;
        }

        reactions_created.push(rx_snap.name.to_string());
        let reaction_name = rx_snap.name.clone();
        if let Err(e) =
            wait_for_computation_creation(&target_core, "reaction", &reaction_name).await
        {
            log::warn!(
                "Clone: reaction '{}' was added but creation failed: {e}",
                rx_snap.id
            );
            errors.push(format!(
                "Reaction '{}' was added but creation failed: {e}",
                rx_snap.id
            ));
        }
    }

    if let Some(config) = computation {
        let registry = plugin_registry.read().await;
        let registration =
            crate::computation::register_components(&config, &target_core, &registry);
        drop(registry);
        match registration.await {
            Ok(_) => {
                components_created.extend(
                    config
                        .definition
                        .components
                        .iter()
                        .map(|component| component.descriptor.id().to_string()),
                );
            }
            Err(error) => {
                log::error!("Clone: failed to add native components: {error:#}");
                errors.push(format!("Failed to add native components: {error:#}"));
            }
        }
    }
    persist_after_operation(&config_persistence, "cloning instance").await?;

    log::info!(
        "Clone complete: {} sources, {} queries, {} reactions cloned from '{}' to '{}'",
        sources_created.len(),
        queries_created.len(),
        reactions_created.len(),
        source_instance_id,
        target_instance_id,
    );

    Ok(Json(ApiResponse::success(CloneInstanceResponse {
        success: errors.is_empty(),
        sources_created,
        queries_created,
        reactions_created,
        components_created,
        errors,
    })))
}

/// Stopped copies of a persisted instance's stored ordinary recipes. Stored
/// recipes keep secret references unresolved; references become names.
fn persisted_definitions(core: &DrasiLib) -> anyhow::Result<Vec<OrdinaryComponentDefinition>> {
    use crate::ordinary_resolver::{component_names, named_reference};
    let desired = core.desired_configuration()?.desired;
    let names = component_names(&desired);
    desired
        .ordinary
        .iter()
        .map(|record| {
            let component = match &record.component {
                OrdinaryComponent::Query { .. } => {
                    let mut query = record.query_configuration()?.into_authoring();
                    query.id = None;
                    query.auto_start = false;
                    for input in &mut query.inputs {
                        input.component = named_reference(&names, input.component.clone());
                    }
                    return OrdinaryComponentDefinition::query(query);
                }
                OrdinaryComponent::Reaction {
                    kind,
                    configuration,
                    inputs,
                } => OrdinaryComponent::Reaction {
                    kind: kind.clone(),
                    configuration: configuration.clone(),
                    inputs: inputs
                        .iter()
                        .cloned()
                        .map(|input| named_reference(&names, input))
                        .collect(),
                },
                OrdinaryComponent::ProcessLocal { .. } => {
                    anyhow::bail!("process-local component '{}' cannot be cloned", record.name)
                }
                component => component.clone(),
            };
            Ok(OrdinaryComponentDefinition {
                id: None,
                name: record.name.clone(),
                auto_start: false,
                component,
            })
        })
        .collect()
}

/// Stopped ordinary definitions reconstructed from an unmanaged instance's snapshot.
fn snapshot_definitions(
    snapshot: &drasi_lib::ConfigurationSnapshot,
) -> anyhow::Result<Vec<OrdinaryComponentDefinition>> {
    use crate::ordinary_resolver::{reaction_definition, source_definition};
    let names: std::collections::BTreeMap<_, _> = snapshot
        .sources
        .iter()
        .map(|source| (source.id.clone(), source.name.clone()))
        .chain(
            snapshot
                .queries
                .iter()
                .map(|query| (query.id.clone(), query.name.clone())),
        )
        .collect();
    let named = |reference: drasi_lib::config::ComponentReference| {
        crate::ordinary_resolver::named_reference(&names, reference)
    };
    let mut definitions = Vec::new();
    for source in snapshot
        .sources
        .iter()
        .filter(|s| !s.name.as_str().starts_with("__"))
    {
        let config = SourceConfig {
            component_id: None,
            kind: source.source_type.clone(),
            id: source.name.to_string(),
            auto_start: false,
            bootstrap_provider: source.bootstrap_provider.as_ref().map(|provider| {
                BootstrapProviderRef::Inline(BootstrapProviderConfig {
                    kind: provider.kind.clone(),
                    config: serde_json::to_value(&provider.properties)
                        .unwrap_or_else(|_| serde_json::Value::Object(Default::default())),
                })
            }),
            identity_provider: None,
            config: serde_json::to_value(&source.properties)?,
        };
        definitions.push(source_definition(&config)?);
    }
    for query in snapshot
        .queries
        .iter()
        .filter(|q| !q.name.as_str().starts_with("__"))
    {
        let mut config = query.config.clone().into_authoring();
        config.id = None;
        config.auto_start = false;
        for input in &mut config.inputs {
            input.component = named(input.component.clone());
        }
        definitions.push(OrdinaryComponentDefinition::query(config)?);
    }
    for reaction in snapshot
        .reactions
        .iter()
        .filter(|r| !r.name.as_str().starts_with("__"))
    {
        let config = ReactionConfig {
            component_id: None,
            kind: reaction.reaction_type.clone(),
            id: reaction.name.to_string(),
            queries: reaction.queries.iter().cloned().map(named).collect(),
            auto_start: false,
            identity_provider: None,
            config: serde_json::to_value(&reaction.properties)?,
        };
        definitions.push(reaction_definition(&config)?);
    }
    Ok(definitions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use drasi_lib::observability::{
        ComponentLogRuntime, LogPolicy, TelemetryProfile, TelemetrySettings,
    };
    use serde_json::json;

    #[tokio::test]
    async fn create_rejects_invalid_ids_while_telemetry_is_enabled() -> anyhow::Result<()> {
        let logging = ComponentLogRuntime::new(LogPolicy::new("info")?)?;
        let registry = InstanceRegistry::new().with_runtime_context(
            crate::TelemetryBindings::new(
                TelemetrySettings::new(TelemetryProfile::Basic).with_metrics(false),
                logging.clone(),
            ),
            crate::ServerCleanupHandle::bootstrap(),
        );
        let error = create_instance_body(
            registry,
            Arc::new(false),
            None,
            Arc::new(RwLock::new(PluginRegistry::new())),
            serde_json::from_value(json!({"id": "my server"}))?,
        )
        .await
        .err()
        .expect("a telemetry-invalid instance ID must be rejected");
        assert_eq!(error.code, error_codes::INVALID_REQUEST);
        let details = error.details.as_ref().expect("error details");
        assert_eq!(details.component_id.as_deref(), Some("my server"));
        assert!(details
            .technical_details
            .as_deref()
            .is_some_and(|technical| technical.contains("telemetry.profile: off")));
        assert!(!error.message.contains("my server"));
        assert_eq!(error.with_status().0, StatusCode::BAD_REQUEST);
        logging.shutdown(std::time::Duration::from_secs(5)).await?;
        Ok(())
    }
}
