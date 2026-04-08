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
use tokio::sync::RwLock;

use super::{
    apply_limit, component_links, persist_after_operation, sse_event_async, ComponentViewQuery,
    ObservabilityQuery,
};
use crate::api::models::{ComponentEventDto, LogMessageDto};
use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::api::shared::responses::{ApiResponse, ComponentListItem, StatusResponse};
use crate::config::ReactionConfig;
use crate::factories::{create_reaction, get_reaction_plugin_metadata};
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;
use drasi_lib::channels::ComponentStatus;
use futures_util::{stream, StreamExt};
use tokio::sync::broadcast;

/// List all reactions for an instance
pub async fn list_reactions(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
) -> Json<ApiResponse<Vec<ComponentListItem>>> {
    let reactions = core.list_reactions().await.unwrap_or_default();
    let mut items = Vec::with_capacity(reactions.len());
    for (id, status) in reactions {
        let links = component_links(&instance_id, "reactions", &id);
        let error_message = if matches!(status, ComponentStatus::Error) {
            match core.get_reaction_info(&id).await {
                Ok(info) => info.error_message,
                Err(e) => {
                    log::warn!("Failed to fetch reaction info for '{id}': {e}");
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

    Json(ApiResponse::success(items))
}

/// Create a new reaction
pub async fn create_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create reactions.",
        ));
    }

    let config: ReactionConfig = serde_json::from_value(config_json).map_err(|e| {
        log::error!("Failed to parse reaction config: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid reaction configuration: {e}"),
        )
    })?;

    let reaction_id = config.id().to_string();
    let auto_start = config.auto_start();

    let reaction = create_reaction(&*plugin_registry.read().await, config.clone())
        .await
        .map_err(|e| {
            log::error!("Failed to create reaction instance: {e}");
            ErrorResponse::new(
                error_codes::REACTION_CREATE_FAILED,
                format!("Failed to create reaction: {e}"),
            )
        })?;

    let plugin_meta = get_reaction_plugin_metadata(&*plugin_registry.read().await, &config.kind);

    match core.add_reaction_with_metadata(reaction, plugin_meta).await {
        Ok(_) => {
            log::info!("Reaction '{reaction_id}' created successfully");

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
                log::info!("Reaction '{reaction_id}' already exists - use PUT for upsert");
                return Err(ErrorResponse::new(
                    error_codes::DUPLICATE_RESOURCE,
                    "Resource already exists",
                ));
            }
            log::error!("Failed to add reaction: {e}");
            Err(ErrorResponse::new(
                error_codes::REACTION_CREATE_FAILED,
                error_msg,
            ))
        }
    }
}

/// Upsert a reaction (create or update)
pub async fn upsert_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(path_id): Path<String>,
    Json(config_json): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create or update reactions.",
        ));
    }

    let config: ReactionConfig = serde_json::from_value(config_json).map_err(|e| {
        log::error!("Failed to parse reaction config: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid reaction configuration: {e}"),
        )
    })?;

    if config.id() != path_id {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!(
                "Path id '{path_id}' does not match body id '{}'",
                config.id()
            ),
        ));
    }

    let reaction_id = config.id().to_string();
    let auto_start = config.auto_start();

    // Check if reaction already exists
    let exists = core.get_reaction_info(&reaction_id).await.is_ok();

    if exists {
        // Create a new reaction instance and use update_reaction to replace in place
        let new_reaction = create_reaction(&*plugin_registry.read().await, config.clone())
            .await
            .map_err(|e| {
                log::error!("Failed to create reaction instance for update: {e}");
                ErrorResponse::new(
                    error_codes::REACTION_CREATE_FAILED,
                    format!("Failed to create reaction for update: {e}"),
                )
            })?;
        if let Err(e) = core.update_reaction(&reaction_id, new_reaction).await {
            log::error!("Failed to update reaction '{reaction_id}': {e}");
            return Err(ErrorResponse::new(
                error_codes::REACTION_CREATE_FAILED,
                format!("Failed to update reaction: {e}"),
            ));
        }

        log::info!("Reaction '{reaction_id}' updated successfully");

        persist_after_operation(&config_persistence, "upserting reaction").await;

        return Ok(Json(ApiResponse::success(StatusResponse {
            message: format!("Reaction '{reaction_id}' updated successfully"),
        })));
    }

    let reaction = create_reaction(&*plugin_registry.read().await, config.clone())
        .await
        .map_err(|e| {
            log::error!("Failed to create reaction instance: {e}");
            ErrorResponse::new(
                error_codes::REACTION_CREATE_FAILED,
                format!("Failed to create reaction: {e}"),
            )
        })?;

    let plugin_meta = get_reaction_plugin_metadata(&*plugin_registry.read().await, &config.kind);

    match core.add_reaction_with_metadata(reaction, plugin_meta).await {
        Ok(_) => {
            log::info!("Reaction '{reaction_id}' created successfully");

            if auto_start {
                if let Err(e) = core.start_reaction(&reaction_id).await {
                    log::warn!("Failed to auto-start reaction '{reaction_id}': {e}");
                }
            }

            persist_after_operation(&config_persistence, "upserting reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Reaction '{reaction_id}' created successfully"),
            })))
        }
        Err(e) => {
            log::error!("Failed to add reaction: {e}");
            Err(ErrorResponse::new(
                error_codes::REACTION_CREATE_FAILED,
                e.to_string(),
            ))
        }
    }
}

