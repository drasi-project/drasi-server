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

//! API v1 handler functions with OpenAPI documentation.
//!
//! These handlers wrap the shared handler implementations with v1-specific
//! path annotations for OpenAPI documentation. The actual business logic
//! is implemented in the shared handlers module.

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::{sse::Sse, Json},
};
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::models::{ComponentEventDto, LogMessageDto, QueryConfigDto};
use crate::api::shared::error::{error_codes, ErrorResponse, JsonBody};
use crate::api::shared::handlers::{ComponentViewQuery, ObservabilityQuery};
use crate::api::shared::{
    ApiResponse, ApiVersionsResponse, ComponentListItem, HealthResponse, InstanceListItem,
    StatusResponse,
};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;

// Re-export shared handler implementations
use crate::api::shared::handlers as shared;

/// Path parameter for instance-specific routes
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

/// Helper to get instance from registry or return error response
async fn get_instance(
    registry: &InstanceRegistry,
    instance_id: &str,
) -> Result<Arc<drasi_lib::DrasiLib>, (StatusCode, String)> {
    registry.get(instance_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Instance '{instance_id}' not found"),
        )
    })
}

/// List available API versions
#[utoipa::path(
    get,
    path = "/api/versions",
    responses(
        (status = 200, description = "List of available API versions", body = ApiVersionsResponse),
    ),
    tag = "API"
)]
pub async fn list_api_versions() -> Json<ApiVersionsResponse> {
    shared::list_api_versions().await
}

/// Check server health
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Server is healthy", body = HealthResponse),
    ),
    tag = "Health"
)]
pub async fn health_check() -> Json<HealthResponse> {
    shared::health_check().await
}

/// List configured DrasiLib instances
#[utoipa::path(
    get,
    path = "/api/v1/instances",
    responses(
        (status = 200, description = "List of DrasiLib instances", body = ApiResponse),
    ),
    tag = "Instances"
)]
pub async fn list_instances(
    Extension(registry): Extension<InstanceRegistry>,
) -> Json<ApiResponse<Vec<InstanceListItem>>> {
    shared::list_instances(Extension(registry)).await
}

/// Create a new DrasiLib instance
#[utoipa::path(
    post,
    path = "/api/v1/instances",
    request_body(content = inline(shared::CreateInstanceRequest)),
    responses(
        (status = 200, description = "Instance created successfully", body = ApiResponse),
        (status = 409, description = "Instance already exists"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Instances"
)]
pub async fn create_instance(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    JsonBody(request): JsonBody<shared::CreateInstanceRequest>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    shared::create_instance(
        Extension(registry),
        Extension(read_only),
        Extension(config_persistence),
        Json(request),
    )
    .await
}

/// Get a configuration snapshot of an instance
///
/// Returns an atomic point-in-time snapshot of all components (sources, queries,
/// reactions) with their configuration properties and dependency edges.
/// Data is read directly from the ComponentGraph — the single source of truth.
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/snapshot",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    responses(
        (status = 200, description = "Configuration snapshot"),
        (status = 404, description = "Instance not found"),
    ),
    tag = "Instances"
)]
pub async fn get_instance_snapshot(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<drasi_lib::ConfigurationSnapshot>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    match core.snapshot_configuration().await {
        Ok(snapshot) => Ok(Json(ApiResponse::success(snapshot))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to capture snapshot: {e}"),
        )),
    }
}

/// List all sources
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    responses(
        (status = 200, description = "List of sources", body = ApiResponse),
    ),
    tag = "Sources"
)]
pub async fn list_sources(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    Ok(shared::list_sources(Extension(core), Extension(instance_id)).await)
}

