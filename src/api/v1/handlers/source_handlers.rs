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

//! Source-related v1 API handler wrappers.

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
    Extension(api_prefix): Extension<shared::ApiPrefix>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, (StatusCode, String)> {
    let core = get_instance(&registry, &instance_id).await?;
    shared::list_sources(
        Extension(core),
        Extension(instance_id),
        Extension(api_prefix),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.message))
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
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
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
        ConfigBody(config_json),
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
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
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
        ConfigBody(config_json),
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
    Extension(api_prefix): Extension<shared::ApiPrefix>,
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
        Extension(api_prefix),
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
    Extension(http_client): Extension<reqwest::Client>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    ConfigBody(body): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ErrorResponse> {
    let core = registry
        .get(&instance_id)
        .await
        .ok_or_else(|| ErrorResponse::new(error_codes::INTERNAL_ERROR, "Instance not found"))?;
    shared::push_source_data(
        Extension(core),
        Extension(http_client),
        Path(id),
        ConfigBody(body),
    )
    .await
}
