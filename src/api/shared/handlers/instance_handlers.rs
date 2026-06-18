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

use super::persist_after_operation;
use crate::api::models::BootstrapProviderConfig;
use crate::api::models::ConfigValue;
use crate::api::shared::error::{error_codes, ConfigBody, ErrorDetail, ErrorResponse};
use crate::api::shared::responses::{ApiResponse, StatusResponse};
use crate::config::{DrasiLibInstanceConfig, ReactionConfig, SourceConfig};
use crate::factories::{create_reaction_locked, create_source_locked};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;
use drasi_lib::{ConfigurationSnapshot, DrasiLib};

/// Request body for creating a new instance
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreateInstanceRequest)]
pub struct CreateInstanceRequest {
    /// Unique identifier for the new instance
    pub id: String,

    /// Whether to use persistent indexing (RocksDB). Default: false (in-memory)
    #[serde(default)]
    pub persist_index: Option<bool>,

    /// Default capacity for priority queues (cascades to queries/reactions)
    #[serde(default)]
    pub default_priority_queue_capacity: Option<usize>,

    /// Default capacity for dispatch buffers (cascades to queries/reactions)
    #[serde(default)]
    pub default_dispatch_buffer_capacity: Option<usize>,
}

/// Create a new DrasiLib instance
pub async fn create_instance(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    ConfigBody(request): ConfigBody<CreateInstanceRequest>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create instances.",
        ));
    }

    let instance_id = request.id.clone();
    let persist_index = request.persist_index.unwrap_or(false);

    // Check if instance already exists
    if registry.contains(&instance_id).await {
        log::info!("Instance '{instance_id}' already exists");
        return Err(ErrorResponse::new(
            error_codes::DUPLICATE_RESOURCE,
            "Resource already exists",
        ));
    }

    // Create a new DrasiLib instance with optional configuration
    let mut builder = DrasiLib::builder().with_id(&instance_id);

    if let Some(capacity) = request.default_priority_queue_capacity {
        builder = builder.with_priority_queue_capacity(capacity);
    }

    if let Some(capacity) = request.default_dispatch_buffer_capacity {
        builder = builder.with_dispatch_buffer_capacity(capacity);
    }

    // Set up RocksDB persistent indexing if requested
    if persist_index {
        let safe_id = instance_id.replace(['/', '\\'], "_").replace("..", "_");
        let index_path = PathBuf::from(format!("./data/{safe_id}/index"));
        log::info!(
            "Enabling persistent indexing for instance '{}' with RocksDB at: {}",
            instance_id,
            index_path.display()
        );
        let rocksdb_provider = drasi_index_rocksdb::RocksDbIndexProvider::new(
            index_path, true,  // enable_archive - support for past() function
            false, // direct_io - use OS page cache
        );
        builder = builder.with_index_provider(Arc::new(rocksdb_provider));
    }

    let core = builder.build().await.map_err(|e| {
        log::error!("Failed to create instance: {e}");
        ErrorResponse::new(
            error_codes::INSTANCE_CREATE_FAILED,
            format!("Failed to create instance: {e}"),
        )
    })?;

    let core = Arc::new(core);

    // Start the instance
    if let Err(e) = core.start().await {
        log::error!("Failed to start instance '{instance_id}': {e}");
        return Err(ErrorResponse::new(
            error_codes::INSTANCE_CREATE_FAILED,
            format!("Failed to start instance: {e}"),
        ));
    }

    // Add to registry
    if let Err(e) = registry.add(instance_id.clone(), core).await {
        log::error!("Failed to register instance: {e}");
        return Err(ErrorResponse::new(error_codes::INSTANCE_CREATE_FAILED, e));
    }

    log::info!("Instance '{instance_id}' created successfully");

    // Persist configuration if enabled
    if let Some(persistence) = &config_persistence {
        let instance_config = DrasiLibInstanceConfig {
            id: ConfigValue::Static(instance_id.clone()),
            persist_index,
            state_store: None,
            secret_store: None,
            default_priority_queue_capacity: request
                .default_priority_queue_capacity
                .map(ConfigValue::Static),
            default_dispatch_buffer_capacity: request
                .default_dispatch_buffer_capacity
                .map(ConfigValue::Static),
            sources: Vec::new(),
            reactions: Vec::new(),
            queries: Vec::new(),
            identity_providers: Vec::new(),
        };
        persistence.register_instance(instance_config).await;
        persist_after_operation(&Some(persistence.clone()), "creating instance").await?;
    }

    Ok(Json(ApiResponse::success(StatusResponse {
        message: format!("Instance '{instance_id}' created successfully"),
    })))
}

// =============================================================================
// Instance Clone
// =============================================================================

