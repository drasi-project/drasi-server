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

//! Shared handler implementations used across API versions.
//!
//! These handler functions contain the core business logic that can be
//! reused by version-specific handlers. Each API version may wrap these
//! with version-specific path annotations.

use axum::{
    extract::{Extension, Path, Query},
    http::{header, HeaderValue, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Json,
    },
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;

use super::responses::{
    ApiResponse, ApiVersionsResponse, ComponentLinks, ComponentListItem, HealthResponse,
    InstanceListItem, StatusResponse,
};
use crate::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};
use crate::api::models::ConfigValue;
use crate::api::models::{ComponentEventDto, LogMessageDto, QueryConfigDto};
use crate::config::{DrasiLibInstanceConfig, ReactionConfig, SourceConfig};
use crate::factories::{create_reaction, create_source};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use drasi_lib::{channels::ComponentStatus, queries::LabelExtractor, DrasiLib};
use drasi_reaction_application::subscription::SubscriptionOptions;
use drasi_reaction_application::ApplicationReaction;
use futures_util::{stream, StreamExt};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Path parameters for instance-specific routes
#[derive(Debug, Deserialize)]
pub struct InstancePath {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
}

/// Path parameters for resource-specific routes
#[derive(Debug, Deserialize)]
pub struct ResourcePath {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    pub id: String,
}

/// Helper to get an instance from the registry, returning an error response if not found
pub async fn get_instance_or_error(
    registry: &InstanceRegistry,
    instance_id: &str,
) -> Result<Arc<DrasiLib>, Json<ApiResponse<()>>> {
    match registry.get(instance_id).await {
        Some(core) => Ok(core),
        None => Err(Json(ApiResponse::error(format!(
            "Instance '{}' not found",
            instance_id
        )))),
    }
}

/// Helper to get the default instance from the registry
pub async fn get_default_instance_or_error(
    registry: &InstanceRegistry,
) -> Result<(String, Arc<DrasiLib>), Json<ApiResponse<()>>> {
    match registry.get_default().await {
        Some((id, core)) => Ok((id, core)),
        None => Err(Json(ApiResponse::error(
            "No instances configured".to_string(),
        ))),
    }
}

fn component_links(instance_id: &str, kind: &str, id: &str) -> ComponentLinks {
    let self_link = format!("/api/v1/instances/{instance_id}/{kind}/{id}");
    ComponentLinks {
        self_link: self_link.clone(),
        full: format!("{self_link}?view=full"),
    }
}

#[derive(Debug, Deserialize)]
pub struct ComponentViewQuery {
    view: Option<String>,
}

impl ComponentViewQuery {
    pub fn new(view: Option<String>) -> Self {
        Self { view }
    }

    fn include_config(&self) -> bool {
        matches!(self.view.as_deref(), Some("full"))
    }
}

const DEFAULT_OBSERVABILITY_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
pub struct ObservabilityQuery {
    pub limit: Option<usize>,
}

fn apply_limit<T>(mut items: Vec<T>, limit: Option<usize>) -> Vec<T> {
    let limit = limit.unwrap_or(DEFAULT_OBSERVABILITY_LIMIT);
    if limit == 0 {
        return Vec::new();
    }
    if items.len() > limit {
        let start = items.len() - limit;
        items = items.split_off(start);
    }
    items
}

fn sse_event<T: Serialize>(payload: T) -> Option<Result<Event, Infallible>> {
    match Event::default().json_data(payload) {
        Ok(event) => Some(Ok(event)),
        Err(e) => {
            log::warn!("Failed to serialize SSE payload: {e}");
            None
        }
    }
}

async fn sse_event_async<T: Serialize>(payload: T) -> Option<Result<Event, Infallible>> {
    sse_event(payload)
}

/// Helper function to persist configuration after a successful operation.
/// Logs errors but does not fail the request - persistence failures are non-fatal.
pub async fn persist_after_operation(
    config_persistence: &Option<Arc<ConfigPersistence>>,
    operation: &str,
) {
    if let Some(persistence) = config_persistence {
        if let Err(e) = persistence.save().await {
            log::error!("Failed to persist configuration after {operation}: {e}");
        }
    }
}

/// List available API versions
pub async fn list_api_versions() -> Json<ApiVersionsResponse> {
    Json(ApiVersionsResponse {
        versions: vec!["v1".to_string()],
        current: "v1".to_string(),
    })
}