/// Create a new source
///
/// Creates a source from a configuration object. The `kind` field determines
/// the source type (mock, http, grpc, postgres).
///
/// Example request body:
/// ```json
/// {
///   "kind": "http",
///   "id": "my-http-source",
///   "auto_start": true,
///   "host": "0.0.0.0",
///   "port": 9000
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/sources",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    request_body = ref("#/components/schemas/SourceConfig"),
    responses(
        (status = 200, description = "Source created successfully", body = ApiResponse),
        (status = 400, description = "Invalid source configuration"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn create_source_handler(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::create_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Extension(plugin_registry),
        Json(config_json),
    )
    .await
}

/// Upsert a source (create or update)
///
/// Creates a source if it doesn't exist, or updates it if it does.
/// When updating, the existing source is stopped and replaced.
///
/// Example request body:
/// ```json
/// {
///   "kind": "http",
///   "id": "my-http-source",
///   "auto_start": true,
///   "host": "0.0.0.0",
///   "port": 9000
/// }
/// ```
#[utoipa::path(
    put,
    path = "/api/v1/instances/{instanceId}/sources/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    request_body = ref("#/components/schemas/SourceConfig"),
    responses(
        (status = 200, description = "Source created or updated successfully", body = ApiResponse),
        (status = 400, description = "Invalid source configuration"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn upsert_source_handler(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(path): Path<ResourcePath>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&path.instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::upsert_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(path.instance_id),
        Extension(plugin_registry),
        Path(path.id),
        Json(config_json),
    )
    .await
}

/// Get source details by ID
///
/// Optional `?view=full` returns the persisted config when available.
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID"),
        ("view" = Option<String>, Query, description = "Use view=full to include config")
    ),
    responses(
        (status = 200, description = "Source found", body = ApiResponse),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn get_source(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(view): Query<ComponentViewQuery>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_source(
        Extension(core),
        Extension(config_persistence),
        Extension(instance_id),
        Query(view),
        Path(id),
    )
    .await
}

/// Get source lifecycle events (snapshot)
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources/{id}/events",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID"),
        ("limit" = Option<usize>, Query, description = "Limit number of events (default 100)")
    ),
    responses(
        (status = 200, description = "Source events", body = ApiResponse<Vec<ComponentEventDto>>),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn get_source_events(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_source_events(Extension(core), Path(id), Query(query)).await
}

/// Stream source lifecycle events as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources/{id}/events/stream",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "SSE stream of source events"),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn stream_source_events(
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
    shared::stream_source_events(Extension(core), Path(id)).await
}

/// Get source logs (snapshot)
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources/{id}/logs",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID"),
        ("limit" = Option<usize>, Query, description = "Limit number of logs (default 100)")
    ),
    responses(
        (status = 200, description = "Source logs", body = ApiResponse<Vec<LogMessageDto>>),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn get_source_logs(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_source_logs(Extension(core), Path(id), Query(query)).await
}

/// Stream source logs as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources/{id}/logs/stream",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "SSE stream of source logs"),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn stream_source_logs(
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
    shared::stream_source_logs(Extension(core), Path(id)).await
}

/// Delete a source
#[utoipa::path(
    delete,
    path = "/api/v1/instances/{instanceId}/sources/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "Source deleted successfully", body = ApiResponse),
    ),
    tag = "Sources"
)]
pub async fn delete_source(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::delete_source(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Path(id),
    )
    .await
}

/// Start a source
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/sources/{id}/start",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "Source started successfully", body = ApiResponse),
        (status = 404, description = "Source not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn start_source(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::start_source(Extension(core), Path(id)).await
}

/// Stop a source
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/sources/{id}/stop",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "Source stopped successfully", body = ApiResponse),
        (status = 404, description = "Source not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn stop_source(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::stop_source(Extension(core), Path(id)).await
}

/// List all queries
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    responses(
        (status = 200, description = "List of queries", body = ApiResponse),
    ),
    tag = "Queries"
)]
pub async fn list_queries(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    Ok(shared::list_queries(Extension(core), Extension(instance_id)).await)
}

/// Create a new query
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/queries",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
    ),
    request_body = QueryConfigDto,
    responses(
        (status = 200, description = "Query created successfully", body = ApiResponse),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn create_query(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    JsonBody(config): JsonBody<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::create_query(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config),
    )
    .await
}

