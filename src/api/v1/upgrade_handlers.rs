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

//! REST API handlers for plugin upgrade lifecycle.
//!
//! Endpoints:
//! - `POST /api/v1/plugins/upgrades/plan` — Plan an upgrade
//! - `POST /api/v1/plugins/upgrades/{planId}/execute` — Execute a planned upgrade
//! - `GET /api/v1/plugins/upgrades` — List all upgrade plans
//! - `GET /api/v1/plugins/upgrades/{planId}` — Get specific plan status
//! - `POST /api/v1/plugins/upgrades/{planId}/rollback` — Rollback an upgrade
//! - `DELETE /api/v1/plugins/upgrades/{planId}` — Cancel a planned upgrade

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::api::shared::error::ErrorResponse;
use crate::upgrade::{UpgradeEngine, UpgradeError};

// ── Request/Response DTOs ────────────────────────────────────────────────

/// Request body for POST /api/v1/plugins/upgrades/plan
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanUpgradeRequest {
    /// Plugin kind to upgrade (e.g., "source/postgres").
    pub plugin_kind: String,
    /// Path to the new plugin binary on the server filesystem.
    pub binary_path: String,
}

/// Request body for POST /api/v1/plugins/upgrades/{planId}/execute
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteUpgradeRequest {
    /// Maximum concurrent component migrations (reserved for future use).
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
}

fn default_max_concurrent() -> usize {
    1
}

/// Response wrapping an upgrade plan.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePlanResponse {
    pub plan: serde_json::Value,
}

/// Response for listing upgrade plans.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePlanListResponse {
    pub upgrades: Vec<serde_json::Value>,
}

// ── Handlers ─────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/plugins/upgrades/plan",
    tag = "Plugin Upgrades",
    request_body = PlanUpgradeRequest,
    responses(
        (status = 200, description = "Upgrade plan created", body = UpgradePlanResponse),
        (status = 400, description = "ABI incompatible or invalid request"),
        (status = 404, description = "Plugin not loaded"),
        (status = 409, description = "Upgrade already in progress")
    )
)]
/// Plan a rolling plugin upgrade.
///
/// Validates ABI compatibility, finds all dependent components, and creates
/// an upgrade plan in `Planned` state. Does not execute — call `/execute` to start.
pub async fn plan_upgrade(
    Extension(engine): Extension<Arc<UpgradeEngine>>,
    Json(body): Json<PlanUpgradeRequest>,
) -> impl IntoResponse {
    let binary_path = PathBuf::from(&body.binary_path);

    match engine.plan_upgrade(&body.plugin_kind, &binary_path).await {
        Ok(plan) => {
            let plan_json = serde_json::to_value(&plan).unwrap_or_default();
            (StatusCode::OK, Json(serde_json::json!({ "plan": plan_json }))).into_response()
        }
        Err(e) => upgrade_error_to_response(e).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/upgrades/{planId}/execute",
    tag = "Plugin Upgrades",
    params(
        ("planId" = String, Path, description = "Upgrade plan ID")
    ),
    responses(
        (status = 202, description = "Upgrade execution started", body = UpgradePlanResponse),
        (status = 404, description = "Plan not found"),
        (status = 409, description = "Plan not in executable state")
    )
)]
/// Execute a planned upgrade using rolling migration.
///
/// Components are upgraded one-at-a-time. On failure, automatic rollback is triggered.
pub async fn execute_upgrade(
    Extension(engine): Extension<Arc<UpgradeEngine>>,
    Path(plan_id): Path<String>,
    Json(_body): Json<Option<ExecuteUpgradeRequest>>,
) -> impl IntoResponse {
    match engine.execute_upgrade(&plan_id).await {
        Ok(plan) => {
            let plan_json = serde_json::to_value(&plan).unwrap_or_default();
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({ "plan": plan_json })),
            )
                .into_response()
        }
        Err(e) => upgrade_error_to_response(e).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/upgrades",
    tag = "Plugin Upgrades",
    responses(
        (status = 200, description = "List of upgrade plans", body = UpgradePlanListResponse)
    )
)]
/// List all upgrade plans (active and historical).
pub async fn list_upgrades(
    Extension(engine): Extension<Arc<UpgradeEngine>>,
) -> impl IntoResponse {
    let plans = engine.list_plans().await;
    let plans_json: Vec<serde_json::Value> = plans
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "upgrades": plans_json })),
    )
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/upgrades/{planId}",
    tag = "Plugin Upgrades",
    params(
        ("planId" = String, Path, description = "Upgrade plan ID")
    ),
    responses(
        (status = 200, description = "Upgrade plan details", body = UpgradePlanResponse),
        (status = 404, description = "Plan not found")
    )
)]
/// Get the status and details of a specific upgrade plan.
pub async fn get_upgrade(
    Extension(engine): Extension<Arc<UpgradeEngine>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    match engine.get_plan(&plan_id).await {
        Some(plan) => {
            let plan_json = serde_json::to_value(&plan).unwrap_or_default();
            (StatusCode::OK, Json(serde_json::json!({ "plan": plan_json }))).into_response()
        }
        None => ErrorResponse::new(
            "UPGRADE_PLAN_NOT_FOUND",
            format!("Upgrade plan '{plan_id}' not found"),
        )
        .into_json_response()
        .into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/upgrades/{planId}/rollback",
    tag = "Plugin Upgrades",
    params(
        ("planId" = String, Path, description = "Upgrade plan ID")
    ),
    responses(
        (status = 202, description = "Rollback initiated", body = UpgradePlanResponse),
        (status = 404, description = "Plan not found"),
        (status = 409, description = "Plan not in rollback-eligible state")
    )
)]
/// Rollback an upgrade, restoring components to the previous version.
pub async fn rollback_upgrade(
    Extension(engine): Extension<Arc<UpgradeEngine>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    match engine.rollback_upgrade(&plan_id).await {
        Ok(plan) => {
            let plan_json = serde_json::to_value(&plan).unwrap_or_default();
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({ "plan": plan_json })),
            )
                .into_response()
        }
        Err(e) => upgrade_error_to_response(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/plugins/upgrades/{planId}",
    tag = "Plugin Upgrades",
    params(
        ("planId" = String, Path, description = "Upgrade plan ID")
    ),
    responses(
        (status = 204, description = "Plan cancelled"),
        (status = 404, description = "Plan not found"),
        (status = 409, description = "Cannot cancel — already executing")
    )
)]
/// Cancel a planned upgrade that hasn't started executing.
pub async fn cancel_upgrade(
    Extension(engine): Extension<Arc<UpgradeEngine>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    match engine.cancel_upgrade(&plan_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => upgrade_error_to_response(e).into_response(),
    }
}