/// Check server health
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// List configured DrasiLib instances
pub async fn list_instances(
    Extension(registry): Extension<InstanceRegistry>,
) -> Json<ApiResponse<Vec<InstanceListItem>>> {
    let instances = registry.list().await;
    let mut data = Vec::with_capacity(instances.len());

    for (id, instance) in instances {
        let source_count = instance.list_sources().await.map(|v| v.len()).unwrap_or(0);
        let query_count = instance.list_queries().await.map(|v| v.len()).unwrap_or(0);
        let reaction_count = instance
            .list_reactions()
            .await
            .map(|v| v.len())
            .unwrap_or(0);

        let base_path = format!("/api/v1/instances/{}", id);
        data.push(InstanceListItem {
            id: id.clone(),
            source_count,
            query_count,
            reaction_count,
            links: crate::api::shared::InstanceLinks {
                self_link: base_path.clone(),
                sources: format!("{}/sources", base_path),
                queries: format!("{}/queries", base_path),
                reactions: format!("{}/reactions", base_path),
            },
        });
    }

    Json(ApiResponse::success(data))
}

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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create instances.".to_string(),
        )));
    }

    let instance_id = request.id.clone();
    let persist_index = request.persist_index.unwrap_or(false);

    // Check if instance already exists
    if registry.contains(&instance_id).await {
        log::info!("Instance '{}' already exists", instance_id);
        return Err(StatusCode::CONFLICT);
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

    let core = builder.build().await;

    let core = match core {
        Ok(c) => Arc::new(c),
        Err(e) => {
            log::error!("Failed to create instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create instance: {e}"
            ))));
        }
    };

    // Start the instance
    if let Err(e) = core.start().await {
        log::error!("Failed to start instance '{}': {e}", instance_id);
        return Ok(Json(ApiResponse::error(format!(
            "Failed to start instance: {e}"
        ))));
    }

    // Add to registry
    if let Err(e) = registry.add(instance_id.clone(), core).await {
        log::error!("Failed to register instance: {e}");
        return Ok(Json(ApiResponse::error(e)));
    }

    log::info!("Instance '{}' created successfully", instance_id);

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
        message: format!("Instance '{}' created successfully", instance_id),
    })))
}

/// List all sources for an instance
pub async fn list_sources(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let sources = core.list_sources().await.unwrap_or_default();
    let mut items = Vec::with_capacity(sources.len());
    for (id, status) in sources {
        let links = component_links(&instance_id, "sources", &id);
        let error_message = if matches!(status, ComponentStatus::Error) {
            match core.get_source_info(&id).await {
                Ok(info) => info.error_message,
                Err(e) => {
                    log::warn!("Failed to fetch source info for '{id}': {e}");
                    None
                }
            }
        } else {
            None
        };
        items.push(ComponentListItem {
            id,
            status,
            error_message,
            links,
            config: None,
        });
    }

    Json(ApiResponse::success(items))
}

/// Create a new source
pub async fn create_source_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create sources.".to_string(),
        )));
    }

    let config: SourceConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to parse source config: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid source configuration: {e}"
            ))));
        }
    };

    let source_id = config.id().to_string();
    let auto_start = config.auto_start();

    let source = match create_source(config.clone()).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create source instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create source: {e}"
            ))));
        }
    };

    match core.add_source(source).await {
        Ok(_) => {
            log::info!("Source '{source_id}' created successfully");

            if let Some(persistence) = &config_persistence {
                persistence.register_source(&instance_id, config).await;
            }

            if auto_start {
                if let Err(e) = core.start_source(&source_id).await {
                    log::warn!("Failed to auto-start source '{source_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "creating source").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Source '{source_id}' created successfully"),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") {
                log::info!("Source '{source_id}' already exists - use PUT for upsert");
                return Err(StatusCode::CONFLICT);
            }
            log::error!("Failed to add source: {e}");
            Ok(Json(ApiResponse::error(error_msg)))
        }
    }
}