/// Request body for cloning an instance's configuration into another instance.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
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
    /// Any errors encountered during the clone
    pub errors: Vec<String>,
}

/// Clone an instance's configuration into an existing target instance.
///
/// Takes an atomic snapshot of the source instance and recreates all
/// components (sources, queries, reactions) in the target instance.
/// All cloned components are created in the stopped state.
/// On failure, already-created components are rolled back.
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

    let snapshot: ConfigurationSnapshot =
        source_core.snapshot_configuration().await.map_err(|e| {
            ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to capture snapshot of source instance: {e}"),
            )
        })?;

    // Get target instance (must already exist)
    let target_core = registry.get(target_instance_id).await.ok_or_else(|| {
        ErrorResponse::new(
            error_codes::INSTANCE_NOT_FOUND,
            format!("Target instance '{target_instance_id}' not found"),
        )
    })?;

    let mut sources_created: Vec<String> = Vec::new();
    let mut queries_created: Vec<String> = Vec::new();
    let mut reactions_created: Vec<String> = Vec::new();

    // Phase 1: Create sources
    for src_snap in &snapshot.sources {
        // Skip internal sources
        if src_snap.id.starts_with("__") {
            continue;
        }

        let properties_json = serde_json::to_value(&src_snap.properties)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let bootstrap_provider = src_snap.bootstrap_provider.as_ref().map(|bp| {
            let bp_config_json = serde_json::to_value(&bp.properties)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            BootstrapProviderConfig {
                kind: bp.kind.clone(),
                config: bp_config_json,
            }
        });

        let source_config = SourceConfig {
            kind: src_snap.source_type.clone(),
            id: src_snap.id.clone(),
            auto_start: false,
            bootstrap_provider,
            identity_provider: None,
            config: properties_json,
        };

        let (source, plugin_meta) =
            match create_source_locked(&plugin_registry, source_config.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Clone: failed to create source '{}': {e}", src_snap.id);
                    let rb = rollback_sources(&target_core, &sources_created).await;
                    return Err(clone_error(
                        error_codes::SOURCE_CREATE_FAILED,
                        format!("Failed to create source '{}': {e}", src_snap.id),
                        "source",
                        &src_snap.id,
                        rb,
                    ));
                }
            };

        if let Err(e) = target_core
            .add_source_with_metadata(source, plugin_meta)
            .await
        {
            log::error!("Clone: failed to add source '{}': {e}", src_snap.id);
            let rb = rollback_sources(&target_core, &sources_created).await;
            return Err(clone_error(
                error_codes::SOURCE_CREATE_FAILED,
                format!("Failed to add source '{}': {e}", src_snap.id),
                "source",
                &src_snap.id,
                rb,
            ));
        }

        sources_created.push(src_snap.id.clone());
    }

    // Phase 2: Create queries
    for q_snap in &snapshot.queries {
        if q_snap.id.starts_with("__") {
            continue;
        }

        let mut query_config = q_snap.config.clone();
        query_config.auto_start = false;

        if let Err(e) = target_core.add_query(query_config).await {
            log::error!("Clone: failed to add query '{}': {e}", q_snap.id);
            let mut rb = rollback_queries(&target_core, &queries_created).await;
            rb.extend(rollback_sources(&target_core, &sources_created).await);
            return Err(clone_error(
                error_codes::QUERY_CREATE_FAILED,
                format!("Failed to add query '{}': {e}", q_snap.id),
                "query",
                &q_snap.id,
                rb,
            ));
        }

        queries_created.push(q_snap.id.clone());
    }

    // Phase 3: Create reactions
    for rx_snap in &snapshot.reactions {
        if rx_snap.id.starts_with("__") {
            continue;
        }

        let properties_json = serde_json::to_value(&rx_snap.properties)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let reaction_config = ReactionConfig {
            kind: rx_snap.reaction_type.clone(),
            id: rx_snap.id.clone(),
            queries: rx_snap.queries.clone(),
            auto_start: false,
            identity_provider: None,
            config: properties_json,
        };

        let (reaction, plugin_meta) =
            match create_reaction_locked(&plugin_registry, reaction_config.clone()).await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Clone: failed to create reaction '{}': {e}", rx_snap.id);
                    let mut rb = rollback_reactions(&target_core, &reactions_created).await;
                    rb.extend(rollback_queries(&target_core, &queries_created).await);
                    rb.extend(rollback_sources(&target_core, &sources_created).await);
                    return Err(clone_error(
                        error_codes::REACTION_CREATE_FAILED,
                        format!("Failed to create reaction '{}': {e}", rx_snap.id),
                        "reaction",
                        &rx_snap.id,
                        rb,
                    ));
                }
            };

        if let Err(e) = target_core
            .add_reaction_with_metadata(reaction, plugin_meta)
            .await
        {
            log::error!("Clone: failed to add reaction '{}': {e}", rx_snap.id);
            let mut rb = rollback_reactions(&target_core, &reactions_created).await;
            rb.extend(rollback_queries(&target_core, &queries_created).await);
            rb.extend(rollback_sources(&target_core, &sources_created).await);
            return Err(clone_error(
                error_codes::REACTION_CREATE_FAILED,
                format!("Failed to add reaction '{}': {e}", rx_snap.id),
                "reaction",
                &rx_snap.id,
                rb,
            ));
        }

        reactions_created.push(rx_snap.id.clone());
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
        success: true,
        sources_created,
        queries_created,
        reactions_created,
        errors: Vec::new(),
    })))
}

