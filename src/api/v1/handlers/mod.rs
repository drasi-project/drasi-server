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

mod query_handlers;
mod reaction_handlers;
mod solution_handlers;
mod source_handlers;

pub use query_handlers::*;
pub use reaction_handlers::*;
pub use solution_handlers::*;
pub use source_handlers::*;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::api::shared::error::ConfigBody;
use crate::api::shared::handlers as shared;
use crate::api::shared::{
    ApiResponse, ApiVersionsResponse, HealthResponse, InstanceListItem, StatusResponse,
};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;

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
    Extension(api_prefix): Extension<shared::ApiPrefix>,
) -> Json<ApiResponse<Vec<InstanceListItem>>> {
    shared::list_instances(Extension(registry), Extension(api_prefix)).await
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
    ConfigBody(request): ConfigBody<shared::CreateInstanceRequest>,
) -> Result<Json<ApiResponse<StatusResponse>>, crate::api::shared::error::ErrorResponse> {
    shared::create_instance(
        Extension(registry),
        Extension(read_only),
        Extension(config_persistence),
        ConfigBody(request),
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