/// Upsert a source (create or update)
pub async fn upsert_source_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create or update sources.".to_string(),
        )));
    }

    let config: SourceConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to parse source config: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid source configuration: {e}"
            ))));
        }
    };

    let source_id = config.id().to_string();
    let auto_start = config.auto_start();

    // Check if source already exists
    let exists = core.get_source_status(&source_id).await.is_ok();

    if exists {
        // Create a new source instance and use update_source to replace in place
        let new_source = match create_source(config.clone()).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to create source instance for update: {e}");
                return Ok(Json(ApiResponse::error(format!(
                    "Failed to create source for update: {e}"
                ))));
            }
        };
        if let Err(e) = core.update_source(&source_id, new_source).await {
            log::error!("Failed to update source '{source_id}': {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to update source: {e}"
            ))));
        }

        log::info!("Source '{source_id}' updated successfully");

        if let Some(persistence) = &config_persistence {
            persistence
                .unregister_source(&instance_id, &source_id)
                .await;
            persistence.register_source(&instance_id, config).await;
        }

        persist_after_operation(&config_persistence, "upserting source").await;

        return Ok(Json(ApiResponse::success(StatusResponse {
            message: format!("Source '{source_id}' updated successfully"),
        })));
    }

    let source = match create_source(config.clone()).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create source instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create source: {e}"
            ))));
        }
    };

    match core.add_source(source).await {
        Ok(_) => {
            log::info!("Source '{source_id}' created successfully");

            if let Some(persistence) = &config_persistence {
                persistence.register_source(&instance_id, config).await;
            }

            if auto_start {
                if let Err(e) = core.start_source(&source_id).await {
                    log::warn!("Failed to auto-start source '{source_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "upserting source").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Source '{source_id}' created successfully"),
            })))
        }
        Err(e) => {
            log::error!("Failed to add source: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get source status by ID
pub async fn get_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let config = if view.include_config() {
        if let Some(persistence) = &config_persistence {
            persistence
                .get_source_config(&instance_id, &id)
                .await
                .map(|value| serde_json::to_value(value).unwrap())
        } else {
            None
        }
    } else {
        None
    };
    match core.get_source_info(&id).await {
        Ok(info) => Ok(Json(ApiResponse::success(ComponentListItem {
            id: info.id,
            status: info.status,
            error_message: info.error_message,
            links: component_links(&instance_id, "sources", &id),
            config,
        }))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Get source lifecycle events (snapshot).
pub async fn get_source_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    core.get_source_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let events = core
        .get_source_events(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let collected = events
        .map(ComponentEventDto::from)
        .collect::<Vec<_>>()
        .await;
    let data = apply_limit(collected, query.limit);
    Ok(Json(ApiResponse::success(data)))
}

/// Stream source lifecycle events as SSE.
pub async fn stream_source_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    core.get_source_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, receiver) = core
        .subscribe_source_events(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let history_stream =
        stream::iter(history.into_iter().map(ComponentEventDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => return Some((ComponentEventDto::from(event), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Get source logs (snapshot).
pub async fn get_source_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    core.get_source_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, _) = core
        .subscribe_source_logs(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let data = apply_limit(
        history.into_iter().map(LogMessageDto::from).collect(),
        query.limit,
    );
    Ok(Json(ApiResponse::success(data)))
}

/// Stream source logs as SSE.
pub async fn stream_source_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    core.get_source_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, mut receiver) = core
        .subscribe_source_logs(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let history_stream =
        stream::iter(history.into_iter().map(LogMessageDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(message) => return Some((LogMessageDto::from(message), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Delete a source
pub async fn delete_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete sources.".to_string(),
        )));
    }

    match core.remove_source(&id, true).await {
        Ok(_) => {
            if let Some(persistence) = &config_persistence {
                persistence.unregister_source(&instance_id, &id).await;
            }

            persist_after_operation(&config_persistence, "deleting source").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Source deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete source: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a source
pub async fn start_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.start_source(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source started successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Stop a source
pub async fn stop_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.stop_source(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source stopped successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// List all queries for an instance
pub async fn list_queries(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let queries = core.list_queries().await.unwrap_or_default();
    let mut items = Vec::with_capacity(queries.len());
    for (id, status) in queries {
        let links = component_links(&instance_id, "queries", &id);
        let error_message = if matches!(status, ComponentStatus::Error) {
            match core.get_query_info(&id).await {
                Ok(info) => info.error_message,
                Err(e) => {
                    log::warn!("Failed to fetch query info for '{id}': {e}");
                    None
                }
            }
        } else {
            None
        };
        items.push(ComponentListItem {
            id,
            status,
            error_message,
            links,
            config: None,
        });
    }

    Json(ApiResponse::success(items))
}

/// Create a new query
pub async fn create_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_dto): Json<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create queries.".to_string(),
        )));
    }

    let query_id = config_dto.id.clone();

    // Convert QueryConfigDto to drasi-lib's QueryConfig
    let mapper = DtoMapper::default();
    let query_mapper = QueryConfigMapper;
    let config = match mapper.map_with(&config_dto, &query_mapper) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to convert QueryConfigDto to QueryConfig: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid query configuration: {e}"
            ))));
        }
    };

    // Pre-flight join validation/logging (non-fatal warnings)
    if let Some(joins) = &config.joins {
        if !joins.is_empty() {
            match LabelExtractor::extract_labels(&config.query, &config.query_language) {
                Ok(labels) => {
                    let rel_labels: std::collections::HashSet<String> =
                        labels.relation_labels.into_iter().collect();
                    for j in joins {
                        if !rel_labels.contains(&j.id) {
                            log::warn!("[JOIN-VALIDATION] Query '{query_id}' defines join id '{}' which does not appear as a relationship label in the Cypher pattern.", j.id);
                        }
                        for key in &j.keys {
                            if key.label.trim().is_empty() || key.property.trim().is_empty() {
                                log::warn!("[JOIN-VALIDATION] Query '{query_id}' join '{}' has an empty label or property (label='{}', property='{}').", j.id, key.label, key.property);
                            }
                        }
                    }
                    log::info!(
                        "Registering query '{query_id}' with {} synthetic join(s)",
                        joins.len()
                    );
                }
                Err(e) => {
                    log::warn!(
                        "[JOIN-VALIDATION] Failed to parse query '{query_id}' for join validation: {e}"
                    );
                }
            }
        } else {
            log::debug!("Registering query '{query_id}' with no synthetic joins");
        }
    } else {
        log::debug!("Registering query '{query_id}' with no synthetic joins");
    }

    match core.add_query(config.clone()).await {
        Ok(_) => {
            log::info!("Query '{query_id}' created successfully");

            // Register the QueryConfigDto for persistence
            if let Some(persistence) = &config_persistence {
                persistence.register_query(&instance_id, config_dto).await;
            }

            persist_after_operation(&config_persistence, "creating query").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query created successfully".to_string(),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                log::info!("Query '{query_id}' already exists - use PUT for upsert");
                return Err(StatusCode::CONFLICT);
            }

            log::error!("Failed to create query: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get query by ID
pub async fn get_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    match core.get_query_config(&id).await {
        Ok(query_config) => {
            let config = if view.include_config() {
                let stored = if let Some(persistence) = &config_persistence {
                    persistence.get_query_config(&instance_id, &id).await
                } else {
                    None
                };
                let dto = stored.unwrap_or_else(|| QueryConfigDto::from(query_config.clone()));
                Some(serde_json::to_value(dto).unwrap())
            } else {
                None
            };
            let status = core
                .get_query_status(&query_config.id)
                .await
                .unwrap_or(ComponentStatus::Error);
            let error_message = if let Ok(info) = core.get_query_info(&query_config.id).await {
                info.error_message
            } else {
                None
            };
            Ok(Json(ApiResponse::success(ComponentListItem {
                id: query_config.id.clone(),
                status,
                error_message,
                links: component_links(&instance_id, "queries", &id),
                config,
            })))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Get query lifecycle events (snapshot).
pub async fn get_query_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    core.get_query_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let events = core
        .get_query_events(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let collected = events
        .map(ComponentEventDto::from)
        .collect::<Vec<_>>()
        .await;
    let data = apply_limit(collected, query.limit);
    Ok(Json(ApiResponse::success(data)))
}

/// Stream query lifecycle events as SSE.
pub async fn stream_query_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    core.get_query_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, receiver) = core
        .subscribe_query_events(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let history_stream =
        stream::iter(history.into_iter().map(ComponentEventDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => return Some((ComponentEventDto::from(event), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Get query logs (snapshot).
pub async fn get_query_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    core.get_query_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, _) = core
        .subscribe_query_logs(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let data = apply_limit(
        history.into_iter().map(LogMessageDto::from).collect(),
        query.limit,
    );
    Ok(Json(ApiResponse::success(data)))
}

/// Stream query logs as SSE.
pub async fn stream_query_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    core.get_query_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, mut receiver) = core
        .subscribe_query_logs(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let history_stream =
        stream::iter(history.into_iter().map(LogMessageDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(message) => return Some((LogMessageDto::from(message), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Delete a query
pub async fn delete_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete queries.".to_string(),
        )));
    }

    match core.remove_query(&id).await {
        Ok(_) => {
            // Unregister the query from persistence
            if let Some(persistence) = &config_persistence {
                persistence.unregister_query(&instance_id, &id).await;
            }

            persist_after_operation(&config_persistence, "deleting query").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete query: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a query
pub async fn start_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.start_query(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query started successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Stop a query
pub async fn stop_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.stop_query(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query stopped successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Get current results of a query
pub async fn get_query_results(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, StatusCode> {
    match core.get_query_results(&id).await {
        Ok(results) => Ok(Json(ApiResponse::success(results))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Attach to a running query and stream results as NDJSON.
pub async fn attach_query_stream(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ApiResponse<StatusResponse>>),
> {
    if let Err(e) = core.get_query_config(&id).await {
        let error_msg = e.to_string();
        let status = if error_msg.contains("not found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        return Err((status, Json(ApiResponse::error(error_msg))));
    }

    let reaction_id = format!("__attach_{}_{}", id, Uuid::new_v4());
    let (reaction, handle) = ApplicationReaction::new(reaction_id.clone(), vec![id.clone()]);
    if let Err(e) = core.add_reaction(reaction).await {
        let error_msg = format!("Failed to add attach reaction: {e}");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(error_msg)),
        ));
    }

    if let Err(e) = core.start_reaction(&reaction_id).await {
        let error_msg = e.to_string();
        if !error_msg.contains("already running") {
            let _ = core.remove_reaction(&reaction_id, true).await;
            let error_msg = format!("Failed to start attach reaction: {error_msg}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(error_msg)),
            ));
        }
    }

    let options = SubscriptionOptions::default().with_query_filter(vec![id.clone()]);
    let subscription = match handle.subscribe_with_options(options).await {
        Ok(subscription) => subscription,
        Err(e) => {
            let _ = core.remove_reaction(&reaction_id, true).await;
            let error_msg = format!("Failed to subscribe to attach reaction: {e}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(error_msg)),
            ));
        }
    };

    let stream = subscription.into_stream();
    let cleanup_core = core.clone();
    let cleanup_id = reaction_id.clone();

    // Create an async stream that yields query results and cleans up on drop
    let sse_stream = async_stream::stream! {
        let mut stream = stream;
        let _cleanup = AttachCleanupGuard::new(cleanup_core, cleanup_id);

        while let Some(result) = stream.next().await {
            if let Ok(json) = serde_json::to_string(&result) {
                yield Ok(Event::default().data(json));
            }
        }
    };

    Ok(Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

/// Guard that cleans up the attach reaction when dropped.
struct AttachCleanupGuard {
    core: Arc<drasi_lib::DrasiLib>,
    reaction_id: String,
}

impl AttachCleanupGuard {
    fn new(core: Arc<drasi_lib::DrasiLib>, reaction_id: String) -> Self {
        Self { core, reaction_id }
    }
}

impl Drop for AttachCleanupGuard {
    fn drop(&mut self) {
        let core = self.core.clone();
        let id = self.reaction_id.clone();
        tokio::spawn(async move {
            let _ = core.remove_reaction(&id, true).await;
            log::debug!("Cleaned up attach reaction: {}", id);
        });
    }
}

/// List all reactions for an instance
pub async fn list_reactions(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let reactions = core.list_reactions().await.unwrap_or_default();
    let mut items = Vec::with_capacity(reactions.len());
    for (id, status) in reactions {
        let links = component_links(&instance_id, "reactions", &id);
        let error_message = if matches!(status, ComponentStatus::Error) {
            match core.get_reaction_info(&id).await {
                Ok(info) => info.error_message,
                Err(e) => {
                    log::warn!("Failed to fetch reaction info for '{id}': {e}");
                    None
                }
            }
        } else {
            None
        };
        items.push(ComponentListItem {
            id,
            status,
            error_message,
            links,
            config: None,
        });
    }

    Json(ApiResponse::success(items))
}

/// Create a new reaction
pub async fn create_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create reactions.".to_string(),
        )));
    }

    let config: ReactionConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to parse reaction config: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid reaction configuration: {e}"
            ))));
        }
    };

    let reaction_id = config.id().to_string();
    let auto_start = config.auto_start();

    let reaction = match create_reaction(config.clone()) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create reaction instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create reaction: {e}"
            ))));
        }
    };

    match core.add_reaction(reaction).await {
        Ok(_) => {
            log::info!("Reaction '{reaction_id}' created successfully");

            if let Some(persistence) = &config_persistence {
                persistence.register_reaction(&instance_id, config).await;
            }

            if auto_start {
                if let Err(e) = core.start_reaction(&reaction_id).await {
                    log::warn!("Failed to auto-start reaction '{reaction_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "creating reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Reaction '{reaction_id}' created successfully"),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") {
                log::info!("Reaction '{reaction_id}' already exists - use PUT for upsert");
                return Err(StatusCode::CONFLICT);
            }
            log::error!("Failed to add reaction: {e}");
            Ok(Json(ApiResponse::error(error_msg)))
        }
    }
}

/// Upsert a reaction (create or update)
pub async fn upsert_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create or update reactions.".to_string(),
        )));
    }

    let config: ReactionConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to parse reaction config: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid reaction configuration: {e}"
            ))));
        }
    };

    let reaction_id = config.id().to_string();
    let auto_start = config.auto_start();

    // Check if reaction already exists
    let exists = core.get_reaction_info(&reaction_id).await.is_ok();

    if exists {
        // Create a new reaction instance and use update_reaction to replace in place
        let new_reaction = match create_reaction(config.clone()) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to create reaction instance for update: {e}");
                return Ok(Json(ApiResponse::error(format!(
                    "Failed to create reaction for update: {e}"
                ))));
            }
        };
        if let Err(e) = core.update_reaction(&reaction_id, new_reaction).await {
            log::error!("Failed to update reaction '{reaction_id}': {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to update reaction: {e}"
            ))));
        }

        log::info!("Reaction '{reaction_id}' updated successfully");

        if let Some(persistence) = &config_persistence {
            persistence
                .unregister_reaction(&instance_id, &reaction_id)
                .await;
            persistence.register_reaction(&instance_id, config).await;
        }

        persist_after_operation(&config_persistence, "upserting reaction").await;

        return Ok(Json(ApiResponse::success(StatusResponse {
            message: format!("Reaction '{reaction_id}' updated successfully"),
        })));
    }

    let reaction = match create_reaction(config.clone()) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create reaction instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create reaction: {e}"
            ))));
        }
    };

    match core.add_reaction(reaction).await {
        Ok(_) => {
            log::info!("Reaction '{reaction_id}' created successfully");

            if let Some(persistence) = &config_persistence {
                persistence.register_reaction(&instance_id, config).await;
            }

            if auto_start {
                if let Err(e) = core.start_reaction(&reaction_id).await {
                    log::warn!("Failed to auto-start reaction '{reaction_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "upserting reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Reaction '{reaction_id}' created successfully"),
            })))
        }
        Err(e) => {
            log::error!("Failed to add reaction: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get reaction status by ID
pub async fn get_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let config = if view.include_config() {
        if let Some(persistence) = &config_persistence {
            persistence
                .get_reaction_config(&instance_id, &id)
                .await
                .map(|value| serde_json::to_value(value).unwrap())
        } else {
            None
        }
    } else {
        None
    };
    match core.get_reaction_info(&id).await {
        Ok(info) => Ok(Json(ApiResponse::success(ComponentListItem {
            id: info.id,
            status: info.status,
            error_message: info.error_message,
            links: component_links(&instance_id, "reactions", &id),
            config,
        }))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Get reaction lifecycle events (snapshot).
pub async fn get_reaction_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    core.get_reaction_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let events = core
        .get_reaction_events(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let collected = events
        .map(ComponentEventDto::from)
        .collect::<Vec<_>>()
        .await;
    let data = apply_limit(collected, query.limit);
    Ok(Json(ApiResponse::success(data)))
}

/// Stream reaction lifecycle events as SSE.
pub async fn stream_reaction_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    core.get_reaction_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, receiver) = core
        .subscribe_reaction_events(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let history_stream =
        stream::iter(history.into_iter().map(ComponentEventDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => return Some((ComponentEventDto::from(event), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Get reaction logs (snapshot).
pub async fn get_reaction_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    core.get_reaction_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, _) = core
        .subscribe_reaction_logs(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let data = apply_limit(
        history.into_iter().map(LogMessageDto::from).collect(),
        query.limit,
    );
    Ok(Json(ApiResponse::success(data)))
}

/// Stream reaction logs as SSE.
pub async fn stream_reaction_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    core.get_reaction_info(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let (history, mut receiver) = core
        .subscribe_reaction_logs(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let history_stream =
        stream::iter(history.into_iter().map(LogMessageDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(message) => return Some((LogMessageDto::from(message), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Delete a reaction
pub async fn delete_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete reactions.".to_string(),
        )));
    }

    match core.remove_reaction(&id, true).await {
        Ok(_) => {
            if let Some(persistence) = &config_persistence {
                persistence.unregister_reaction(&instance_id, &id).await;
            }

            persist_after_operation(&config_persistence, "deleting reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete reaction: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a reaction
pub async fn start_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.start_reaction(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction started successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Stop a reaction
pub async fn stop_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.stop_reaction(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction stopped successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}
