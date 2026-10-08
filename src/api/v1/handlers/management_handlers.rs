// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use axum::{
    extract::{Extension, Path},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    Json,
};
use drasi_lib::{
    computation::v1::{FailurePhase, ResourceRealization},
    management::{AcceptanceReceipt, CommittedConfiguration, DesiredInstance, ManagementError},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{computation_handlers::require_writable, InstancePath};
use crate::{
    api::shared::{
        error::{error_codes, ErrorResponse},
        extractor::ConfigBody,
        handlers::get_instance_or_error,
        ApiResponse,
    },
    computation::{validate_definition, ComputationConfig},
    instance_registry::InstanceRegistry,
};

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesiredConfigurationRequest {
    pub expected_revision: u64,
    pub request_id: String,
    /// Versioned topology, optionally with `retirement`: exact removed resources
    /// and components, `from_revision`, and `allow_data_loss: true`. Requires
    /// durable configuration storage and completed stop; never deletes stored data.
    #[schema(value_type = serde_json::Value)]
    pub desired: DesiredInstance,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationReceipt {
    pub request_id: String,
    pub revision: u64,
    pub durable: bool,
}

impl From<AcceptanceReceipt> for ConfigurationReceipt {
    fn from(receipt: AcceptanceReceipt) -> Self {
        Self {
            request_id: receipt.request_id,
            revision: receipt.revision,
            durable: receipt.durable,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationManagementStatus {
    pub revision: u64,
    pub persistent: bool,
    pub state: String,
    pub reconciling: bool,
    pub applied: bool,
    pub converged: bool,
    pub cleanup_pending: bool,
    pub failed_resources: Vec<String>,
}

#[derive(Deserialize)]
pub struct ReceiptPath {
    #[serde(rename = "instanceId")]
    instance_id: String,
    #[serde(rename = "requestId")]
    request_id: String,
}

fn no_store() -> HeaderMap {
    HeaderMap::from_iter([(CACHE_CONTROL, HeaderValue::from_static("no-store"))])
}

fn require_managed(core: &drasi_lib::DrasiLib) -> Result<(), ErrorResponse> {
    if !core.has_managed_configuration() {
        return Err(ErrorResponse::new(
            error_codes::MANAGED_CONFIGURATION_REQUIRED,
            "Configure configurationStore before using desired-state management",
        ));
    }
    Ok(())
}

fn management_error(error: drasi_lib::DrasiError) -> ErrorResponse {
    if error
        .downcast_ref::<drasi_lib::management::RecoveryTransitionRequired>()
        .is_some()
    {
        return ErrorResponse::new(
            error_codes::CONFIGURATION_TRANSITION_REQUIRED,
            "Configuration was not accepted: recovery storage or its connected processing definitions require verified drain or explicit retirement",
        );
    }
    let code = match error.downcast_ref::<ManagementError>() {
        Some(ManagementError::RevisionConflict { .. } | ManagementError::RequestConflict) => {
            error_codes::CONFIGURATION_CONFLICT
        }
        _ => error_codes::CONFIGURATION_UNCONFIRMED,
    };
    let message = if code == error_codes::CONFIGURATION_CONFLICT {
        "Configuration revision or request ID conflicts with an accepted request"
    } else {
        "Configuration operation was not confirmed; inspect the request receipt and management status before retrying"
    };
    ErrorResponse::new(code, message)
}

async fn status(
    core: &drasi_lib::DrasiLib,
) -> Result<ConfigurationManagementStatus, ErrorResponse> {
    let status = core.management_status().await.map_err(management_error)?;
    let snapshot = core.inspect_computation_graph()?.snapshot();
    let cleanup_pending = snapshot
        .observed
        .resources
        .values()
        .any(|resource| resource.realization == ResourceRealization::CleanupRequired)
        || snapshot.observed.components.values().any(|component| {
            component.failure.as_ref().is_some_and(|failure| {
                matches!(failure.phase, FailurePhase::Stop | FailurePhase::Removal)
            })
        });
    let failed = status.error.is_some()
        || !status.resource_errors.is_empty()
        || snapshot
            .observed
            .components
            .values()
            .any(|component| component.failure.is_some());
    let converged = status.converged();
    let state = if cleanup_pending {
        "cleanupPending"
    } else if failed {
        "failed"
    } else if converged {
        "ready"
    } else if status.reconciling {
        "initializing"
    } else {
        "accepted"
    };
    Ok(ConfigurationManagementStatus {
        revision: status.revision,
        persistent: status.persistent,
        state: state.into(),
        reconciling: status.reconciling,
        applied: status.applied,
        converged,
        cleanup_pending,
        failed_resources: status
            .resource_errors
            .keys()
            .map(ToString::to_string)
            .collect(),
    })
}

#[utoipa::path(
    put, path = "/api/v1/instances/{instanceId}/computation/desired",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    request_body(content = DesiredConfigurationRequest, description = "JSON or YAML revisioned desired state"),
    responses((status = 202, description = "Accepted before effects, not necessarily ready", body = ApiResponse<ConfigurationReceipt>),
        (status = 400, description = "Invalid definition"), (status = 409, description = "Conflict or read-only"),
        (status = 503, description = "Unconfirmed operation; resolve by request ID")),
    tag = "Computation"
)]
pub async fn apply_computation_desired(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    ConfigBody(request): ConfigBody<DesiredConfigurationRequest>,
) -> Result<
    (
        StatusCode,
        HeaderMap,
        Json<ApiResponse<ConfigurationReceipt>>,
    ),
    ErrorResponse,
> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    if request.desired.version != 1
        || request.request_id.is_empty()
        || request.request_id.len() > 1024
        || request.request_id.starts_with("drasi-server/")
    {
        return Err(ErrorResponse::new(error_codes::INVALID_REQUEST,
            "Use desired version 1 and a nonempty request ID of at most 1024 bytes outside the reserved drasi-server/ prefix"));
    }
    validate_definition(&ComputationConfig {
        definition: request.desired.topology.clone(),
    })
    .map_err(|_| {
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid reconstructible computation definition",
        )
    })?;
    let desired = request.desired.normalized().map_err(|_| {
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Invalid reconstructible instance definition",
        )
    })?;
    let receipt = core
        .apply_desired_state(request.expected_revision, request.request_id, desired)
        .await
        .map_err(management_error)?;
    Ok((
        StatusCode::ACCEPTED,
        no_store(),
        Json(ApiResponse::success(receipt.into())),
    ))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/desired",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    responses((status = 200, description = "Privileged authoritative desired state; may contain secrets", body = serde_json::Value),
        (status = 503, description = "State is unconfirmed")),
    tag = "Computation"
)]
pub async fn get_computation_desired(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<(HeaderMap, Json<ApiResponse<CommittedConfiguration>>), ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    Ok((
        no_store(),
        Json(ApiResponse::success(
            core.desired_configuration().map_err(management_error)?,
        )),
    ))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/management",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    responses((status = 200, description = "Secret-safe acceptance and realization status", body = ApiResponse<ConfigurationManagementStatus>)),
    tag = "Computation"
)]
pub async fn get_computation_management(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<(HeaderMap, Json<ApiResponse<ConfigurationManagementStatus>>), ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    Ok((no_store(), Json(ApiResponse::success(status(&core).await?))))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/reconcile",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID")),
    responses((status = 200, description = "Reconciliation attempted; inspect status", body = ApiResponse<ConfigurationManagementStatus>),
        (status = 409, description = "Read-only"), (status = 503, description = "Reconciliation could not be confirmed")),
    tag = "Computation"
)]
pub async fn reconcile_computation_desired(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
) -> Result<(HeaderMap, Json<ApiResponse<ConfigurationManagementStatus>>), ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    core.reconcile_desired_state()
        .await
        .map_err(management_error)?;
    Ok((no_store(), Json(ApiResponse::success(status(&core).await?))))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/receipts/{requestId}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("requestId" = String, Path, description = "Client request ID, URL encoded")),
    responses((status = 200, description = "Durably recorded acceptance receipt", body = ApiResponse<ConfigurationReceipt>),
        (status = 404, description = "No recorded acceptance; not proof of rejection while a request is in flight")),
    tag = "Computation"
)]
pub async fn get_computation_receipt(
    Extension(registry): Extension<InstanceRegistry>,
    Path(ReceiptPath {
        instance_id,
        request_id,
    }): Path<ReceiptPath>,
) -> Result<(HeaderMap, Json<ApiResponse<ConfigurationReceipt>>), ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let receipt = core
        .configuration_receipt(request_id)
        .await
        .map_err(management_error)?
        .ok_or_else(|| {
            ErrorResponse::new(
                error_codes::COMPUTATION_NOT_FOUND,
                "No recorded acceptance receipt",
            )
        })?;
    Ok((no_store(), Json(ApiResponse::success(receipt.into()))))
}
