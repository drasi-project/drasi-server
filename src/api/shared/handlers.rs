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

//! Shared handler implementations used across API versions.
//!
//! These handler functions contain the core business logic that can be
//! reused by version-specific handlers. Each API version may wrap these
//! with version-specific path annotations.

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::Json,
};
use indexmap::IndexMap;
use std::sync::Arc;

use super::responses::{
    ApiResponse, ApiVersionsResponse, ComponentListItem, HealthResponse, InstanceListItem,
    StatusResponse,
};
use crate::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};
use crate::api::models::QueryConfigDto;
use crate::config::{ReactionConfig, SourceConfig};
use crate::factories::{create_reaction, create_source};
use crate::persistence::ConfigPersistence;
use drasi_lib::{channels::ComponentStatus, queries::LabelExtractor, QueryConfig};

/// Helper function to persist configuration after a successful operation.
/// Logs errors but does not fail the request - persistence failures are non-fatal.
pub async fn persist_after_operation(
    config_persistence: &Option<Arc<ConfigPersistence>>,
    operation: &str,
) {
    if let Some(persistence) = config_persistence {
        if let Err(e) = persistence.save().await {
            log::error!("Failed to persist configuration after {operation}: {e}");
        }
    }
}

/// List available API versions
pub async fn list_api_versions() -> Json<ApiVersionsResponse> {
    Json(ApiVersionsResponse {
        versions: vec!["v1".to_string()],
        current: "v1".to_string(),
    })
}

/// Check server health
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// List configured DrasiLib instances
pub async fn list_instances(
    Extension(instances): Extension<Arc<IndexMap<String, Arc<drasi_lib::DrasiLib>>>>,
) -> Json<ApiResponse<Vec<InstanceListItem>>> {
    let data = instances
        .keys()
        .cloned()
        .map(|id| InstanceListItem { id })
        .collect();

    Json(ApiResponse::success(data))
}

/// List all sources for an instance
pub async fn list_sources(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let sources = core.list_sources().await.unwrap_or_default();
    let items: Vec<ComponentListItem> = sources
        .into_iter()
        .map(|(id, status)| ComponentListItem { id, status })
        .collect();

    Json(ApiResponse::success(items))
}

/// Create a new source
pub async fn create_source_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create sources.".to_string(),
        )));
    }

    let config: SourceConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to parse source config: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid source configuration: {e}"
            ))));
        }
    };

    let source_id = config.id().to_string();
    let auto_start = config.auto_start();

    let source = match create_source(config.clone()).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create source instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create source: {e}"
            ))));
        }
    };

    match core.add_source(source).await {
        Ok(_) => {
            log::info!("Source '{source_id}' created successfully");

            if let Some(persistence) = &config_persistence {
                persistence.register_source(&instance_id, config).await;
            }

            if auto_start {
                if let Err(e) = core.start_source(&source_id).await {
                    log::warn!("Failed to auto-start source '{source_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "creating source").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Source '{source_id}' created successfully"),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") {
                log::info!("Source '{source_id}' already exists");
                return Ok(Json(ApiResponse::success(StatusResponse {
                    message: format!("Source '{source_id}' already exists"),
                })));
            }
            log::error!("Failed to add source: {e}");
            Ok(Json(ApiResponse::error(error_msg)))
        }
    }
}

