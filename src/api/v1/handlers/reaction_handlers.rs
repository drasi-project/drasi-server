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

//! Reaction-related v1 API handler wrappers.

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::{sse::Sse, Json},
};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::models::{ComponentEventDto, LogMessageDto};
use crate::api::shared::error::{error_codes, ConfigBody, ErrorResponse};
use crate::api::shared::handlers::{ComponentViewQuery, ObservabilityQuery};
use crate::api::shared::{ApiResponse, ComponentListItem, StatusResponse};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;

use super::{get_instance, InstancePath, ResourcePath};

// Re-export shared handler implementations
use crate::api::shared::handlers as shared;

/// List all reactions
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    responses(
        (status = 200, description = "List of reactions", body = ApiResponse),
    ),
    tag = "Reactions"
)]
pub async fn list_reactions(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(api_prefix): Extension<shared::ApiPrefix>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    shared::list_reactions(
        Extension(core),
        Extension(instance_id),
        Extension(api_prefix),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.message))
}

/// Create a new reaction
///
/// Creates a reaction from a configuration object. The `kind` field determines
/// the reaction type (log, http, http-adaptive, grpc, grpc-adaptive, sse, profiler).
///
/// Example request body:
/// ```json
/// {
///   "kind": "log",
///   "id": "my-log-reaction",
///   "queries": ["my-query"],
///   "auto_start": true,
///   "log_level": "info"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/reactions",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    request_body = ref("#/components/schemas/ReactionConfig"),
    responses(
        (status = 200, description = "Reaction created successfully", body = ApiResponse),
        (status = 400, description = "Invalid reaction configuration"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn create_reaction_handler(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::create_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Extension(plugin_registry),
        ConfigBody(config_json),
    )
    .await
}

/// Upsert a reaction (create or update)
///
/// Creates a reaction if it doesn't exist, or updates it if it does.
/// When updating, the existing reaction is stopped and replaced.
///
/// Example request body:
/// ```json
/// {
///   "kind": "log",
///   "id": "my-log-reaction",
///   "queries": ["my-query"],
///   "auto_start": true
/// }
/// ```
#[utoipa::path(
    put,
    path = "/api/v1/instances/{instanceId}/reactions/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    request_body = ref("#/components/schemas/ReactionConfig"),
    responses(
        (status = 200, description = "Reaction created or updated successfully", body = ApiResponse),
        (status = 400, description = "Invalid reaction configuration"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn upsert_reaction_handler(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(path): Path<ResourcePath>,
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&path.instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::upsert_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(path.instance_id),
        Extension(plugin_registry),
        Path(path.id),
        ConfigBody(config_json),
    )
    .await
}

/// Get reaction details by ID
///
/// Optional `?view=full` returns the persisted config when available.
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID"),
        ("view" = Option<String>, Query, description = "Use view=full to include config")
    ),
    responses(
        (status = 200, description = "Reaction found", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn get_reaction(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(api_prefix): Extension<shared::ApiPrefix>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(view): Query<ComponentViewQuery>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_reaction(
        Extension(core),
        Extension(config_persistence),
        Extension(instance_id),
        Extension(api_prefix),
        Query(view),
        Path(id),
    )
    .await
}

/// Get reaction lifecycle events (snapshot)
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions/{id}/events",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID"),
        ("limit" = Option<usize>, Query, description = "Limit number of events (default 100)")
    ),
    responses(
        (status = 200, description = "Reaction events", body = ApiResponse<Vec<ComponentEventDto>>),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn get_reaction_events(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_reaction_events(Extension(core), Path(id), Query(query)).await
}

/// Stream reaction lifecycle events as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions/{id}/events/stream",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "SSE stream of reaction events"),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn stream_reaction_events(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    ErrorResponse,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::stream_reaction_events(Extension(core), Path(id)).await
}

/// Get reaction logs (snapshot)
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions/{id}/logs",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID"),
        ("limit" = Option<usize>, Query, description = "Limit number of logs (default 100)")
    ),
    responses(
        (status = 200, description = "Reaction logs", body = ApiResponse<Vec<LogMessageDto>>),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn get_reaction_logs(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_reaction_logs(Extension(core), Path(id), Query(query)).await
}

/// Stream reaction logs as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions/{id}/logs/stream",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "SSE stream of reaction logs"),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn stream_reaction_logs(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    ErrorResponse,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::stream_reaction_logs(Extension(core), Path(id)).await
}

/// Delete a reaction
#[utoipa::path(
    delete,
    path = "/api/v1/instances/{instanceId}/reactions/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction deleted successfully", body = ApiResponse),
    ),
    tag = "Reactions"
)]
pub async fn delete_reaction(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::delete_reaction(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Path(id),
    )
    .await
}

/// Start a reaction
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/reactions/{id}/start",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction started successfully", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn start_reaction(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::start_reaction(Extension(core), Path(id)).await
}

/// Stop a reaction
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/reactions/{id}/stop",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction stopped successfully", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn stop_reaction(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::stop_reaction(Extension(core), Path(id)).await
}

// ============================================================================
// Global Component Events (SSE)
// ============================================================================

/// Stream all component events for an instance as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/events",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    responses(
        (status = 200, description = "SSE stream of all component events", content_type = "text/event-stream"),
        (status = 404, description = "Instance not found"),
    ),
    tag = "Instances"
)]
pub async fn stream_all_component_events(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    ErrorResponse,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    Ok(shared::stream_all_component_events(Extension(core)).await)
}
