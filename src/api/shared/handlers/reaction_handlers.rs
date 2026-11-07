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
    apply_limit, component_links, persist_after_operation, sse_event_async, ApiPrefix,
    ComponentViewQuery, ObservabilityQuery,
};
use crate::api::models::ReactionConfig;
use crate::api::models::{ComponentEventDto, LogMessageDto};
use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::api::shared::extractor::ConfigBody;
use crate::api::shared::responses::{
    ApiResponse, ComponentCreatedResponse, ComponentListItem, StatusResponse,
};
use crate::factories::create_reaction_instance;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;
use futures_util::{stream, StreamExt};
use tokio::sync::broadcast;

/// List all reactions for an instance
pub async fn list_reactions(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
    Extension(api_prefix): Extension<ApiPrefix>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, ErrorResponse> {
    Ok(Json(ApiResponse::success(
        super::component_list(
            &core,
            drasi_lib::ComponentKind::Reaction,
            "reactions",
            &instance_id,
            &api_prefix.0,
        )
        .await?,
    )))
}

/// Create a new reaction
pub async fn create_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<ComponentCreatedResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create reactions.",
        ));
    }

    super::require_imperative_configuration(&core)?;
    let config: ReactionConfig = serde_json::from_value(config_json).map_err(|e| {
        log::error!("Failed to parse reaction config: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid reaction configuration: {e}"),
        )
    })?;

    let handle = create_reaction_instance(&core, plugin_registry, config.clone()).await?;
    let reaction_id = handle.id().to_string();
    if config_persistence.is_some() {
        super::wait_for_computation_creation(&core, "reaction", &reaction_id).await?;
    }
    persist_after_operation(&config_persistence, "creating reaction").await?;
    Ok(Json(ApiResponse::success(ComponentCreatedResponse {
        id: reaction_id,
        name: config.name,
        message: "Reaction declaration accepted".into(),
    })))
}

/// Upsert a reaction (create or update)
pub async fn upsert_reaction_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(_instance_id): Extension<String>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Path(path_id): Path<String>,
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create or update reactions.",
        ));
    }

    super::require_imperative_configuration(&core)?;
    let mut config: ReactionConfig = serde_json::from_value(config_json).map_err(|e| {
        log::error!("Failed to parse reaction config: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid reaction configuration: {e}"),
        )
    })?;

    let id = drasi_lib::config::ComponentId::try_new(path_id)
        .map_err(|error| ErrorResponse::new(error_codes::INVALID_REQUEST, error.to_string()))?;
    if config.id.as_ref().is_some_and(|requested| requested != &id) {
        return Err(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "The component UUID cannot be changed",
        ));
    }

    config.id = Some(id.clone());
    let reaction_id = id.to_string();
    let snapshot = core.computation_control()?.desired_snapshot();
    if let Some(existing) = snapshot.component(&id) {
        core.get_reaction_status(&reaction_id).await?;
        if existing.name.as_str() != config.name {
            return Err(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Use the component rename endpoint to change its name",
            ));
        }
        let definition =
            crate::managed_configuration::ordinary::reaction_definition(config.clone())?
                .with_assigned_id(id);
        core.update_reaction(&reaction_id, definition).await?;
    } else {
        create_reaction_instance(&core, plugin_registry, config.clone()).await?;
    }
    if config_persistence.is_some() {
        super::wait_for_computation_creation(&core, "reaction", &reaction_id).await?;
    }
    persist_after_operation(&config_persistence, "upserting reaction").await?;
    Ok(Json(ApiResponse::success(StatusResponse {
        message: format!("Reaction '{reaction_id}' definition accepted"),
    })))
}

/// Get reaction status by ID
pub async fn get_reaction(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(_config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Extension(api_prefix): Extension<ApiPrefix>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    if let Some(component) = super::managed_component(
        &core,
        &id,
        "reactions",
        &instance_id,
        &api_prefix.0,
        view.include_config(),
    )
    .await?
    {
        return Ok(Json(ApiResponse::success(component)));
    }
    // Read authoritative graph-backed reaction information.
    let info = core
        .get_reaction_info(&id)
        .await
        .map_err(ErrorResponse::from)?;

    // Build config from runtime info if view=full
    let config = if view.include_config() {
        let mut config_map: serde_json::Map<_, _> = info.properties.clone().into_iter().collect();
        config_map.insert(
            "kind".to_string(),
            serde_json::Value::String(info.reaction_type.clone()),
        );
        config_map.insert("id".to_string(), serde_json::Value::String(info.id.clone()));
        config_map.insert(
            "name".to_string(),
            serde_json::Value::String(info.name.clone()),
        );
        config_map.insert(
            "queries".to_string(),
            serde_json::to_value(&info.queries).map_err(|error| {
                ErrorResponse::new(error_codes::INTERNAL_ERROR, error.to_string())
            })?,
        );
        Some(serde_json::Value::Object(config_map))
    } else {
        None
    };

    Ok(Json(ApiResponse::success(ComponentListItem {
        id: info.id,
        name: info.name,
        status: info.status,
        error_message: info.error_message,
        links: component_links(&api_prefix.0, &instance_id, "reactions", &id),
        config,
    })))
}

/// Get reaction lifecycle events (snapshot).
pub async fn get_reaction_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    core.get_reaction_status(&id)
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
    core.get_reaction_status(&id)
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
    core.get_reaction_status(&id)
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
    core.get_reaction_status(&id)
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

    super::require_imperative_configuration(&core)?;
    match core.remove_reaction(&id, true).await {
        Ok(_) => {
            persist_after_operation(&config_persistence, "deleting reaction").await?;

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
