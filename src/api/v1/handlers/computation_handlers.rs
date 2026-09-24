// Copyright 2026 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue},
    Json,
};
use drasi_lib::computation::v1::{
    ComputationInfo, ComputationOptions, GraphEntity, InstanceConfigurationSnapshot,
    OperationSummary,
};
use serde::Serialize;
use tokio::sync::RwLock;

use super::{InstancePath, ResourcePath};
use crate::{
    api::shared::{
        error::{error_codes, ErrorDetail, ErrorResponse},
        extractor::ConfigBody,
        handlers::{get_instance_or_error, persist_after_operation},
        ApiResponse, StatusResponse,
    },
    computation::ComputationGraphConfig,
    instance_registry::InstanceRegistry,
    persistence::ConfigPersistence,
    plugin_registry::PluginRegistry,
};

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComputationGraphInfo {
    pub id: String,
    pub auto_start: bool,
    pub state: String,
    pub revision: u64,
    pub driver_failed: bool,
}

impl From<ComputationInfo> for ComputationGraphInfo {
    fn from(info: ComputationInfo) -> Self {
        Self {
            id: info.id,
            auto_start: info.auto_start,
            state: format!("{:?}", info.state),
            revision: info.revision.0,
            driver_failed: info.driver_error.is_some(),
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComputationComponentInfo {
    pub id: String,
    pub role: String,
    pub ports: serde_json::Value,
    pub realization: Option<String>,
    pub lifecycle: Option<String>,
    pub health: Option<String>,
    pub failure_phase: Option<String>,
}

/// Public inspection deliberately excludes configuration values and provider recipes.
#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComputationGraphInspection {
    pub graph: ComputationGraphInfo,
    pub components: Vec<ComputationComponentInfo>,
    pub relationships: Vec<serde_json::Value>,
    pub resources: Vec<serde_json::Value>,
}

/// Privileged export; unlike inspection this can contain sensitive component configuration.
#[derive(utoipa::ToSchema)]
pub struct ComputationConfigurationSnapshotSchema {
    pub version: u32,
    pub instance: serde_json::Value,
    pub native_components: Option<serde_json::Value>,
    pub graphs: Vec<serde_json::Value>,
}

fn require_writable(read_only: bool) -> Result<(), ErrorResponse> {
    if read_only {
        Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot mutate computation graphs.",
        ))
    } else {
        Ok(())
    }
}