/// Get query by ID
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID"),
        ("view" = Option<String>, Query, description = "Use view=full to include config")
    ),
    responses(
        (status = 200, description = "Query found", body = ApiResponse),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn get_query(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(view): Query<ComponentViewQuery>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_query(
        Extension(core),
        Extension(config_persistence),
        Extension(instance_id),
        Query(view),
        Path(id),
    )
    .await
}

/// Get query lifecycle events (snapshot)
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}/events",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID"),
        ("limit" = Option<usize>, Query, description = "Limit number of events (default 100)")
    ),
    responses(
        (status = 200, description = "Query events", body = ApiResponse<Vec<ComponentEventDto>>),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn get_query_events(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_query_events(Extension(core), Path(id), Query(query)).await
}

/// Stream query lifecycle events as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}/events/stream",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "SSE stream of query events"),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn stream_query_events(
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
    shared::stream_query_events(Extension(core), Path(id)).await
}

/// Get query logs (snapshot)
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}/logs",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID"),
        ("limit" = Option<usize>, Query, description = "Limit number of logs (default 100)")
    ),
    responses(
        (status = 200, description = "Query logs", body = ApiResponse<Vec<LogMessageDto>>),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn get_query_logs(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_query_logs(Extension(core), Path(id), Query(query)).await
}

/// Stream query logs as SSE
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}/logs/stream",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "SSE stream of query logs"),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn stream_query_logs(
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
    shared::stream_query_logs(Extension(core), Path(id)).await
}

/// Delete a query
#[utoipa::path(
    delete,
    path = "/api/v1/instances/{instanceId}/queries/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query deleted successfully", body = ApiResponse),
    ),
    tag = "Queries"
)]
pub async fn delete_query(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::delete_query(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Path(id),
    )
    .await
}

/// Start a query
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/queries/{id}/start",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query started successfully", body = ApiResponse),
        (status = 404, description = "Query not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn start_query(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::start_query(Extension(core), Path(id)).await
}

/// Stop a query
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/queries/{id}/stop",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query stopped successfully", body = ApiResponse),
        (status = 404, description = "Query not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn stop_query(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::stop_query(Extension(core), Path(id)).await
}

/// Get current results of a query
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}/results",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Current query results", body = ApiResponse<Vec<serde_json::Value>>),
        (status = 404, description = "Query not found"),
        (status = 400, description = "Query is not running"),
    ),
    tag = "Queries"
)]
pub async fn get_query_results(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::get_query_results(Extension(core), Path(id)).await
}

/// Attach to a running query and stream results as NDJSON.
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/queries/{id}/attach",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Streaming query results (NDJSON)"),
        (status = 404, description = "Query not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn attach_query_stream(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> impl axum::response::IntoResponse {
    let core = match registry.get(&instance_id).await {
        Some(c) => c,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<StatusResponse>::error(
                    "Instance not found".to_string(),
                )),
            ))
        }
    };
    shared::attach_query_stream(Extension(core), Path(id)).await
}

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
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    Ok(shared::list_reactions(Extension(core), Extension(instance_id)).await)
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
    Json(config_json): Json<serde_json::Value>,
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
        Json(config_json),
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
    Json(config_json): Json<serde_json::Value>,
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
        Json(config_json),
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

/// Push data to a source's listening port (proxy to avoid CORS)
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/sources/{id}/push",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Data pushed successfully"),
        (status = 404, description = "Instance or source not found"),
    ),
    tag = "Sources"
)]
pub async fn push_source_data(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::push_source_data(Extension(core), Path(id), Json(body)).await
}

// ==================== Solution Template Handlers ====================

use crate::api::models::solution::{
    CreateSolutionTemplateRequest, CreateSolutionTemplateResponse, SolutionDeployRequest,
    SolutionDeployResponse, SolutionTemplateDetail, SolutionTemplateSummary,
};
use crate::api::shared::solutions;

