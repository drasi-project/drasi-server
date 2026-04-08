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
    extract::{Extension, Path, Query},
    response::{
        sse::{Event, Sse},
        Json,
    },
};
use std::convert::Infallible;
use std::sync::Arc;

use super::{
    apply_limit, component_links, persist_after_operation, sse_event_async, ComponentViewQuery,
    ObservabilityQuery,
};
use crate::api::mappings::{DtoMapper, QueryConfigMapper};
use crate::api::models::{ComponentEventDto, LogMessageDto, QueryConfigDto};
use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::api::shared::responses::{ApiResponse, ComponentListItem, StatusResponse};
use crate::persistence::ConfigPersistence;
use drasi_lib::{channels::ComponentStatus, queries::LabelExtractor};
use drasi_reaction_application::subscription::SubscriptionOptions;
use drasi_reaction_application::ApplicationReaction;
use futures_util::{stream, StreamExt};
use tokio::sync::broadcast;
use uuid::Uuid;

/// List all queries for an instance
pub async fn list_queries(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, ErrorResponse> {
    let queries = core.list_queries().await.map_err(ErrorResponse::from)?;
    let mut items = Vec::with_capacity(queries.len());
    for (id, status) in queries {
        let links = component_links(&instance_id, "queries", &id);
        let error_message = if matches!(status, ComponentStatus::Error) {
            match core.get_query_info(&id).await {
                Ok(info) => info.error_message,
                Err(e) => {
                    log::warn!("Failed to fetch query info for '{id}': {e}");
                    None
                }
            }
        } else {
            None
        };
        items.push(ComponentListItem {
            id,
            status,
            error_message,
            links,
            config: None,
        });
    }

    Ok(Json(ApiResponse::success(items)))
}

/// Create a new query
pub async fn create_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Json(config_dto): Json<QueryConfigDto>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create queries.",
        ));
    }

    let query_id = config_dto.id.clone();

    // Convert QueryConfigDto to drasi-lib's QueryConfig
    let mapper = DtoMapper::default();
    let query_mapper = QueryConfigMapper;
    let config = mapper.map_with(&config_dto, &query_mapper).map_err(|e| {
        log::error!("Failed to convert QueryConfigDto to QueryConfig: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid query configuration: {e}"),
        )
    })?;

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

            persist_after_operation(&config_persistence, "creating query").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query created successfully".to_string(),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                log::info!("Query '{query_id}' already exists");
                return Err(ErrorResponse::new(
                    error_codes::DUPLICATE_RESOURCE,
                    "Resource already exists",
                ));
            }

            log::error!("Failed to create query: {e}");
            Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                "Internal server error",
            ))
        }
    }
}

/// Get query by ID
pub async fn get_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(_config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    match core.get_query_config(&id).await {
        Ok(query_config) => {
            let config = if view.include_config() {
                let dto = QueryConfigDto::try_from(query_config.clone()).map_err(|e| {
                    log::error!("Failed to serialize query config: {e}");
                    ErrorResponse::new(error_codes::INTERNAL_ERROR, "Internal server error")
                })?;
                match serde_json::to_value(dto) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        log::error!("Failed to serialize query config: {e}");
                        return Err(ErrorResponse::new(
                            error_codes::INTERNAL_ERROR,
                            "Internal server error",
                        ));
                    }
                }
            } else {
                None
            };
            let status = core
                .get_query_status(&query_config.id)
                .await
                .unwrap_or(ComponentStatus::Error);
            let error_message = if let Ok(info) = core.get_query_info(&query_config.id).await {
                info.error_message
            } else {
                None
            };
            Ok(Json(ApiResponse::success(ComponentListItem {
                id: query_config.id.clone(),
                status,
                error_message,
                links: component_links(&instance_id, "queries", &id),
                config,
            })))
        }
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Get query lifecycle events (snapshot).
pub async fn get_query_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    core.get_query_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let events = core
        .get_query_events(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let collected = events
        .map(ComponentEventDto::from)
        .collect::<Vec<_>>()
        .await;
    let data = apply_limit(collected, query.limit);
    Ok(Json(ApiResponse::success(data)))
}