// =============================================================================
// Clone rollback helpers
// =============================================================================
//
// These helpers are intentionally **best-effort**: each call to
// `DrasiLib::remove_*` is attempted in sequence and any error is recorded
// but does not stop the loop. This is safe at this call site for the
// following reasons:
//
// 1. **All cloned components are stopped**. `clone_instance` forces
//    `auto_start = false` for every cloned source/query/reaction (see
//    Phases 1–3 above), so rollback is removing components that were
//    never started.
// 2. **Removal happens in dependent-first order** (reactions → queries →
//    sources). This means each component being removed has already had
//    its dependents removed in a prior helper call (or never had any),
//    so `DrasiLib`'s internal `can_remove` dependent check should not
//    reject the removal.
// 3. **Teardown of a never-started component is essentially a map
//    removal**. With `cleanup = false`, no provider-level
//    deprovisioning runs; the component is simply unregistered from the
//    runtime map.
// 4. **Graph deregister failures are absorbed inside `remove_*`**
//    (the entry is marked `Error` and `Ok(())` is returned), so most
//    realistic failure modes never reach the caller.
//
// The realistic failure surface is therefore (a) concurrent mutation
// races (a sibling request creating a dependent between phase 1 and
// rollback — narrow given the lack of an instance-wide lock) or
// (b) bugs/panics in `drasi-lib` internals. In either case, refusing
// to roll back or retrying would not be safer than logging and
// continuing — the alternative is leaving the user in a state where
// some components were rolled back and others were not, with no
// record of which is which.
//
// Each helper returns a `Vec<String>` of human-readable rollback
// failures (empty on the happy path). The caller threads these into
// the outgoing `ErrorResponse.details.technical_details` so an
// operator who hits the rare race can identify any orphans for
// manual cleanup. The rollback log lines are emitted at `error!` so
// they are visible in default-level operator logs without needing
// `RUST_LOG=warn`.

async fn rollback_sources(core: &Arc<DrasiLib>, sources: &[String]) -> Vec<String> {
    let mut failures = Vec::new();
    for source_id in sources {
        if let Err(e) = core.remove_source(source_id, false).await {
            log::error!("Clone rollback: failed to remove source '{source_id}': {e}");
            failures.push(format!("source '{source_id}': {e}"));
        }
    }
    failures
}

async fn rollback_queries(core: &Arc<DrasiLib>, queries: &[String]) -> Vec<String> {
    let mut failures = Vec::new();
    for query_id in queries {
        if let Err(e) = core.remove_query(query_id).await {
            log::error!("Clone rollback: failed to remove query '{query_id}': {e}");
            failures.push(format!("query '{query_id}': {e}"));
        }
    }
    failures
}

async fn rollback_reactions(core: &Arc<DrasiLib>, reactions: &[String]) -> Vec<String> {
    let mut failures = Vec::new();
    for reaction_id in reactions {
        if let Err(e) = core.remove_reaction(reaction_id, false).await {
            log::error!("Clone rollback: failed to remove reaction '{reaction_id}': {e}");
            failures.push(format!("reaction '{reaction_id}': {e}"));
        }
    }
    failures
}

/// Build an `ErrorResponse` for a clone-phase failure, attaching any
/// rollback failures into `ErrorDetail::technical_details` so operators
/// have a structured list of any orphans to inspect manually.
fn clone_error(
    code: &'static str,
    primary_message: String,
    component_type: &str,
    component_id: &str,
    rollback_failures: Vec<String>,
) -> ErrorResponse {
    let technical_details = if rollback_failures.is_empty() {
        None
    } else {
        Some(format!(
            "Rollback was best-effort and the following components could not be removed and may be orphaned: {}",
            rollback_failures.join("; ")
        ))
    };
    ErrorResponse::new(code, primary_message).with_details(ErrorDetail {
        component_type: Some(component_type.to_string()),
        component_id: Some(component_id.to_string()),
        technical_details,
    })
}