/// List all available solution templates
#[utoipa::path(
    get,
    path = "/api/v1/catalog/solutions",
    responses(
        (status = 200, description = "List of solution templates", body = ApiResponse<Vec<SolutionTemplateSummary>>),
    ),
    tag = "Catalog"
)]
pub async fn list_solutions(
    Extension(solutions_dir): Extension<Option<String>>,
) -> Json<ApiResponse<Vec<SolutionTemplateSummary>>> {
    solutions::list_solutions(solutions_dir).await
}

/// Get detailed information about a solution template
#[utoipa::path(
    get,
    path = "/api/v1/catalog/solutions/{id}",
    params(
        ("id" = String, Path, description = "Solution template ID (filename without extension)")
    ),
    responses(
        (status = 200, description = "Solution template details", body = ApiResponse<SolutionTemplateDetail>),
        (status = 404, description = "Solution template not found"),
    ),
    tag = "Catalog"
)]
pub async fn get_solution(
    Extension(solutions_dir): Extension<Option<String>>,
    Path(id): Path<String>,
) -> Json<ApiResponse<SolutionTemplateDetail>> {
    solutions::get_solution(solutions_dir, &id).await
}

/// Create a new solution template from components in an instance
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/catalog/solutions",
    params(
        ("instanceId" = String, Path, description = "Source instance ID")
    ),
    request_body = CreateSolutionTemplateRequest,
    responses(
        (status = 200, description = "Creation result", body = ApiResponse<CreateSolutionTemplateResponse>),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Instance not found"),
    ),
    tag = "Catalog"
)]
pub async fn create_solution_template(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(solutions_dir): Extension<Option<String>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(request): Json<CreateSolutionTemplateRequest>,
) -> Json<ApiResponse<CreateSolutionTemplateResponse>> {
    let core = match registry.get(&instance_id).await {
        Some(c) => c,
        None => {
            return Json(ApiResponse::success(CreateSolutionTemplateResponse {
                success: false,
                template_id: None,
                error: Some(format!("Instance '{instance_id}' not found")),
            }));
        }
    };
    solutions::create_solution_template(core, persistence, solutions_dir, &instance_id, request)
        .await
}

/// Deploy a solution template to an instance
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/solutions",
    params(
        ("instanceId" = String, Path, description = "Target instance ID")
    ),
    request_body = SolutionDeployRequest,
    responses(
        (status = 200, description = "Deployment result", body = ApiResponse<SolutionDeployResponse>),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Instance or template not found"),
    ),
    tag = "Solutions"
)]
pub async fn deploy_solution(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(solutions_dir): Extension<Option<String>>,
    Extension(plugin_registry): Extension<Arc<RwLock<crate::plugin_registry::PluginRegistry>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(request): Json<SolutionDeployRequest>,
) -> Json<ApiResponse<SolutionDeployResponse>> {
    let reg = plugin_registry.read().await;
    solutions::deploy_solution(
        registry,
        persistence,
        solutions_dir,
        &reg,
        &instance_id,
        request,
    )
    .await
}

/// Clone another instance's configuration into this instance
///
/// Takes an atomic snapshot of the source instance and recreates all
/// components (sources, queries, reactions) in the target instance.
/// All cloned components are created in the stopped state.
/// On failure, already-created components are rolled back.
#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/clone",
    params(
        ("instanceId" = String, Path, description = "Target instance ID to clone into")
    ),
    request_body(content = inline(shared::CloneInstanceRequest)),
    responses(
        (status = 200, description = "Clone result", body = ApiResponse<shared::CloneInstanceResponse>),
        (status = 404, description = "Source or target instance not found"),
    ),
    tag = "Instances"
)]
pub async fn clone_instance(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(request): Json<shared::CloneInstanceRequest>,
) -> Result<Json<ApiResponse<shared::CloneInstanceResponse>>, ErrorResponse> {
    shared::clone_instance(
        registry,
        read_only,
        plugin_registry,
        config_persistence,
        &instance_id,
        &request.source_instance_id,
    )
    .await
}