/// Stream query lifecycle events as SSE.
pub async fn stream_query_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_query_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, receiver) = core
        .subscribe_query_events(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let history_stream =
        stream::iter(history.into_iter().map(ComponentEventDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => return Some((ComponentEventDto::from(event), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Get query logs (snapshot).
pub async fn get_query_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, ErrorResponse> {
    core.get_query_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, _) = core
        .subscribe_query_logs(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let data = apply_limit(
        history.into_iter().map(LogMessageDto::from).collect(),
        query.limit,
    );
    Ok(Json(ApiResponse::success(data)))
}

/// Stream query logs as SSE.
pub async fn stream_query_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_query_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, receiver) = core
        .subscribe_query_logs(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let history_stream =
        stream::iter(history.into_iter().map(LogMessageDto::from)).filter_map(sse_event_async);
    let live_stream = stream::unfold(receiver, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(message) => return Some((LogMessageDto::from(message), receiver)),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    })
    .filter_map(sse_event_async);
    let stream = history_stream.chain(live_stream);
    Ok(Sse::new(stream))
}

/// Delete a query
pub async fn delete_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot delete queries.",
        ));
    }

    match core.remove_query(&id).await {
        Ok(_) => {
            persist_after_operation(&config_persistence, "deleting query").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Query deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete query: {e}");
            Err(ErrorResponse::new(
                error_codes::QUERY_DELETE_FAILED,
                e.to_string(),
            ))
        }
    }
}

/// Start a query
pub async fn start_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    match core.start_query(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query started successfully".to_string(),
        }))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Stop a query
pub async fn stop_query(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    match core.stop_query(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Query stopped successfully".to_string(),
        }))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Get current results of a query
pub async fn get_query_results(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ErrorResponse> {
    match core.get_query_results(&id).await {
        Ok(results) => Ok(Json(ApiResponse::success(results))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Attach to a running query and stream results as NDJSON.
pub async fn attach_query_stream(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_query_config(&id)
        .await
        .map_err(ErrorResponse::from)?;

    let reaction_id = format!("__attach_{}_{}", id, Uuid::new_v4());
    let (reaction, handle) = ApplicationReaction::new(reaction_id.clone(), vec![id.clone()]);
    if let Err(e) = core.add_reaction(reaction).await {
        return Err(ErrorResponse::new(
            error_codes::INTERNAL_ERROR,
            format!("Failed to add attach reaction: {e}"),
        ));
    }

    if let Err(e) = core.start_reaction(&reaction_id).await {
        let error_msg = e.to_string();
        if !error_msg.contains("already running") {
            let _ = core.remove_reaction(&reaction_id, true).await;
            return Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to start attach reaction: {error_msg}"),
            ));
        }
    }

    let options = SubscriptionOptions::default().with_query_filter(vec![id.clone()]);
    let subscription = match handle.subscribe_with_options(options).await {
        Ok(subscription) => subscription,
        Err(e) => {
            let _ = core.remove_reaction(&reaction_id, true).await;
            return Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Failed to subscribe to attach reaction: {e}"),
            ));
        }
    };

    let stream = subscription.into_stream();
    let cleanup_core = core.clone();
    let cleanup_id = reaction_id.clone();

    // Create an async stream that yields query results and cleans up on drop
    let sse_stream = async_stream::stream! {
        let mut stream = stream;
        let _cleanup = AttachCleanupGuard::new(cleanup_core, cleanup_id);

        while let Some(result) = stream.next().await {
            if let Ok(json) = serde_json::to_string(&result) {
                yield Ok(Event::default().data(json));
            }
        }
    };

    Ok(Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("heartbeat"),
    ))
}

/// Guard that cleans up the attach reaction when dropped.
struct AttachCleanupGuard {
    core: Arc<drasi_lib::DrasiLib>,
    reaction_id: String,
}

impl AttachCleanupGuard {
    fn new(core: Arc<drasi_lib::DrasiLib>, reaction_id: String) -> Self {
        Self { core, reaction_id }
    }
}

impl Drop for AttachCleanupGuard {
    fn drop(&mut self) {
        let core = self.core.clone();
        let id = self.reaction_id.clone();
        tokio::spawn(async move {
            let _ = core.remove_reaction(&id, true).await;
            log::debug!("Cleaned up attach reaction: {id}");
        });
    }
}
