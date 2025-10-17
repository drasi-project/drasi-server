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

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::Json,
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::persistence::ConfigPersistence;
use drasi_server_core::{
    // Internal types (doc-hidden but accessible)
    channels::ComponentStatus,
    queries::LabelExtractor, // For join validation
    // Public config types
    QueryConfig,
    ReactionConfig,
    SourceConfig,
};

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    /// Health status of the server
    status: String,
    /// Current server timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct ComponentListItem {
    /// ID of the component
    id: String,
    /// Current status of the component
    status: ComponentStatus,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    /// Whether the request was successful
    success: bool,
    /// Response data if successful
    data: Option<T>,
    /// Error message if unsuccessful
    error: Option<String>,
}

/// Generic API Response schema for OpenAPI documentation
#[derive(Serialize, ToSchema)]
#[schema(as = ApiResponse)]
pub struct ApiResponseSchema {
    /// Whether the request was successful
    success: bool,
    /// Response data if successful
    data: Option<serde_json::Value>,
    /// Error message if unsuccessful
    error: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct StatusResponse {
    /// Status message
    message: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
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
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// List all sources
#[utoipa::path(
    get,
    path = "/sources",
    responses(
        (status = 200, description = "List of sources", body = ApiResponse),
    ),
    tag = "Sources"
)]
pub async fn list_sources(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let sources = core.list_sources().await.unwrap_or_default();
    let items: Vec<ComponentListItem> = sources
        .into_iter()
        .map(|(id, status)| ComponentListItem { id, status })
        .collect();

    Json(ApiResponse::success(items))
}

