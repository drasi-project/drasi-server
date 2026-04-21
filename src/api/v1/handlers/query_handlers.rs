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

//! Query-related v1 API handler wrappers.

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::{sse::Sse, Json},
};
use std::convert::Infallible;
use std::sync::Arc;

use crate::api::models::{ComponentEventDto, LogMessageDto, QueryConfigDto};
use crate::api::shared::error::{error_codes, ErrorResponse, JsonBody};
use crate::api::shared::handlers::{ComponentViewQuery, ObservabilityQuery};
use crate::api::shared::{ApiResponse, ComponentListItem, StatusResponse};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;

use super::{get_instance, InstancePath, ResourcePath};

// Re-export shared handler implementations
use crate::api::shared::handlers as shared;

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
    Extension(api_prefix): Extension<shared::ApiPrefix>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    shared::list_queries(Extension(core), Extension(instance_id), Extension(api_prefix))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.message))
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
    Extension(api_prefix): Extension<shared::ApiPrefix>,
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
        Extension(api_prefix),
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
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    ErrorResponse,
> {
    let core = match registry.get(&instance_id).await {
        Some(c) => c,
        None => {
            return Err(ErrorResponse::new(
                error_codes::INSTANCE_NOT_FOUND,
                format!("Instance '{instance_id}' not found"),
            ))
        }
    };
    shared::attach_query_stream(Extension(core), Path(id)).await
}
