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

//! Solution template v1 API handler wrappers.

use axum::{
    extract::{Extension, Path},
    response::Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::models::solution::{
    CreateSolutionTemplateRequest, CreateSolutionTemplateResponse, SolutionDeployRequest,
    SolutionDeployResponse, SolutionTemplateDetail, SolutionTemplateSummary,
};
use crate::api::shared::error::{error_codes, ConfigBody, ErrorResponse};
use crate::api::shared::handlers as shared;
use crate::api::shared::solutions;
use crate::api::shared::ApiResponse;
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;

use super::InstancePath;

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
) -> Result<Json<ApiResponse<Vec<SolutionTemplateSummary>>>, ErrorResponse> {
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
) -> Result<Json<ApiResponse<SolutionTemplateDetail>>, ErrorResponse> {
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
        (status = 403, description = "Server is in read-only mode"),
        (status = 404, description = "Instance not found"),
    ),
    tag = "Catalog"
)]
pub async fn create_solution_template(
    Extension(read_only): Extension<Arc<bool>>,
    Extension(registry): Extension<InstanceRegistry>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(solutions_dir): Extension<Option<String>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(request): ConfigBody<CreateSolutionTemplateRequest>,
) -> Result<Json<ApiResponse<CreateSolutionTemplateResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create solution templates.",
        ));
    }
    let core = match registry.get(&instance_id).await {
        Some(c) => c,
        None => {
            return Err(ErrorResponse::new(
                error_codes::INSTANCE_NOT_FOUND,
                format!("Instance '{instance_id}' not found"),
            ));
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
        (status = 403, description = "Server is in read-only mode"),
        (status = 404, description = "Instance or template not found"),
    ),
    tag = "Solutions"
)]
pub async fn deploy_solution(
    Extension(read_only): Extension<Arc<bool>>,
    Extension(registry): Extension<InstanceRegistry>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(solutions_dir): Extension<Option<String>>,
    Extension(plugin_registry): Extension<Arc<RwLock<crate::plugin_registry::PluginRegistry>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(request): ConfigBody<SolutionDeployRequest>,
) -> Result<Json<ApiResponse<SolutionDeployResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot deploy solutions.",
        ));
    }
    solutions::deploy_solution(
        registry,
        persistence,
        solutions_dir,
        &plugin_registry,
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
    ConfigBody(request): ConfigBody<shared::CloneInstanceRequest>,
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
