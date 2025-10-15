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

use drasi_server_core::{
    config::{QueryRuntime, ReactionRuntime, SourceRuntime},
    queries::LabelExtractor, // For join validation
    routers::{BootstrapRouter, DataRouter, SubscriptionRouter},
    ComponentStatus,
    QueryConfig,
    QueryManager,
    ReactionConfig,
    ReactionManager,
    SourceConfig,
    SourceManager,
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
    Extension(source_manager): Extension<Arc<SourceManager>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let sources = source_manager.list_sources().await;
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
    Extension(source_manager): Extension<Arc<SourceManager>>,
    Extension(bootstrap_router): Extension<Arc<BootstrapRouter>>,
    Extension(read_only): Extension<Arc<bool>>,
    Json(config): Json<SourceConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create sources.".to_string(),
        )));
    }

    let source_id = config.id.clone();
    let bootstrap_provider_config = config.bootstrap_provider.clone();

    // Use source manager's add_source which handles auto-start internally
    match source_manager.add_source(config).await {
        Ok(_) => {
            // Register with bootstrap router
            if let Some(source_config) = source_manager.get_source_config(&source_id).await {
                let source_config_arc = Arc::new(source_config);
                let source_change_tx = source_manager.get_source_change_sender();
                if let Err(e) = bootstrap_router
                    .register_provider(
                        source_id.clone(),
                        source_config_arc,
                        bootstrap_provider_config,
                        source_change_tx,
                    )
                    .await
                {
                    log::warn!(
                        "Failed to register bootstrap provider for source '{}': {}",
                        source_id,
                        e
                    );
                } else {
                    log::info!("Registered bootstrap provider for source '{}'", source_id);
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Source created successfully".to_string(),
            })))
        }
        Err(e) => {
            // Check if the source already exists
            if e.to_string().contains("already exists") {
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
    Extension(source_manager): Extension<Arc<SourceManager>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<SourceRuntime>>, StatusCode> {
    match source_manager.get_source(id).await {
        Ok(runtime) => Ok(Json(ApiResponse::success(runtime))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Update an existing source
#[utoipa::path(
    put,
    path = "/sources/{id}",
    params(
        ("id" = String, Path, description = "Source ID")
    ),
    request_body = SourceConfig,
    responses(
        (status = 200, description = "Source updated successfully", body = ApiResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn update_source(
    Extension(source_manager): Extension<Arc<SourceManager>>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(id): Path<String>,
    Json(config): Json<SourceConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot update sources.".to_string(),
        )));
    }

    if config.id != id {
        return Err(StatusCode::BAD_REQUEST);
    }

    match source_manager.update_source(id, config).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source updated successfully".to_string(),
        }))),
        Err(e) => {
            log::error!("Failed to update source: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
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
    Extension(source_manager): Extension<Arc<SourceManager>>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete sources.".to_string(),
        )));
    }

    match source_manager.delete_source(id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source deleted successfully".to_string(),
        }))),
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
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn start_source(
    Extension(source_manager): Extension<Arc<SourceManager>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match source_manager.start_source(id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source started successfully".to_string(),
        }))),
        Err(e) => {
            log::error!("Failed to start source: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
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
        (status = 500, description = "Internal server error"),
    ),
    tag = "Sources"
)]
pub async fn stop_source(
    Extension(source_manager): Extension<Arc<SourceManager>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match source_manager.stop_source(id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source stopped successfully".to_string(),
        }))),
        Err(e) => {
            log::error!("Failed to stop source: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
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
    Extension(query_manager): Extension<Arc<QueryManager>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let queries = query_manager.list_queries().await;
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
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Extension(data_router): Extension<Arc<DataRouter>>,
    Extension(bootstrap_router): Extension<Arc<BootstrapRouter>>,
    Extension(read_only): Extension<Arc<bool>>,
    Json(config): Json<QueryConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create queries.".to_string(),
        )));
    }

    let query_id = config.id.clone();
    let should_auto_start = config.auto_start;
    let sources = config.sources.clone();
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

    match query_manager.add_query(config).await {
        Ok(_) => {
            // Register with bootstrap router
            let bootstrap_senders = query_manager.get_bootstrap_response_senders().await;
            if let Some(sender) = bootstrap_senders.get(&query_id) {
                bootstrap_router
                    .register_query(query_id.clone(), sender.clone())
                    .await;
                log::info!("Registered query '{}' with bootstrap router", query_id);
            }

            // Auto-start if configured
            if should_auto_start {
                log::info!("Auto-starting query: {}", query_id);
                let rx = data_router
                    .add_query_subscription(query_id.clone(), sources)
                    .await;

                if let Err(e) = query_manager.start_query(query_id.clone(), rx).await {
                    log::error!("Failed to auto-start query {}: {}", query_id, e);
                    // Don't fail the add operation, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query created successfully".to_string(),
            })))
        }
        Err(e) => {
            // Check if the query already exists
            if e.to_string().contains("already exists") {
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
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<QueryRuntime>>, StatusCode> {
    match query_manager.get_query(id).await {
        Ok(runtime) => Ok(Json(ApiResponse::success(runtime))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Update an existing query
#[utoipa::path(
    put,
    path = "/queries/{id}",
    params(
        ("id" = String, Path, description = "Query ID")
    ),
    request_body = QueryConfig,
    responses(
        (status = 200, description = "Query updated successfully", body = ApiResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn update_query(
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(id): Path<String>,
    Json(config): Json<QueryConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot update queries.".to_string(),
        )));
    }

    if config.id != id {
        return Err(StatusCode::BAD_REQUEST);
    }

    match query_manager.update_query(id, config).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query updated successfully".to_string(),
        }))),
        Err(e) => {
            log::error!("Failed to update query: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
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
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Extension(data_router): Extension<Arc<DataRouter>>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete queries.".to_string(),
        )));
    }

    match query_manager.delete_query(id.clone()).await {
        Ok(_) => {
            // Remove the query's subscription from the data router
            data_router.remove_query_subscription(&id).await;

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
    ),
    tag = "Queries"
)]
pub async fn start_query(
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Extension(data_router): Extension<Arc<DataRouter>>,
    Path(id): Path<String>,
) -> Json<ApiResponse<StatusResponse>> {
    // Get the query to retrieve its source configuration
    match query_manager.get_query(id.clone()).await {
        Ok(runtime) => {
            // Check if query is already running
            if matches!(runtime.status, ComponentStatus::Running) {
                return Json(ApiResponse::error(
                    "Component is already running".to_string(),
                ));
            }

            // Get a receiver connected to the data router
            let rx = data_router
                .add_query_subscription(id.clone(), runtime.sources.clone())
                .await;

            // Start the query with the receiver
            match query_manager.start_query(id.clone(), rx).await {
                Ok(_) => Json(ApiResponse::success(StatusResponse {
                    message: "Query started successfully".to_string(),
                })),
                Err(e) => Json(ApiResponse::error(e.to_string())),
            }
        }
        Err(_) => Json(ApiResponse::error(format!("Query '{}' not found", id))),
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
        (status = 500, description = "Internal server error"),
    ),
    tag = "Queries"
)]
pub async fn stop_query(
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Extension(data_router): Extension<Arc<DataRouter>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match query_manager.stop_query(id.clone()).await {
        Ok(_) => {
            // Remove the query's subscription from the data router
            data_router.remove_query_subscription(&id).await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query stopped successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to stop query: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
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
    Extension(query_manager): Extension<Arc<QueryManager>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, StatusCode> {
    match query_manager.get_query_results(&id).await {
        Ok(results) => Ok(Json(ApiResponse::success(results))),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else if e.to_string().contains("not running") {
                Ok(Json(ApiResponse::error(e.to_string())))
            } else {
                Err(StatusCode::INTERNAL_SERVER_ERROR)
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
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let reactions = reaction_manager.list_reactions().await;
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
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
    Extension(subscription_router): Extension<Arc<SubscriptionRouter>>,
    Extension(read_only): Extension<Arc<bool>>,
    Json(config): Json<ReactionConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create reactions.".to_string(),
        )));
    }

    let reaction_id = config.id.clone();
    let should_auto_start = config.auto_start;
    let queries = config.queries.clone();

    match reaction_manager.add_reaction(config).await {
        Ok(_) => {
            // Auto-start if configured
            if should_auto_start {
                log::info!("Auto-starting reaction: {}", reaction_id);
                let rx = subscription_router
                    .add_reaction_subscription(reaction_id.clone(), queries)
                    .await;

                if let Err(e) = reaction_manager
                    .start_reaction(reaction_id.clone(), rx)
                    .await
                {
                    log::error!("Failed to auto-start reaction {}: {}", reaction_id, e);
                    // Don't fail the add operation, just log the error
                }
            }

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction created successfully".to_string(),
            })))
        }
        Err(e) => {
            // Check if the reaction already exists
            if e.to_string().contains("already exists") {
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
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ReactionRuntime>>, StatusCode> {
    match reaction_manager.get_reaction(id).await {
        Ok(runtime) => Ok(Json(ApiResponse::success(runtime))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Update an existing reaction
#[utoipa::path(
    put,
    path = "/reactions/{id}",
    params(
        ("id" = String, Path, description = "Reaction ID")
    ),
    request_body = ReactionConfig,
    responses(
        (status = 200, description = "Reaction updated successfully", body = ApiResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn update_reaction(
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(id): Path<String>,
    Json(config): Json<ReactionConfig>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot update reactions.".to_string(),
        )));
    }

    if config.id != id {
        return Err(StatusCode::BAD_REQUEST);
    }

    match reaction_manager.update_reaction(id, config).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction updated successfully".to_string(),
        }))),
        Err(e) => {
            log::error!("Failed to update reaction: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
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
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
    Extension(subscription_router): Extension<Arc<SubscriptionRouter>>,
    Extension(read_only): Extension<Arc<bool>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete reactions.".to_string(),
        )));
    }

    match reaction_manager.delete_reaction(id.clone()).await {
        Ok(_) => {
            // Remove the reaction's subscription from the subscription router
            subscription_router.remove_reaction_subscription(&id).await;

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
    ),
    tag = "Reactions"
)]
pub async fn start_reaction(
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
    Extension(subscription_router): Extension<Arc<SubscriptionRouter>>,
    Path(id): Path<String>,
) -> Json<ApiResponse<StatusResponse>> {
    // Get the reaction to retrieve its query configuration
    match reaction_manager.get_reaction(id.clone()).await {
        Ok(runtime) => {
            // Check if the reaction is already running
            if runtime.status == ComponentStatus::Running {
                log::info!("Reaction '{}' is already running", id);
                return Json(ApiResponse::error(
                    "Component is already running".to_string(),
                ));
            }

            // Get a receiver connected to the subscription router
            let rx = subscription_router
                .add_reaction_subscription(id.clone(), runtime.queries.clone())
                .await;

            // Start the reaction with the receiver
            match reaction_manager.start_reaction(id.clone(), rx).await {
                Ok(_) => Json(ApiResponse::success(StatusResponse {
                    message: "Reaction started successfully".to_string(),
                })),
                Err(e) => Json(ApiResponse::error(e.to_string())),
            }
        }
        Err(_) => Json(ApiResponse::error(format!("Reaction '{}' not found", id))),
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
        (status = 500, description = "Internal server error"),
    ),
    tag = "Reactions"
)]
pub async fn stop_reaction(
    Extension(reaction_manager): Extension<Arc<ReactionManager>>,
    Extension(subscription_router): Extension<Arc<SubscriptionRouter>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    match reaction_manager.stop_reaction(id.clone()).await {
        Ok(_) => {
            // Remove the reaction's subscription from the subscription router
            subscription_router.remove_reaction_subscription(&id).await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction stopped successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to stop reaction: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}
