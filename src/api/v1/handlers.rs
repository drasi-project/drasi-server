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
    extract::{Extension, Path},
    http::StatusCode,
    response::Json,
};
use indexmap::IndexMap;
use std::sync::Arc;

use crate::api::models::{QueryConfigDto, ReactionConfig, SourceConfig};
use crate::api::shared::{
    ApiResponse, ApiVersionsResponse, ComponentListItem, HealthResponse, InstanceListItem,
    StatusResponse,
};
use crate::persistence::ConfigPersistence;
use drasi_lib::QueryConfig;

// Re-export shared handler implementations
use crate::api::shared::handlers as shared;

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
    Extension(instances): Extension<Arc<IndexMap<String, Arc<drasi_lib::DrasiLib>>>>,
) -> Json<ApiResponse<Vec<InstanceListItem>>> {
    shared::list_instances(Extension(instances)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    shared::list_sources(Extension(core)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    shared::create_source_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config_json),
    )
    .await
}

/// Get source status by ID
///
/// Note: Source configs are not stored - sources are instances.
/// This endpoint returns the source status instead.
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/sources/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "Source found", body = ApiResponse),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn get_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    shared::get_source(Extension(core), Path(id)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    shared::list_queries(Extension(core)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config): Json<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query found", body = ApiResponse),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn get_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<QueryConfig>>, StatusCode> {
    shared::get_query(Extension(core), Path(id)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, StatusCode> {
    shared::get_query_results(Extension(core), Path(id)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    shared::list_reactions(Extension(core)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    shared::create_reaction_handler(
        Extension(core),
        Extension(read_only),
        Extension(config_persistence),
        Extension(instance_id),
        Json(config_json),
    )
    .await
}

/// Get reaction status by ID
///
/// Note: Reaction configs are not stored - reactions are instances.
/// This endpoint returns the reaction status instead.
#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/reactions/{id}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction found", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn get_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    shared::get_reaction(Extension(core), Path(id)).await
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
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
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    shared::stop_reaction(Extension(core), Path(id)).await
}
