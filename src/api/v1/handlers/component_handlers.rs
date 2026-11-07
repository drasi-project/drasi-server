// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    Json,
};
use drasi_lib::config::{ComponentId, ComponentName};
use serde::{Deserialize, Serialize};

use super::ResourcePath;
use crate::{
    api::shared::{
        error::{error_codes, ErrorResponse},
        extractor::ConfigBody,
        handlers::{get_instance_or_error, persist_after_operation},
        ApiResponse,
    },
    instance_registry::InstanceRegistry,
    persistence::ConfigPersistence,
};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ComponentIdentity {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ComponentNamePath {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameComponentRequest {
    #[schema(value_type = String)]
    pub name: ComponentName,
    pub expected_revision: Option<u64>,
    pub request_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/instances/{instanceId}/components/by-name/{name}",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("name" = String, Path, description = "Exact case-sensitive component name")
    ),
    responses(
        (status = 200, description = "Current component identity", body = ApiResponse),
        (status = 404, description = "Name not registered", body = ErrorResponse)
    ),
    tag = "Components"
)]
pub async fn lookup_component(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ComponentNamePath { instance_id, name }): Path<ComponentNamePath>,
) -> Result<Json<ApiResponse<ComponentIdentity>>, ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let name = ComponentName::try_new(name)
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    let snapshot = core.computation_control()?.desired_snapshot();
    let component = snapshot.component_by_name(&name).ok_or_else(|| {
        ErrorResponse::new(
            error_codes::COMPUTATION_NOT_FOUND,
            format!("Component name '{name}' is not registered"),
        )
    })?;
    Ok(Json(ApiResponse::success(ComponentIdentity {
        id: component.descriptor.id().to_string(),
        name: component.name.to_string(),
    })))
}

#[utoipa::path(
    post,
    path = "/api/v1/instances/{instanceId}/components/{id}/rename",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("id" = String, Path, description = "Immutable component UUID")
    ),
    request_body = RenameComponentRequest,
    responses(
        (status = 200, description = "Name accepted without replacing the component", body = ApiResponse),
        (status = 400, description = "Invalid rename request", body = ErrorResponse),
        (status = 409, description = "Name or revision conflict", body = ErrorResponse)
    ),
    tag = "Components"
)]
pub async fn rename_component(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(ResourcePath { instance_id, id }): Path<ResourcePath>,
    ConfigBody(request): ConfigBody<RenameComponentRequest>,
) -> Result<Json<ApiResponse<ComponentIdentity>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is read-only",
        ));
    }
    let core = get_instance_or_error(&registry, &instance_id).await?;
    let id = ComponentId::try_new(id)
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    let managed = if core.has_managed_configuration() {
        let current = core
            .desired_configuration()
            .map_err(super::management_handlers::management_error)?;
        (current
            .desired
            .topology
            .components
            .iter()
            .any(|component| component.descriptor.id() == &id)
            || current
                .desired
                .ordinary
                .iter()
                .any(|component| component.id == id))
        .then_some(current)
    } else {
        None
    };
    if let Some(mut current) = managed {
        let revision = request.expected_revision.ok_or_else(|| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Managed rename requires expectedRevision",
            )
        })?;
        let request_id = request.request_id.ok_or_else(|| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Managed rename requires requestId",
            )
        })?;
        super::managed_ordinary_handlers::validate_request_id(&request_id)?;
        if let Some(component) = current
            .desired
            .topology
            .components
            .iter_mut()
            .find(|component| component.descriptor.id() == &id)
        {
            component.name = request.name.clone();
        } else {
            let component = current
                .desired
                .ordinary
                .iter_mut()
                .find(|component| component.id == id)
                .ok_or_else(|| {
                    ErrorResponse::new(error_codes::COMPUTATION_NOT_FOUND, "Component was removed")
                })?;
            component.name = request.name.clone();
        }
        core.apply_desired_state(revision, request_id, current.desired)
            .await
            .map_err(super::management_handlers::management_error)?;
    } else {
        if request.expected_revision.is_some() || request.request_id.is_some() {
            return Err(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "This component is not managed",
            ));
        }
        core.rename_component(id.as_str(), request.name.as_str())
            .await?;
    }
    persist_after_operation(&persistence, "renaming component").await?;
    Ok(Json(ApiResponse::success(ComponentIdentity {
        id: id.to_string(),
        name: request.name.to_string(),
    })))
}
