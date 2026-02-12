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

use crate::api::models::{ComponentEventDto, LogMessageDto, QueryConfigDto};
use crate::api::shared::handlers::{ComponentViewQuery, ObservabilityQuery};
use crate::api::shared::{
    ApiResponse, ApiVersionsResponse, ComponentListItem, HealthResponse, InstanceListItem,
    StatusResponse,
};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;

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
            format!("Instance '{}' not found", instance_id),
        )
    })
}

/// Helper to get default instance from registry
async fn get_default(
    registry: &InstanceRegistry,
) -> Result<(String, Arc<drasi_lib::DrasiLib>), (StatusCode, String)> {
    registry
        .get_default()
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, "No instances configured".to_string()))
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
    Json(request): Json<shared::CreateInstanceRequest>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    shared::create_instance(
        Extension(registry),
        Extension(read_only),
        Extension(config_persistence),
        Json(request),
    )
    .await
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
/// the source type (mock, http, grpc, postgres, platform).
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
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    shared::create_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
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
    path = "/api/v1/instances/{instanceId}/sources",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
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
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    shared::upsert_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
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
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    StatusCode,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    StatusCode,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    Json(config): Json<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    StatusCode,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    StatusCode,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
/// the reaction type (log, http, http-adaptive, grpc, grpc-adaptive, sse, platform, profiler).
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
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    shared::create_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
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
    path = "/api/v1/instances/{instanceId}/reactions",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID")
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
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    shared::upsert_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
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
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    StatusCode,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
    StatusCode,
> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
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
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    shared::stop_reaction(Extension(core), Path(id)).await
}

// ============================================================================
// Default instance handlers (convenience routes)
// These use the first configured instance
// ============================================================================

/// List all sources (default instance)
pub async fn list_sources_default(
    Extension(registry): Extension<InstanceRegistry>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let (instance_id, core) = get_default(&registry).await?;
    Ok(shared::list_sources(Extension(core), Extension(instance_id)).await)
}

/// Create a new source (default instance)
pub async fn create_source_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::create_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config_json),
    )
    .await
}

/// Upsert a source (default instance)
pub async fn upsert_source_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::upsert_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config_json),
    )
    .await
}

/// Get source details (default instance)
pub async fn get_source_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
    Query(view): Query<ComponentViewQuery>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_source(
        Extension(core),
        Extension(config_persistence),
        Extension(instance_id),
        Query(view),
        Path(id),
    )
    .await
}

/// Get source events (default instance)
pub async fn get_source_events_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_source_events(Extension(core), Path(id), Query(query)).await
}

/// Stream source events (default instance)
pub async fn stream_source_events_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    StatusCode,
> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stream_source_events(Extension(core), Path(id)).await
}

/// Get source logs (default instance)
pub async fn get_source_logs_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_source_logs(Extension(core), Path(id), Query(query)).await
}

/// Stream source logs (default instance)
pub async fn stream_source_logs_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    StatusCode,
> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stream_source_logs(Extension(core), Path(id)).await
}

/// Delete a source (default instance)
pub async fn delete_source_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::delete_source(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Path(id),
    )
    .await
}

/// Start a source (default instance)
pub async fn start_source_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::start_source(Extension(core), Path(id)).await
}

/// Stop a source (default instance)
pub async fn stop_source_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stop_source(Extension(core), Path(id)).await
}

/// List all queries (default instance)
pub async fn list_queries_default(
    Extension(registry): Extension<InstanceRegistry>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let (instance_id, core) = get_default(&registry).await?;
    Ok(shared::list_queries(Extension(core), Extension(instance_id)).await)
}

/// Create a new query (default instance)
pub async fn create_query_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config): Json<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::create_query(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config),
    )
    .await
}

/// Get query details (default instance)
pub async fn get_query_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
    Query(view): Query<ComponentViewQuery>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_query(
        Extension(core),
        Extension(config_persistence),
        Extension(instance_id),
        Query(view),
        Path(id),
    )
    .await
}

/// Get query events (default instance)
pub async fn get_query_events_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_query_events(Extension(core), Path(id), Query(query)).await
}

/// Stream query events (default instance)
pub async fn stream_query_events_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    StatusCode,
> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stream_query_events(Extension(core), Path(id)).await
}

/// Get query logs (default instance)
pub async fn get_query_logs_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_query_logs(Extension(core), Path(id), Query(query)).await
}

/// Stream query logs (default instance)
pub async fn stream_query_logs_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    StatusCode,
> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stream_query_logs(Extension(core), Path(id)).await
}

/// Delete a query (default instance)
pub async fn delete_query_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::delete_query(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Path(id),
    )
    .await
}

/// Start a query (default instance)
pub async fn start_query_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::start_query(Extension(core), Path(id)).await
}

/// Stop a query (default instance)
pub async fn stop_query_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stop_query(Extension(core), Path(id)).await
}

/// Get query results (default instance)
pub async fn get_query_results_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_query_results(Extension(core), Path(id)).await
}

/// Attach to query stream (default instance)
pub async fn attach_query_stream_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    let core = match registry.get_default().await {
        Some((_, c)) => c,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<StatusResponse>::error(
                    "No instances configured".to_string(),
                )),
            ))
        }
    };
    shared::attach_query_stream(Extension(core), Path(id)).await
}

/// List all reactions (default instance)
pub async fn list_reactions_default(
    Extension(registry): Extension<InstanceRegistry>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let (instance_id, core) = get_default(&registry).await?;
    Ok(shared::list_reactions(Extension(core), Extension(instance_id)).await)
}

/// Create a new reaction (default instance)
pub async fn create_reaction_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::create_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config_json),
    )
    .await
}

/// Upsert a reaction (default instance)
pub async fn upsert_reaction_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::upsert_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config_json),
    )
    .await
}

/// Get reaction details (default instance)
pub async fn get_reaction_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
    Query(view): Query<ComponentViewQuery>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_reaction(
        Extension(core),
        Extension(config_persistence),
        Extension(instance_id),
        Query(view),
        Path(id),
    )
    .await
}

/// Get reaction events (default instance)
pub async fn get_reaction_events_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_reaction_events(Extension(core), Path(id), Query(query)).await
}

/// Stream reaction events (default instance)
pub async fn stream_reaction_events_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    StatusCode,
> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stream_reaction_events(Extension(core), Path(id)).await
}

/// Get reaction logs (default instance)
pub async fn get_reaction_logs_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::get_reaction_logs(Extension(core), Path(id), Query(query)).await
}

/// Stream reaction logs (default instance)
pub async fn stream_reaction_logs_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    StatusCode,
> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stream_reaction_logs(Extension(core), Path(id)).await
}

/// Delete a reaction (default instance)
pub async fn delete_reaction_default(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (instance_id, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::delete_reaction(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Path(id),
    )
    .await
}

/// Start a reaction (default instance)
pub async fn start_reaction_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::start_reaction(Extension(core), Path(id)).await
}

/// Stop a reaction (default instance)
pub async fn stop_reaction_default(
    Extension(registry): Extension<InstanceRegistry>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    let (_, core) = registry.get_default().await.ok_or(StatusCode::NOT_FOUND)?;
    shared::stop_reaction(Extension(core), Path(id)).await
}