/// Get reaction status by ID
pub async fn get_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(_config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    // Get reaction runtime info from ComponentGraph (source of truth)
    let info = core
        .get_reaction_info(&id)
        .await
        .map_err(ErrorResponse::from)?;

    // Build config from runtime info if view=full
    let config = if view.include_config() {
        let mut config_map = serde_json::Map::new();
        config_map.insert(
            "kind".to_string(),
            serde_json::Value::String(info.reaction_type.clone()),
        );
        config_map.insert("id".to_string(), serde_json::Value::String(info.id.clone()));
        config_map.insert(
            "queries".to_string(),
            serde_json::Value::Array(
                info.queries
                    .iter()
                    .map(|q| serde_json::Value::String(q.clone()))
                    .collect(),
            ),
        );
        // Include properties from runtime
        for (key, value) in &info.properties {
            config_map.insert(key.clone(), value.clone());
        }
        Some(serde_json::Value::Object(config_map))
    } else {
        None
    };

    Ok(Json(ApiResponse::success(ComponentListItem {
        id: info.id,
        status: info.status,
        error_message: info.error_message,
        links: component_links(&instance_id, "reactions", &id),
        config,
    })))
}

/// Get reaction lifecycle events (snapshot).
pub async fn get_reaction_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    core.get_reaction_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let events = core
        .get_reaction_events(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let collected = events
        .map(ComponentEventDto::from)
        .collect::<Vec<_>>()
        .await;
    let data = apply_limit(collected, query.limit);
    Ok(Json(ApiResponse::success(data)))
}

/// Stream reaction lifecycle events as SSE.
pub async fn stream_reaction_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_reaction_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, receiver) = core
        .subscribe_reaction_events(&id)
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

/// Get reaction logs (snapshot).
pub async fn get_reaction_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, ErrorResponse> {
    core.get_reaction_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, _) = core
        .subscribe_reaction_logs(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let data = apply_limit(
        history.into_iter().map(LogMessageDto::from).collect(),
        query.limit,
    );
    Ok(Json(ApiResponse::success(data)))
}

/// Stream reaction logs as SSE.
pub async fn stream_reaction_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_reaction_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, receiver) = core
        .subscribe_reaction_logs(&id)
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

/// Delete a reaction
pub async fn delete_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot delete reactions.",
        ));
    }

    match core.remove_reaction(&id, true).await {
        Ok(_) => {
            persist_after_operation(&config_persistence, "deleting reaction").await;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Reaction deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete reaction: {e}");
            Err(ErrorResponse::new(
                error_codes::REACTION_DELETE_FAILED,
                e.to_string(),
            ))
        }
    }
}

/// Start a reaction
pub async fn start_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    match core.start_reaction(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction started successfully".to_string(),
        }))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Stop a reaction
pub async fn stop_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    match core.stop_reaction(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Reaction stopped successfully".to_string(),
        }))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Stream ALL component events for an instance as SSE.
///
/// This endpoint subscribes to the global component event broadcast channel
/// and streams every component lifecycle change (status transitions, additions,
/// removals) as SSE events. Used by the UI to reactively update without polling.
pub async fn stream_all_component_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let receiver = core.subscribe_all_component_events();

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

    Sse::new(live_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("heartbeat"),
    )
}
