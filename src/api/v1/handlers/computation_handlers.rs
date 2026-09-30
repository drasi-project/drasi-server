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
    ComponentId, ComputationInfo, DesiredMutation, GraphEntity, GraphSelection,
    InstanceConfigurationSnapshot, OperationSummary, RemovalPolicy, ResourceId, TopologyBindings,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::InstancePath;
use crate::{
    api::shared::{
        error::{error_codes, ErrorDetail, ErrorResponse},
        extractor::ConfigBody,
        handlers::{get_instance_or_error, persist_after_operation},
        ApiResponse, StatusResponse,
    },
    computation::ComputationConfig,
    instance_registry::InstanceRegistry,
    persistence::ConfigPersistence,
    plugin_registry::PluginRegistry,
};

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComputationGraphInfo {
    pub id: String,
    pub state: String,
    pub revision: u64,
    pub driver_failed: bool,
}

impl From<ComputationInfo> for ComputationGraphInfo {
    fn from(info: ComputationInfo) -> Self {
        Self {
            id: info.id,
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
    pub auto_start: bool,
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
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentSelection {
    pub components: Vec<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentRemoval {
    pub components: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
}

fn require_writable(read_only: bool) -> Result<(), ErrorResponse> {
    if read_only {
        Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot mutate computation components.",
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
    post, path = "/api/v1/instances/{instanceId}/computation/components",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body(content = ComputationConfig, description = "Accepts application/json and application/yaml"),
    responses((status = 200, description = "Components added; inspect individual creation and startup status", body = ApiResponse<ComputationGraphInfo>),
              (status = 400, description = "Invalid definition or missing factory/resource", body = ErrorResponse),
              (status = 409, description = "Read-only or duplicate component", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn create_computation_components(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(plugins): Extension<Arc<RwLock<PluginRegistry>>>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(config): ConfigBody<ComputationConfig>,
) -> Result<Json<ApiResponse<ComputationGraphInfo>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let snapshot = core.computation_control()?.desired_snapshot();
    for component in &config.definition.components {
        if snapshot
            .nodes
            .iter()
            .any(|node| node.descriptor.id() == component.descriptor.id())
        {
            return Err(ErrorResponse::new(
                error_codes::DUPLICATE_RESOURCE,
                format!("Component '{}' already exists", component.descriptor.id()),
            ));
        }
    }
    let plugins = plugins.read().await;
    let components = crate::computation::build_components(
        &config,
        &core,
        plugins
            .computation_factory_registry()
            .map_err(|error| operation_error(&instance_id, "read factories for", error))?,
        plugins
            .transactional_transformer_registry(core.middleware_registry())
            .map_err(|error| operation_error(&instance_id, "read transformers for", error))?,
    )
    .await
    .map_err(|error| {
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid computation component configuration",
        )
        .with_details(ErrorDetail {
            component_type: Some("computation".to_string()),
            component_id: Some(instance_id.clone()),
            technical_details: Some(format!("{error:#}")),
        })
    })?;
    drop(plugins);
    core.add_components(components).await?;
    persist_after_operation(&persistence, "creating computation components").await?;
    Ok(Json(ApiResponse::success(
        core.computation_info().await?.into(),
    )))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    responses((status = 200, description = "Native graph inspection without configuration values", body = ApiResponse<ComputationGraphInspection>),
              (status = 404, description = "Instance not found", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn inspect_computation_graph(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<Json<ApiResponse<ComputationGraphInspection>>, ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let snapshot = core.inspect_computation_graph()?.snapshot();
    let topology = snapshot.topology();
    let components = snapshot
        .desired
        .nodes
        .iter()
        .map(|node| {
            let observed = snapshot.observed.components.get(node.descriptor.id());
            Ok(ComputationComponentInfo {
                id: node.descriptor.id().to_string(),
                role: match topology.nodes.get(
                    &drasi_lib::computation::v1::GraphEntityId::Component(
                        node.descriptor.id().clone(),
                    ),
                ) {
                    Some(GraphEntity::Component(component)) => format!("{:?}", component.kind),
                    _ => format!("{:?}", node.role),
                },
                auto_start: snapshot.desired.lifecycle_policies[node.descriptor.id()].auto_start,
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
        .map_err(|error| operation_error(&instance_id, "inspect", error))?;
    let mut relationships = Vec::new();
    let mut resources = Vec::new();
    for entity in topology.nodes.values() {
        match entity {
            GraphEntity::Pipe(pipe) => relationships.push(serde_json::json!({
                "representation": "NativeProvider",
                "from": pipe.relationship.definition.from,
                "to": pipe.relationship.definition.to,
                "binding": pipe.observed.as_ref().map(|value| format!("{:?}", value.binding)),
                "availability": pipe.observed.as_ref().map(|value| format!("{:?}", value.availability)),
            })),
            GraphEntity::SubscriptionPipe(pipe) => relationships.push(serde_json::json!({
                "representation": "HostSubscription",
                "from": pipe.from,
                "to": pipe.to,
                "producerStarted": pipe.producer_started(),
                "consumerStarted": pipe.consumer_started(),
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
        graph: core.computation_info().await?.into(),
        components,
        relationships,
        resources,
    })))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/start",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body(content = ComponentSelection, description = "Native component IDs to start"),
    responses((status = 200, description = "Components started", body = ApiResponse<ComputationGraphInfo>),
              (status = 409, description = "Read-only", body = ErrorResponse),
              (status = 500, description = "One or more nodes failed to start", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn start_computation_components(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(selection): ConfigBody<ComponentSelection>,
) -> Result<Json<ApiResponse<ComputationGraphInfo>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let selected = native_selection(&core, &selection.components).await?;
    let control = core.computation_control()?;
    let report = control
        .start_requested(
            control.desired_snapshot().revision,
            GraphSelection::Exact(selected),
        )
        .await
        .map_err(|error| operation_error(&instance_id, "start", error))?;
    if report.summary != OperationSummary::Completed {
        return Err(operation_error(
            &instance_id,
            "start",
            "One or more nodes failed; inspect graph observations",
        ));
    }
    Ok(Json(ApiResponse::success(
        core.computation_info().await?.into(),
    )))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/stop",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body(content = ComponentSelection, description = "Native component IDs to stop"),
    responses((status = 200, description = "Components stopped", body = ApiResponse<ComputationGraphInfo>),
              (status = 409, description = "Read-only", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn stop_computation_components(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(selection): ConfigBody<ComponentSelection>,
) -> Result<Json<ApiResponse<ComputationGraphInfo>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let selected = native_selection(&core, &selection.components).await?;
    let control = core.computation_control()?;
    let revision = control.desired_snapshot().revision;
    let quiesced = control
        .quiesce_components(revision, GraphSelection::Exact(selected.clone()))
        .await;
    let report = control
        .stop_components(revision, GraphSelection::Exact(selected))
        .await
        .map_err(|error| operation_error(&instance_id, "stop", error))?;
    quiesced.map_err(|error| operation_error(&instance_id, "quiesce", error))?;
    if report.summary != OperationSummary::Completed {
        return Err(operation_error(
            &instance_id,
            "stop",
            "One or more nodes failed; inspect graph observations",
        ));
    }
    Ok(Json(ApiResponse::success(
        core.computation_info().await?.into(),
    )))
}

#[utoipa::path(
    delete, path = "/api/v1/instances/{instanceId}/computation/components",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body(content = ComponentRemoval, description = "Native component and resource IDs to remove"),
    responses((status = 200, description = "Components removed and selected owned resources disposed", body = ApiResponse<StatusResponse>),
              (status = 409, description = "Read-only", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn delete_computation_components(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(removal): ConfigBody<ComponentRemoval>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let selected = if removal.components.is_empty() && !removal.resources.is_empty() {
        Vec::new()
    } else {
        native_selection(&core, &removal.components).await?
    };
    let configuration = core.snapshot_computation_configuration().await?;
    let native = configuration.native_components.as_ref().ok_or_else(|| {
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "No native components are configured",
        )
    })?;
    let mut changes = Vec::new();
    if !selected.is_empty() {
        changes.push(DesiredMutation::RemoveComponents {
            selection: GraphSelection::Exact(selected),
            policy: RemovalPolicy::Reject,
        });
    }
    for id in removal.resources {
        let resource = ResourceId::try_new(id)
            .map_err(|error| operation_error(&instance_id, "remove resource", error))?;
        if !native
            .topology
            .resource_configurations
            .contains_key(&resource)
        {
            return Err(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Resource '{resource}' is not a declared native resource recipe"),
            ));
        }
        changes.push(DesiredMutation::RemoveResource {
            resource,
            policy: RemovalPolicy::Reject,
        });
    }
    let control = core.computation_control()?;
    let preview = control
        .preview(control.desired_snapshot().revision, changes)
        .await
        .map_err(|error| operation_error(&instance_id, "remove components", error))?;
    crate::computation::validate_named_mutation(preview.desired())
        .map_err(|error| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!("Invalid named pipe removal: {error:#}"),
            )
        })?;
    let report = control
        .reconcile(preview, TopologyBindings::default())
        .await
        .map_err(|error| operation_error(&instance_id, "remove components", error))?;
    if !report.committed || report.summary != OperationSummary::Completed {
        return Err(operation_error(
            &instance_id,
            "remove components",
            "component or resource cleanup is incomplete",
        ));
    }
    persist_after_operation(&persistence, "deleting computation components").await?;
    Ok(Json(ApiResponse::success(StatusResponse {
        message: "Computation components removed".into(),
    })))
}

async fn native_selection(
    core: &drasi_lib::DrasiLib,
    ids: &[String],
) -> Result<Vec<ComponentId>, ErrorResponse> {
    if ids.is_empty() {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "At least one component ID is required",
        ));
    }
    let configuration = core.snapshot_computation_configuration().await?;
    let declared = configuration
        .native_components
        .as_ref()
        .map(|native| native.topology.components.as_slice())
        .unwrap_or_default();
    ids.iter()
        .map(|id| {
            if !declared
                .iter()
                .any(|component| component.descriptor.id().as_str() == id)
            {
                return Err(ErrorResponse::new(
                    error_codes::INVALID_REQUEST,
                    format!("Native component '{id}' does not exist"),
                ));
            }
            ComponentId::try_new(id.as_str())
                .map_err(|error| operation_error(id, "select component", error))
        })
        .collect()
}