/// Get source status by ID
pub async fn get_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    match core.get_source_status(&id).await {
        Ok(status) => Ok(Json(ApiResponse::success(ComponentListItem { id, status }))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a source
pub async fn delete_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete sources.".to_string(),
        )));
    }

    match core.remove_source(&id).await {
        Ok(_) => {
            if let Some(persistence) = &config_persistence {
                persistence.unregister_source(&instance_id, &id).await;
            }

            persist_after_operation(&config_persistence, "deleting source").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Source deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete source: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a source
pub async fn start_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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
pub async fn stop_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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

/// List all queries for an instance
pub async fn list_queries(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let queries = core.list_queries().await.unwrap_or_default();
    let items: Vec<ComponentListItem> = queries
        .into_iter()
        .map(|(id, status)| ComponentListItem { id, status })
        .collect();

    Json(ApiResponse::success(items))
}

/// Create a new query
pub async fn create_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_dto): Json<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create queries.".to_string(),
        )));
    }

    let query_id = config_dto.id.clone();

    // Convert QueryConfigDto to drasi-lib's QueryConfig
    let mapper = DtoMapper::default();
    let query_mapper = QueryConfigMapper;
    let config = match mapper.map_with(&config_dto, &query_mapper) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to convert QueryConfigDto to QueryConfig: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid query configuration: {e}"
            ))));
        }
    };

    // Pre-flight join validation/logging (non-fatal warnings)
    if let Some(joins) = &config.joins {
        if !joins.is_empty() {
            match LabelExtractor::extract_labels(&config.query, &config.query_language) {
                Ok(labels) => {
                    let rel_labels: std::collections::HashSet<String> =
                        labels.relation_labels.into_iter().collect();
                    for j in joins {
                        if !rel_labels.contains(&j.id) {
                            log::warn!("[JOIN-VALIDATION] Query '{query_id}' defines join id '{}' which does not appear as a relationship label in the Cypher pattern.", j.id);
                        }
                        for key in &j.keys {
                            if key.label.trim().is_empty() || key.property.trim().is_empty() {
                                log::warn!("[JOIN-VALIDATION] Query '{query_id}' join '{}' has an empty label or property (label='{}', property='{}').", j.id, key.label, key.property);
                            }
                        }
                    }
                    log::info!(
                        "Registering query '{query_id}' with {} synthetic join(s)",
                        joins.len()
                    );
                }
                Err(e) => {
                    log::warn!(
                        "[JOIN-VALIDATION] Failed to parse query '{query_id}' for join validation: {e}"
                    );
                }
            }
        } else {
            log::debug!("Registering query '{query_id}' with no synthetic joins");
        }
    } else {
        log::debug!("Registering query '{query_id}' with no synthetic joins");
    }

    match core.add_query(config.clone()).await {
        Ok(_) => {
            log::info!("Query '{query_id}' created successfully");

            // Register the QueryConfigDto for persistence
            if let Some(persistence) = &config_persistence {
                persistence.register_query(&instance_id, config_dto).await;
            }

            persist_after_operation(&config_persistence, "creating query").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query created successfully".to_string(),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                log::info!("Query '{query_id}' already exists, skipping creation");
                return Ok(Json(ApiResponse::success(StatusResponse {
                    message: format!("Query '{query_id}' already exists"),
                })));
            }

            log::error!("Failed to create query: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get query by ID
pub async fn get_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<QueryConfig>>, StatusCode> {
    match core.get_query_config(&id).await {
        Ok(config) => Ok(Json(ApiResponse::success(config))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a query
pub async fn delete_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete queries.".to_string(),
        )));
    }

    match core.remove_query(&id).await {
        Ok(_) => {
            // Unregister the query from persistence
            if let Some(persistence) = &config_persistence {
                persistence.unregister_query(&instance_id, &id).await;
            }

            persist_after_operation(&config_persistence, "deleting query").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete query: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a query
pub async fn start_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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
pub async fn stop_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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
pub async fn get_query_results(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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

/// List all reactions for an instance
pub async fn list_reactions(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let reactions = core.list_reactions().await.unwrap_or_default();
    let items: Vec<ComponentListItem> = reactions
        .into_iter()
        .map(|(id, status)| ComponentListItem { id, status })
        .collect();

    Json(ApiResponse::success(items))
}

/// Create a new reaction
pub async fn create_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot create reactions.".to_string(),
        )));
    }

    let config: ReactionConfig = match serde_json::from_value(config_json) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to parse reaction config: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Invalid reaction configuration: {e}"
            ))));
        }
    };

    let reaction_id = config.id().to_string();
    let auto_start = config.auto_start();

    let reaction = match create_reaction(config.clone()) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create reaction instance: {e}");
            return Ok(Json(ApiResponse::error(format!(
                "Failed to create reaction: {e}"
            ))));
        }
    };

    match core.add_reaction(reaction).await {
        Ok(_) => {
            log::info!("Reaction '{reaction_id}' created successfully");

            if let Some(persistence) = &config_persistence {
                persistence.register_reaction(&instance_id, config).await;
            }

            if auto_start {
                if let Err(e) = core.start_reaction(&reaction_id).await {
                    log::warn!("Failed to auto-start reaction '{reaction_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "creating reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Reaction '{reaction_id}' created successfully"),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") {
                log::info!("Reaction '{reaction_id}' already exists");
                return Ok(Json(ApiResponse::success(StatusResponse {
                    message: format!("Reaction '{reaction_id}' already exists"),
                })));
            }
            log::error!("Failed to add reaction: {e}");
            Ok(Json(ApiResponse::error(error_msg)))
        }
    }
}

/// Get reaction status by ID
pub async fn get_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, StatusCode> {
    match core.get_reaction_status(&id).await {
        Ok(status) => Ok(Json(ApiResponse::success(ComponentListItem { id, status }))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a reaction
pub async fn delete_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, StatusCode> {
    if *read_only {
        return Ok(Json(ApiResponse::error(
            "Server is in read-only mode. Cannot delete reactions.".to_string(),
        )));
    }

    match core.remove_reaction(&id).await {
        Ok(_) => {
            if let Some(persistence) = &config_persistence {
                persistence.unregister_reaction(&instance_id, &id).await;
            }

            persist_after_operation(&config_persistence, "deleting reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete reaction: {e}");
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a reaction
pub async fn start_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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
pub async fn stop_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
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