/// Create a new source
#[utoipa::path(
    post,
    path = "/sources",
    request_body = SourceConfig,
    responses(
        (status = 200, description = "Source created successfully", body = ApiResponse),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn create_source(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config): Json<SourceConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create sources.".to_string(),
        )));
    }

    let source_id = config.id.clone();

    // Use DrasiServerCore's public API to add source at runtime
    match core.add_source_runtime(config.clone()).await {
        Ok(_) => {
            log::info!("Source '{}' created successfully", source_id);

            // Save configuration if persistence is enabled
            if let Some(persistence) = config_persistence {
                if let Err(e) = persistence.save().await {
                    log::error!("Failed to persist configuration after creating source: {}", e);
                    // Don't fail the request, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Source created successfully".to_string(),
            })))
        }
        Err(e) => {
            // Check if the source already exists
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                log::info!("Source '{}' already exists, skipping creation", source_id);
                // Return success since the source exists (idempotent behavior)
                return Ok(Json(ApiResponse::success(StatusResponse {
                    message: format!("Source '{}' already exists", source_id),
                })));
            }

            log::error!("Failed to create source: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get source by name
#[utoipa::path(
    get,
    path = "/sources/{id}",
    params(
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "Source found", body = ApiResponse),
        (status = 404, description = "Source not found"),
    ),
    tag = "Sources"
)]
pub async fn get_source(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<SourceConfig>>, StatusCode> {
    match core.get_source_config(&id).await {
        Ok(config) => Ok(Json(ApiResponse::success(config))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a source
#[utoipa::path(
    delete,
    path = "/sources/{id}",
    params(
        ("id" = String, Path, description = "Source ID")
    ),
    responses(
        (status = 200, description = "Source deleted successfully", body = ApiResponse),
    ),
    tag = "Sources"
)]
pub async fn delete_source(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete sources.".to_string(),
        )));
    }

    match core.remove_source(&id).await {
        Ok(_) => {
            // Save configuration if persistence is enabled
            if let Some(persistence) = config_persistence {
                if let Err(e) = persistence.save().await {
                    log::error!("Failed to persist configuration after deleting source: {}", e);
                    // Don't fail the request, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Source deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete source: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a source
#[utoipa::path(
    post,
    path = "/sources/{id}/start",
    params(
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
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.start_source(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source started successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Stop a source
#[utoipa::path(
    post,
    path = "/sources/{id}/stop",
    params(
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
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.stop_source(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source stopped successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

// Query endpoints
/// List all queries
#[utoipa::path(
    get,
    path = "/queries",
    responses(
        (status = 200, description = "List of queries", body = ApiResponse),
    ),
    tag = "Queries"
)]
pub async fn list_queries(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let queries = core.list_queries().await.unwrap_or_default();
    let items: Vec<ComponentListItem> = queries
        .into_iter()
        .map(|(id, status)| ComponentListItem { id, status })
        .collect();

    Json(ApiResponse::success(items))
}

/// Create a new query
#[utoipa::path(
    post,
    path = "/queries",
    request_body = QueryConfig,
    responses(
        (status = 200, description = "Query created successfully", body = ApiResponse),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn create_query(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config): Json<QueryConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create queries.".to_string(),
        )));
    }

    let query_id = config.id.clone();
    let join_count = config.joins.as_ref().map(|j| j.len()).unwrap_or(0);

    // Pre-flight join validation/logging (non-fatal warnings)
    if join_count > 0 {
        match LabelExtractor::extract_labels(&config.query, &config.query_language) {
            Ok(labels) => {
                let rel_labels: std::collections::HashSet<String> =
                    labels.relation_labels.into_iter().collect();
                for j in config.joins.as_ref().unwrap() {
                    if !rel_labels.contains(&j.id) {
                        log::warn!("[JOIN-VALIDATION] Query '{}' defines join id '{}' which does not appear as a relationship label in the Cypher pattern.", query_id, j.id);
                    }
                    for key in &j.keys {
                        if key.label.trim().is_empty() || key.property.trim().is_empty() {
                            log::warn!("[JOIN-VALIDATION] Query '{}' join '{}' has an empty label or property (label='{}', property='{}').", query_id, j.id, key.label, key.property);
                        }
                    }
                }
                log::info!(
                    "Registering query '{}' with {} synthetic join(s)",
                    query_id,
                    join_count
                );
            }
            Err(e) => {
                log::warn!(
                    "[JOIN-VALIDATION] Failed to parse query '{}' for join validation: {}",
                    query_id,
                    e
                );
            }
        }
    } else {
        log::debug!("Registering query '{}' with no synthetic joins", query_id);
    }

    // Use DrasiServerCore's public API to add query at runtime
    match core.add_query_runtime(config.clone()).await {
        Ok(_) => {
            log::info!("Query '{}' created successfully", query_id);

            // Save configuration if persistence is enabled
            if let Some(persistence) = config_persistence {
                if let Err(e) = persistence.save().await {
                    log::error!("Failed to persist configuration after creating query: {}", e);
                    // Don't fail the request, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query created successfully".to_string(),
            })))
        }
        Err(e) => {
            // Check if the query already exists
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                log::info!("Query '{}' already exists, skipping creation", query_id);
                // Return success since the query exists (idempotent behavior)
                return Ok(Json(ApiResponse::success(StatusResponse {
                    message: format!("Query '{}' already exists", query_id),
                })));
            }

            log::error!("Failed to create query: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get query by name
#[utoipa::path(
    get,
    path = "/queries/{id}",
    params(
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query found", body = ApiResponse),
        (status = 404, description = "Query not found"),
    ),
    tag = "Queries"
)]
pub async fn get_query(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<QueryConfig>>, StatusCode> {
    match core.get_query_config(&id).await {
        Ok(config) => Ok(Json(ApiResponse::success(config))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a query
#[utoipa::path(
    delete,
    path = "/queries/{id}",
    params(
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query deleted successfully", body = ApiResponse),
    ),
    tag = "Queries"
)]
pub async fn delete_query(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete queries.".to_string(),
        )));
    }

    match core.remove_query(&id).await {
        Ok(_) => {
            // Save configuration if persistence is enabled
            if let Some(persistence) = config_persistence {
                if let Err(e) = persistence.save().await {
                    log::error!("Failed to persist configuration after deleting query: {}", e);
                    // Don't fail the request, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete query: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a query
#[utoipa::path(
    post,
    path = "/queries/{id}/start",
    params(
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query started successfully", body = ApiResponse),
        (status = 404, description = "Query not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn start_query(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.start_query(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query started successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Stop a query
#[utoipa::path(
    post,
    path = "/queries/{id}/stop",
    params(
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Query stopped successfully", body = ApiResponse),
        (status = 404, description = "Query not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn stop_query(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.stop_query(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query stopped successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Get current results of a query
#[utoipa::path(
    get,
    path = "/queries/{id}/results",
    params(
        ("id" = String, Path, description = "Query ID")
    ),
    responses(
        (status = 200, description = "Current query results", body = ApiResponse<Vec<serde_json::Value>>),
        (status = 404, description = "Query not found"),
        (status = 400, description = "Query is not running"),
    ),
    tag = "Queries"
)]
pub async fn get_query_results(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, StatusCode> {
    match core.get_query_results(&id).await {
        Ok(results) => Ok(Json(ApiResponse::success(results))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

// Reaction endpoints
/// List all reactions
#[utoipa::path(
    get,
    path = "/reactions",
    responses(
        (status = 200, description = "List of reactions", body = ApiResponse),
    ),
    tag = "Reactions"
)]
pub async fn list_reactions(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let reactions = core.list_reactions().await.unwrap_or_default();
    let items: Vec<ComponentListItem> = reactions
        .into_iter()
        .map(|(id, status)| ComponentListItem { id, status })
        .collect();

    Json(ApiResponse::success(items))
}

/// Create a new reaction
#[utoipa::path(
    post,
    path = "/reactions",
    request_body = ReactionConfig,
    responses(
        (status = 200, description = "Reaction created successfully", body = ApiResponse),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn create_reaction(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Json(config): Json<ReactionConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create reactions.".to_string(),
        )));
    }

    let reaction_id = config.id.clone();

    // Use DrasiServerCore's public API to add reaction at runtime
    match core.add_reaction_runtime(config.clone()).await {
        Ok(_) => {
            log::info!("Reaction '{}' created successfully", reaction_id);

            // Save configuration if persistence is enabled
            if let Some(persistence) = config_persistence {
                if let Err(e) = persistence.save().await {
                    log::error!("Failed to persist configuration after creating reaction: {}", e);
                    // Don't fail the request, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction created successfully".to_string(),
            })))
        }
        Err(e) => {
            // Check if the reaction already exists
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                log::info!(
                    "Reaction '{}' already exists, skipping creation",
                    reaction_id
                );
                // Return success since the reaction exists (idempotent behavior)
                return Ok(Json(ApiResponse::success(StatusResponse {
                    message: format!("Reaction '{}' already exists", reaction_id),
                })));
            }

            log::error!("Failed to create reaction: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get reaction by name
#[utoipa::path(
    get,
    path = "/reactions/{id}",
    params(
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction found", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
    ),
    tag = "Reactions"
)]
pub async fn get_reaction(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ReactionConfig>>, StatusCode> {
    match core.get_reaction_config(&id).await {
        Ok(config) => Ok(Json(ApiResponse::success(config))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a reaction
#[utoipa::path(
    delete,
    path = "/reactions/{id}",
    params(
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction deleted successfully", body = ApiResponse),
    ),
    tag = "Reactions"
)]
pub async fn delete_reaction(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete reactions.".to_string(),
        )));
    }

    match core.remove_reaction(&id).await {
        Ok(_) => {
            // Save configuration if persistence is enabled
            if let Some(persistence) = config_persistence {
                if let Err(e) = persistence.save().await {
                    log::error!("Failed to persist configuration after deleting reaction: {}", e);
                    // Don't fail the request, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete reaction: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a reaction
#[utoipa::path(
    post,
    path = "/reactions/{id}/start",
    params(
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction started successfully", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn start_reaction(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.start_reaction(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction started successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}

/// Stop a reaction
#[utoipa::path(
    post,
    path = "/reactions/{id}/stop",
    params(
        ("id" = String, Path, description = "Reaction ID")
    ),
    responses(
        (status = 200, description = "Reaction stopped successfully", body = ApiResponse),
        (status = 404, description = "Reaction not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn stop_reaction(
    Extension(core): Extension<Arc<drasi_server_core::DrasiServerCore>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match core.stop_reaction(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction stopped successfully".to_string(),
        }))),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(Json(ApiResponse::error(error_msg)))
            }
        }
    }
}
