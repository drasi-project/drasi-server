// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    Json,
};
use drasi_lib::{config::ComponentId, management::OrdinaryComponentDefinition};
use serde::Deserialize;
use tokio::sync::RwLock;

use super::{
    computation_handlers::require_writable,
    management_handlers::{management_error, require_managed, ConfigurationReceipt},
    InstancePath, ResourcePath,
};
use crate::{
    api::{
        mappings::{queries::QueryConfigMapper, DtoMapper},
        models::{QueryConfigDto, ReactionConfig, SourceConfig},
        shared::{
            error::{error_codes, ErrorResponse},
            extractor::ConfigBody,
            handlers::get_instance_or_error,
            ApiResponse,
        },
    },
    instance_registry::InstanceRegistry,
    plugin_registry::PluginRegistry,
};

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", content = "definition", rename_all = "camelCase")]
pub enum OrdinaryDefinition {
    Source(SourceConfig),
    Query(QueryConfigDto),
    Reaction(ReactionConfig),
}

impl OrdinaryDefinition {
    fn into_definition(self) -> anyhow::Result<OrdinaryComponentDefinition> {
        match self {
            Self::Source(config) => {
                crate::managed_configuration::ordinary::source_definition(config)
            }
            Self::Query(config) => OrdinaryComponentDefinition::query(
                DtoMapper::new().map_with(&config, &QueryConfigMapper)?,
            ),
            Self::Reaction(config) => {
                crate::managed_configuration::ordinary::reaction_definition(config)
            }
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrdinaryDefinitionRequest {
    pub expected_revision: u64,
    pub request_id: String,
    pub component: OrdinaryDefinition,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationRevisionRequest {
    pub expected_revision: u64,
    pub request_id: String,
}

pub(super) fn validate_request_id(id: &str) -> Result<(), ErrorResponse> {
    if id.is_empty() || id.len() > 1024 || id.starts_with("drasi-server/") {
        return Err(ErrorResponse::new(error_codes::INVALID_REQUEST,
            "Use a nonempty request ID of at most 1024 bytes outside the reserved drasi-server/ prefix"));
    }
    Ok(())
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/ordinary-definitions",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body = OrdinaryDefinitionRequest,
    responses(
        (status = 200, description = "Durably assigned UUID; construction may still be pending", body = ApiResponse<ConfigurationReceipt>),
        (status = 400, description = "Invalid definition", body = ErrorResponse),
        (status = 409, description = "Identity, name or revision conflict", body = ErrorResponse)
    ),
    tag = "Computation"
)]
pub async fn create_managed_ordinary(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(plugins): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(request): ConfigBody<OrdinaryDefinitionRequest>,
) -> Result<Json<ApiResponse<ConfigurationReceipt>>, ErrorResponse> {
    require_writable(*read_only)?;
    validate_request_id(&request.request_id)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    if matches!(&request.component, OrdinaryDefinition::Query(_)) {
        crate::query_plugin::ensure_registered(&core, &plugins)
            .await
            .map_err(|error| {
                ErrorResponse::new(error_codes::PLUGIN_LOAD_FAILED, error.to_string())
            })?;
    }
    let definition = request
        .component
        .into_definition()
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    let receipt = core
        .create_managed_ordinary_component(
            request.expected_revision,
            request.request_id,
            definition,
        )
        .await
        .map_err(management_error)?;
    Ok(Json(ApiResponse::success(receipt.into())))
}

#[utoipa::path(
    put, path = "/api/v1/instances/{instanceId}/computation/ordinary-definitions/{id}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("id" = String, Path, description = "Existing component UUID")),
    request_body = OrdinaryDefinitionRequest,
    responses((status = 200, description = "Configuration accepted, retaining UUID", body = ApiResponse<ConfigurationReceipt>),
        (status = 409, description = "Revision conflict", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn update_managed_ordinary(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    ConfigBody(request): ConfigBody<OrdinaryDefinitionRequest>,
) -> Result<Json<ApiResponse<ConfigurationReceipt>>, ErrorResponse> {
    require_writable(*read_only)?;
    validate_request_id(&request.request_id)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let id = ComponentId::try_new(id)
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    let definition = request
        .component
        .into_definition()
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    if definition
        .id
        .as_ref()
        .is_some_and(|requested| requested != &id)
    {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "A component UUID cannot be changed",
        ));
    }
    let mut current = core.desired_configuration().map_err(management_error)?;
    let record = current
        .desired
        .ordinary
        .iter_mut()
        .find(|record| record.id == id)
        .ok_or_else(|| {
            ErrorResponse::new(
                error_codes::COMPUTATION_NOT_FOUND,
                "Ordinary component is not registered",
            )
        })?;
    if record.name != definition.name {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Use the rename endpoint to change a component name",
        ));
    }
    if std::mem::discriminant(&record.component) != std::mem::discriminant(&definition.component) {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "An ordinary component cannot change its semantic kind",
        ));
    }
    *record = definition.with_assigned_id(id);
    let receipt = core
        .apply_desired_state(
            request.expected_revision,
            request.request_id,
            current.desired,
        )
        .await
        .map_err(management_error)?;
    Ok(Json(ApiResponse::success(receipt.into())))
}

#[utoipa::path(
    delete, path = "/api/v1/instances/{instanceId}/computation/ordinary-definitions/{id}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("id" = String, Path, description = "Existing component UUID")),
    request_body = ConfigurationRevisionRequest,
    responses((status = 200, description = "Removal accepted", body = ApiResponse<ConfigurationReceipt>),
        (status = 409, description = "Revision or dependency conflict", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn delete_managed_ordinary(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    ConfigBody(request): ConfigBody<ConfigurationRevisionRequest>,
) -> Result<Json<ApiResponse<ConfigurationReceipt>>, ErrorResponse> {
    require_writable(*read_only)?;
    validate_request_id(&request.request_id)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let id = ComponentId::try_new(id)
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    let mut current = core.desired_configuration().map_err(management_error)?;
    let index = current
        .desired
        .ordinary
        .iter()
        .position(|record| record.id == id)
        .ok_or_else(|| {
            ErrorResponse::new(
                error_codes::COMPUTATION_NOT_FOUND,
                "Ordinary component is not registered",
            )
        })?;
    current.desired.ordinary.remove(index);
    let receipt = core
        .apply_desired_state(
            request.expected_revision,
            request.request_id,
            current.desired,
        )
        .await
        .map_err(management_error)?;
    Ok(Json(ApiResponse::success(receipt.into())))
}
