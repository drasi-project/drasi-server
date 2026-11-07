// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{num::NonZeroUsize, sync::Arc};

use axum::{
    extract::{Extension, Path, Query},
    http::HeaderMap,
    Json,
};
use drasi_lib::management::{CommittedConfiguration, ConfigurationSnapshotSummary};
use serde::Deserialize;

use super::{
    computation_handlers::require_writable,
    managed_ordinary_handlers::{validate_request_id, ConfigurationRevisionRequest},
    management_handlers::{management_error, no_store, require_managed, ConfigurationReceipt},
    InstancePath,
};
use crate::{
    api::shared::{
        error::{error_codes, ErrorResponse},
        extractor::ConfigBody,
        handlers::get_instance_or_error,
        ApiResponse,
    },
    instance_registry::InstanceRegistry,
};

#[derive(Deserialize)]
pub struct SnapshotPath {
    #[serde(rename = "instanceId")]
    instance_id: String,
    name: String,
}

#[derive(Deserialize)]
pub struct SnapshotListQuery {
    after: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteSnapshotRequest {
    pub expected_revision: u64,
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/snapshots",
    params(
        ("instanceId" = String, Path, description = "DrasiLib instance ID"),
        ("after" = Option<String>, Query, description = "Exclusive name cursor"),
        ("limit" = Option<usize>, Query, description = "Maximum 1000, default 100")
    ),
    responses((status = 200, description = "Saved configuration snapshot metadata", body = serde_json::Value)),
    tag = "Computation"
)]
pub async fn list_configuration_snapshots(
    Extension(registry): Extension<InstanceRegistry>,
    Path(InstancePath { instance_id }): Path<InstancePath>,
    Query(query): Query<SnapshotListQuery>,
) -> Result<
    (
        HeaderMap,
        Json<ApiResponse<Vec<ConfigurationSnapshotSummary>>>,
    ),
    ErrorResponse,
> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let limit = NonZeroUsize::new(query.limit.unwrap_or(100))
        .filter(|value| value.get() <= 1000)
        .ok_or_else(|| {
            ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Snapshot limit must be between 1 and 1000",
            )
        })?;
    let snapshots = core
        .list_configuration_snapshots(query.after, limit)
        .await
        .map_err(management_error)?;
    Ok((no_store(), Json(ApiResponse::success(snapshots))))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/snapshots/{name}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("name" = String, Path, description = "New immutable snapshot name")),
    responses((status = 200, description = "Atomically saved current accepted configuration, not processing data", body = serde_json::Value)),
    tag = "Computation"
)]
pub async fn create_configuration_snapshot(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(SnapshotPath { instance_id, name }): Path<SnapshotPath>,
) -> Result<(HeaderMap, Json<ApiResponse<CommittedConfiguration>>), ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let snapshot = core
        .snapshot_desired_configuration(name)
        .await
        .map_err(management_error)?;
    Ok((no_store(), Json(ApiResponse::success(snapshot))))
}

#[utoipa::path(
    get, path = "/api/v1/instances/{instanceId}/computation/snapshots/{name}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("name" = String, Path, description = "Existing snapshot name")),
    responses((status = 200, description = "Privileged saved configuration; may contain credentials", body = serde_json::Value),
        (status = 404, description = "Snapshot not found", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn get_configuration_snapshot(
    Extension(registry): Extension<InstanceRegistry>,
    Path(SnapshotPath { instance_id, name }): Path<SnapshotPath>,
) -> Result<(HeaderMap, Json<ApiResponse<CommittedConfiguration>>), ErrorResponse> {
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let snapshot = core
        .load_configuration_snapshot(name)
        .await
        .map_err(management_error)?
        .ok_or_else(|| {
            ErrorResponse::new(
                error_codes::COMPUTATION_NOT_FOUND,
                "Configuration snapshot not found",
            )
        })?;
    Ok((no_store(), Json(ApiResponse::success(snapshot))))
}

#[utoipa::path(
    post, path = "/api/v1/instances/{instanceId}/computation/snapshots/{name}/restore",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("name" = String, Path, description = "Actual stored snapshot to restore")),
    request_body = ConfigurationRevisionRequest,
    responses((status = 200, description = "Stored UUIDs and definitions accepted; processing data is not rewound", body = ApiResponse<ConfigurationReceipt>),
        (status = 409, description = "Revision conflict", body = ErrorResponse)),
    tag = "Computation"
)]
pub async fn restore_configuration_snapshot(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(SnapshotPath { instance_id, name }): Path<SnapshotPath>,
    ConfigBody(request): ConfigBody<ConfigurationRevisionRequest>,
) -> Result<Json<ApiResponse<ConfigurationReceipt>>, ErrorResponse> {
    require_writable(*read_only)?;
    validate_request_id(&request.request_id)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let receipt = core
        .restore_configuration_snapshot(name, request.expected_revision, request.request_id)
        .await
        .map_err(management_error)?;
    Ok(Json(ApiResponse::success(receipt.into())))
}

#[utoipa::path(
    delete, path = "/api/v1/instances/{instanceId}/computation/snapshots/{name}",
    params(("instanceId" = String, Path, description = "DrasiLib instance ID"), ("name" = String, Path, description = "Snapshot to delete")),
    request_body = DeleteSnapshotRequest,
    responses((status = 200, description = "Whether the named snapshot was deleted; current state is unchanged", body = serde_json::Value)),
    tag = "Computation"
)]
pub async fn delete_configuration_snapshot(
    Extension(registry): Extension<InstanceRegistry>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(SnapshotPath { instance_id, name }): Path<SnapshotPath>,
    ConfigBody(request): ConfigBody<DeleteSnapshotRequest>,
) -> Result<Json<ApiResponse<bool>>, ErrorResponse> {
    require_writable(*read_only)?;
    let core = get_instance_or_error(&registry, &instance_id).await?;
    require_managed(&core)?;
    let removed = core
        .delete_configuration_snapshot(name, request.expected_revision)
        .await
        .map_err(management_error)?;
    Ok(Json(ApiResponse::success(removed)))
}