// ── Error mapping ────────────────────────────────────────────────────────

/// Map UpgradeError to appropriate HTTP response.
fn upgrade_error_to_response(err: UpgradeError) -> impl IntoResponse {
    let (status, code) = match &err {
        UpgradeError::PluginNotLoaded { .. } => {
            (StatusCode::NOT_FOUND, "UPGRADE_PLUGIN_NOT_FOUND")
        }
        UpgradeError::PlanNotFound { .. } => {
            (StatusCode::NOT_FOUND, "UPGRADE_PLAN_NOT_FOUND")
        }
        UpgradeError::AbiMismatch { .. } => (StatusCode::BAD_REQUEST, "UPGRADE_ABI_MISMATCH"),
        UpgradeError::TargetMismatch { .. } => {
            (StatusCode::BAD_REQUEST, "UPGRADE_TARGET_MISMATCH")
        }
        UpgradeError::UpgradeAlreadyInProgress { .. } => {
            (StatusCode::CONFLICT, "UPGRADE_ALREADY_IN_PROGRESS")
        }
        UpgradeError::InvalidPlanState { .. } => {
            (StatusCode::CONFLICT, "UPGRADE_INVALID_STATE")
        }
        UpgradeError::CannotCancel { .. } => (StatusCode::CONFLICT, "UPGRADE_CANNOT_CANCEL"),
        UpgradeError::LoadFailed { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, "UPGRADE_LOAD_FAILED")
        }
        UpgradeError::ComponentFailed { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, "UPGRADE_COMPONENT_FAILED")
        }
        UpgradeError::OldPluginNotAvailable { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, "UPGRADE_ROLLBACK_UNAVAILABLE")
        }
        UpgradeError::DependentLookupFailed { .. } => {
            (StatusCode::BAD_REQUEST, "UPGRADE_NO_DEPENDENTS")
        }
        UpgradeError::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "UPGRADE_INTERNAL_ERROR")
        }
    };

    (
        status,
        Json(serde_json::json!({
            "code": code,
            "message": err.to_string(),
        })),
    )
}

/// Build the upgrade routes sub-router.
///
/// These routes are nested under `/api/v1/plugins/upgrades`.
pub fn upgrade_routes() -> axum::Router {
    axum::Router::new()
        .route("/plan", axum::routing::post(plan_upgrade))
        .route("/", axum::routing::get(list_upgrades))
        .route("/:plan_id", axum::routing::get(get_upgrade))
        .route("/:plan_id/execute", axum::routing::post(execute_upgrade))
        .route("/:plan_id/rollback", axum::routing::post(rollback_upgrade))
        .route("/:plan_id", axum::routing::delete(cancel_upgrade))
}