fn operation_error(id: &str, operation: &str, error: impl std::fmt::Display) -> ErrorResponse {
    ErrorResponse::new(
        error_codes::COMPUTATION_OPERATION_FAILED,
        format!("Failed to {operation} computation graph"),
    )
    .with_details(ErrorDetail {
        component_type: Some("computation".to_string()),
        component_id: Some(id.to_string()),
        technical_details: Some(error.to_string()),
    })
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/configuration",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    responses((status = 200, description = "Privileged full instance configuration; may contain secrets", body = ApiResponse<ComputationConfigurationSnapshotSchema>),
              (status = 404, description = "Instance not found", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn get_computation_configuration(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<(HeaderMap, Json<ApiResponse<InstanceConfigurationSnapshot>>), ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let snapshot = core
        .snapshot_computation_configuration()
        .await
        .map_err(|error| operation_error(&instance_id, "snapshot", error))?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok((headers, Json(ApiResponse::success(snapshot))))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/graphs",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    responses((status = 200, description = "Registered native graphs", body = ApiResponse<Vec<ComputationGraphInfo>>)),
    tag = "Computation"
)]
pub async fn list_computation_graphs(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<Vec<ComputationGraphInfo>>>, ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    Ok(Json(ApiResponse::success(
        core.list_computation_graphs()
            .await?
            .into_iter()
            .map(ComputationGraphInfo::from)
            .collect(),
    )))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/graphs",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body(content = ComputationGraphConfig, description = "Accepts application/json and application/yaml"),
    responses((status = 200, description = "Graph registered; inspect deployment and start status", body = ApiResponse<ComputationGraphInfo>),
              (status = 400, description = "Invalid graph or missing factory/resource", body = ErrorResponse),
              (status = 409, description = "Read-only or duplicate graph", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn create_computation_graph(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(plugins): Extension<Arc<RwLock<PluginRegistry>>>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(config): ConfigBody<ComputationGraphConfig>,
) -> Result<Json<ApiResponse<ComputationGraphInfo>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let id = &config.definition.graph_id;
    if core
        .list_computation_graphs()
        .await?
        .iter()
        .any(|graph| &graph.id == id)
    {
        return Err(ErrorResponse::new(
            error_codes::DUPLICATE_RESOURCE,
            "Computation graph already exists",
        ));
    }
    let plugins = plugins.read().await;
    let graph = crate::computation::build_graph(
        &config,
        &core,
        plugins
            .computation_factory_registry()
            .map_err(|error| operation_error(id, "read factories for", error))?,
        plugins
            .transactional_transformer_registry(core.middleware_registry())
            .map_err(|error| operation_error(id, "read transformers for", error))?,
    )
    .await
    .map_err(|error| {
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid computation graph configuration",
        )
        .with_details(ErrorDetail {
            component_type: Some("computation".to_string()),
            component_id: Some(id.clone()),
            technical_details: Some(format!("{error:#}")),
        })
    })?;
    drop(plugins);
    let handle = core
        .add_computation_graph(
            graph,
            ComputationOptions {
                auto_start: config.auto_start,
            },
        )
        .await?;
    persist_after_operation(&persistence, "creating computation graph").await?;
    Ok(Json(ApiResponse::success(handle.info().into())))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/graphs/{id}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("id" = String, Path, description = "Native graph ID")),
    responses((status = 200, description = "Native graph inspection without configuration values", body = ApiResponse<ComputationGraphInspection>),
              (status = 404, description = "Graph not found", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn inspect_computation_graph(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<ComputationGraphInspection>>, ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let handle = core.get_computation_graph(&id).await?;
    let snapshot = handle.inspector().snapshot();
    let components = snapshot
        .desired
        .nodes
        .iter()
        .map(|node| {
            let observed = snapshot.observed.components.get(node.descriptor.id());
            Ok(ComputationComponentInfo {
                id: node.descriptor.id().to_string(),
                role: format!("{:?}", node.role),
                ports: serde_json::to_value(node.descriptor.ports())?,
                realization: observed.map(|value| format!("{:?}", value.realization)),
                lifecycle: observed.map(|value| format!("{:?}", value.lifecycle)),
                health: observed.map(|value| format!("{:?}", value.health)),
                failure_phase: observed
                    .and_then(|value| value.failure.as_ref())
                    .map(|failure| format!("{:?}", failure.phase)),
            })
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()
        .map_err(|error| operation_error(&id, "inspect", error))?;
    let topology = snapshot.topology();
    let mut relationships = Vec::new();
    let mut resources = Vec::new();
    for entity in topology.nodes.values() {
        match entity {
            GraphEntity::Pipe(pipe) => relationships.push(serde_json::json!({
                "from": pipe.relationship.definition.from,
                "to": pipe.relationship.definition.to,
                "binding": pipe.observed.as_ref().map(|value| format!("{:?}", value.binding)),
                "availability": pipe.observed.as_ref().map(|value| format!("{:?}", value.availability)),
            })),
            GraphEntity::Resource(resource) => resources.push(serde_json::json!({
                "id": resource.desired.id,
                "role": resource.desired.role,
                "ownership": resource.desired.ownership,
                "realization": resource.observed.as_ref().map(|value| format!("{:?}", value.realization)),
            })),
            _ => {}
        }
    }
    Ok(Json(ApiResponse::success(ComputationGraphInspection {
        graph: handle.info().into(),
        components,
        relationships,
        resources,
    })))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/graphs/{id}/start",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("id" = String, Path, description = "Native graph ID")),
    responses((status = 200, description = "Graph started", body = ApiResponse<ComputationGraphInfo>),
              (status = 409, description = "Read-only", body = ErrorResponse),
              (status = 500, description = "One or more nodes failed to start", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn start_computation_graph(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<ComputationGraphInfo>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let handle = core.get_computation_graph(&id).await?;
    let report = handle
        .start()
        .await
        .map_err(|error| operation_error(&id, "start", error))?;
    if report.summary != OperationSummary::Completed {
        return Err(operation_error(
            &id,
            "start",
            "One or more nodes failed; inspect graph observations",
        ));
    }
    Ok(Json(ApiResponse::success(handle.info().into())))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/graphs/{id}/stop",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("id" = String, Path, description = "Native graph ID")),
    responses((status = 200, description = "Graph stopped", body = ApiResponse<ComputationGraphInfo>),
              (status = 409, description = "Read-only", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn stop_computation_graph(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<ComputationGraphInfo>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let handle = core.get_computation_graph(&id).await?;
    let report = handle
        .stop()
        .await
        .map_err(|error| operation_error(&id, "stop", error))?;
    if report.summary != OperationSummary::Completed {
        return Err(operation_error(
            &id,
            "stop",
            "One or more nodes failed; inspect graph observations",
        ));
    }
    Ok(Json(ApiResponse::success(handle.info().into())))
}

#[utoipa::path(
    delete, path = "/api/v1/instances/{instanceId}/computation/graphs/{id}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("id" = String, Path, description = "Native graph ID")),
    responses((status = 200, description = "Graph removed and owned resources disposed", body = ApiResponse<StatusResponse>),
              (status = 409, description = "Read-only", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn delete_computation_graph(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    core.remove_computation_graph(&id).await?;
    persist_after_operation(&persistence, "deleting computation graph").await?;
    Ok(Json(ApiResponse::success(StatusResponse {
        message: format!("Computation graph '{id}' removed"),
    })))
}
