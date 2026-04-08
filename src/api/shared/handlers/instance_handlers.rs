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
use std::sync::Arc;
use tokio::sync::RwLock;

use super::persist_after_operation;
use crate::api::models::BootstrapProviderConfig;
use crate::api::models::ConfigValue;
use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::api::shared::responses::{ApiResponse, StatusResponse};
use crate::config::{DrasiLibInstanceConfig, ReactionConfig, SourceConfig};
use crate::factories::{
    create_reaction, create_source, get_reaction_plugin_metadata, get_source_plugin_metadata,
};
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
    Json(request): Json<CreateInstanceRequest>,
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

    // Note: persist_index requires RocksDB setup which needs a data path
    // For now, we skip persistent index for dynamically created instances
    // TODO: Add support for persistent index with configurable data path

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
            default_priority_queue_capacity: request
                .default_priority_queue_capacity
                .map(ConfigValue::Static),
            default_dispatch_buffer_capacity: request
                .default_dispatch_buffer_capacity
                .map(ConfigValue::Static),
            sources: Vec::new(),
            reactions: Vec::new(),
            queries: Vec::new(),
        };
        persistence.register_instance(instance_config).await;
        persist_after_operation(&Some(persistence.clone()), "creating instance").await;
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
            config: properties_json,
        };

        let source =
            match create_source(&*plugin_registry.read().await, source_config.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Clone: failed to create source '{}': {e}", src_snap.id);
                    rollback_sources(&target_core, &sources_created).await;
                    return Err(ErrorResponse::new(
                        error_codes::SOURCE_CREATE_FAILED,
                        format!("Failed to create source '{}': {e}", src_snap.id),
                    ));
                }
            };

        let plugin_meta =
            get_source_plugin_metadata(&*plugin_registry.read().await, &source_config.kind);

        if let Err(e) = target_core
            .add_source_with_metadata(source, plugin_meta)
            .await
        {
            log::error!("Clone: failed to add source '{}': {e}", src_snap.id);
            rollback_sources(&target_core, &sources_created).await;
            return Err(ErrorResponse::new(
                error_codes::SOURCE_CREATE_FAILED,
                format!("Failed to add source '{}': {e}", src_snap.id),
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
            rollback_queries(&target_core, &queries_created).await;
            rollback_sources(&target_core, &sources_created).await;
            return Err(ErrorResponse::new(
                error_codes::QUERY_CREATE_FAILED,
                format!("Failed to add query '{}': {e}", q_snap.id),
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
            config: properties_json,
        };

        let reaction =
            match create_reaction(&*plugin_registry.read().await, reaction_config.clone()).await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Clone: failed to create reaction '{}': {e}", rx_snap.id);
                    rollback_reactions(&target_core, &reactions_created).await;
                    rollback_queries(&target_core, &queries_created).await;
                    rollback_sources(&target_core, &sources_created).await;
                    return Err(ErrorResponse::new(
                        error_codes::REACTION_CREATE_FAILED,
                        format!("Failed to create reaction '{}': {e}", rx_snap.id),
                    ));
                }
            };

        let plugin_meta =
            get_reaction_plugin_metadata(&*plugin_registry.read().await, &reaction_config.kind);

        if let Err(e) = target_core
            .add_reaction_with_metadata(reaction, plugin_meta)
            .await
        {
            log::error!("Clone: failed to add reaction '{}': {e}", rx_snap.id);
            rollback_reactions(&target_core, &reactions_created).await;
            rollback_queries(&target_core, &queries_created).await;
            rollback_sources(&target_core, &sources_created).await;
            return Err(ErrorResponse::new(
                error_codes::REACTION_CREATE_FAILED,
                format!("Failed to add reaction '{}': {e}", rx_snap.id),
            ));
        }

        reactions_created.push(rx_snap.id.clone());
    }

    persist_after_operation(&config_persistence, "cloning instance").await;

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

async fn rollback_sources(core: &Arc<DrasiLib>, sources: &[String]) {
    for source_id in sources {
        if let Err(e) = core.remove_source(source_id, false).await {
            log::warn!("Clone rollback: failed to remove source '{source_id}': {e}");
        }
    }
}

async fn rollback_queries(core: &Arc<DrasiLib>, queries: &[String]) {
    for query_id in queries {
        if let Err(e) = core.remove_query(query_id).await {
            log::warn!("Clone rollback: failed to remove query '{query_id}': {e}");
        }
    }
}

async fn rollback_reactions(core: &Arc<DrasiLib>, reactions: &[String]) {
    for reaction_id in reactions {
        if let Err(e) = core.remove_reaction(reaction_id, false).await {
            log::warn!("Clone rollback: failed to remove reaction '{reaction_id}': {e}");
        }
    }
}
