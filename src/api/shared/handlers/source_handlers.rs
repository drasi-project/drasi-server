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
use crate::api::models::{ComponentEventDto, LogMessageDto};
use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::api::shared::extractor::ConfigBody;
use crate::api::shared::responses::{ApiResponse, ComponentListItem, StatusResponse};
use crate::config::SourceConfig;
use crate::factories::{create_source_locked, resolve_source_bootstrap_provider};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;
use drasi_lib::channels::ComponentStatus;
use drasi_lib::DrasiLib;
use futures_util::{stream, StreamExt};
use tokio::sync::broadcast;

/// List all sources for an instance
pub async fn list_sources(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(instance_id): Extension<String>,
    Extension(api_prefix): Extension<ApiPrefix>,
) -> Result<Json<ApiResponse<Vec<ComponentListItem>>>, ErrorResponse> {
    let sources = core.list_sources().await.map_err(ErrorResponse::from)?;
    let mut items = Vec::with_capacity(sources.len());
    for (id, status) in sources {
        let links = component_links(&api_prefix.0, &instance_id, "sources", &id);
        let error_message = if matches!(status, ComponentStatus::Error) {
            match core.get_source_info(&id).await {
                Ok(info) => info.error_message,
                Err(e) => {
                    log::warn!("Failed to fetch source info for '{id}': {e}");
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

/// Resolve a source's `bootstrapProvider: <id>` reference (if any) against the
/// instance's declared top-level bootstrap providers, returning a config whose
/// bootstrap provider is inlined so it can be instantiated and wired live.
///
/// Inline definitions and sources without a bootstrap provider are returned
/// unchanged. Returns an error when the referenced id is not declared for the
/// instance. The caller keeps the original config (with the reference intact)
/// for persistence so the reference — not an inlined copy — is written back.
async fn resolve_source_bootstrap_ref(
    instance_registry: &InstanceRegistry,
    instance_id: &str,
    config: &SourceConfig,
) -> Result<SourceConfig, ErrorResponse> {
    let mut resolved = config.clone();
    let providers = instance_registry.bootstrap_providers(instance_id).await;
    // A dangling / unknown `bootstrapProvider: <id>` reference is a client
    // configuration error, so surface it as INVALID_REQUEST (HTTP 400) rather
    // than SOURCE_CREATE_FAILED (HTTP 500).
    resolve_source_bootstrap_provider(&mut resolved, &providers)
        .map_err(|e| ErrorResponse::new(error_codes::INVALID_REQUEST, e.to_string()))?;
    Ok(resolved)
}

/// Create a new source
pub async fn create_source_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Extension(instance_registry): Extension<InstanceRegistry>,
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create sources.",
        ));
    }

    let config: SourceConfig = serde_json::from_value(config_json).map_err(|e| {
        log::error!("Failed to parse source config: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid source configuration: {e}"),
        )
    })?;

    let source_id = config.id().to_string();
    let auto_start = config.auto_start();

    // Resolve any top-level `bootstrapProvider: <id>` reference against this
    // instance's declared providers so the source is wired (and bootstraps)
    // live. The original `config` retains the reference for persistence.
    let create_config =
        resolve_source_bootstrap_ref(&instance_registry, &instance_id, &config).await?;

    let (source, plugin_meta) = create_source_locked(&plugin_registry, create_config)
        .await
        .map_err(|e| {
            log::error!("Failed to create source instance: {e}");
            ErrorResponse::new(
                error_codes::SOURCE_CREATE_FAILED,
                format!("Failed to create source: {e}"),
            )
        })?;

    match core.add_source_with_metadata(source, plugin_meta).await {
        Ok(_) => {
            log::info!("Source '{source_id}' created successfully");

            if auto_start {
                if let Err(e) = core.start_source(&source_id).await {
                    log::warn!("Failed to auto-start source '{source_id}': {e}");
                }
            }

            // Track any `identityProvider` reference so persistence can
            // round-trip it (snapshot_configuration() doesn't carry it).
            if let Some(p) = &config_persistence {
                p.register_source_identity_provider(
                    &instance_id,
                    &source_id,
                    config.identity_provider(),
                )
                .await;
                // Track the source's bootstrapProvider (inline or reference)
                // so it round-trips through persistence — snapshot_configuration()
                // does not reliably carry it (issue #105).
                p.register_source_bootstrap_provider(
                    &instance_id,
                    &source_id,
                    config.bootstrap_provider(),
                )
                .await;
            }

            persist_after_operation(&config_persistence, "creating source").await?;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Source '{source_id}' created successfully"),
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") {
                log::info!("Source '{source_id}' already exists - use PUT for upsert");
                return Err(ErrorResponse::new(
                    error_codes::DUPLICATE_RESOURCE,
                    "Resource already exists",
                ));
            }
            log::error!("Failed to add source: {e}");
            Err(ErrorResponse::new(
                error_codes::SOURCE_CREATE_FAILED,
                error_msg,
            ))
        }
    }
}

/// Upsert a source (create or update)
#[allow(clippy::too_many_arguments)]
pub async fn upsert_source_handler(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Extension(plugin_registry): Extension<Arc<RwLock<PluginRegistry>>>,
    Extension(instance_registry): Extension<InstanceRegistry>,
    Path(path_id): Path<String>,
    ConfigBody(config_json): ConfigBody<serde_json::Value>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot create or update sources.",
        ));
    }

    let config: SourceConfig = serde_json::from_value(config_json).map_err(|e| {
        log::error!("Failed to parse source config: {e}");
        ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            format!("Invalid source configuration: {e}"),
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

    let source_id = config.id().to_string();
    let auto_start = config.auto_start();

    // Resolve any top-level `bootstrapProvider: <id>` reference so the source
    // is wired live; `config` keeps the reference for persistence.
    let create_config =
        resolve_source_bootstrap_ref(&instance_registry, &instance_id, &config).await?;

    // Check if source already exists
    let exists = core.get_source_status(&source_id).await.is_ok();

    if exists {
        // Create a new source instance and use update_source to replace in place
        let (new_source, _meta) = create_source_locked(&plugin_registry, create_config)
            .await
            .map_err(|e| {
                log::error!("Failed to create source instance for update: {e}");
                ErrorResponse::new(
                    error_codes::SOURCE_CREATE_FAILED,
                    format!("Failed to create source for update: {e}"),
                )
            })?;
        if let Err(e) = core.update_source(&source_id, new_source).await {
            log::error!("Failed to update source '{source_id}': {e}");
            return Err(ErrorResponse::new(
                error_codes::SOURCE_CREATE_FAILED,
                format!("Failed to update source: {e}"),
            ));
        }

        log::info!("Source '{source_id}' updated successfully");

        if let Some(p) = &config_persistence {
            p.register_source_identity_provider(
                &instance_id,
                &source_id,
                config.identity_provider(),
            )
            .await;
            p.register_source_bootstrap_provider(
                &instance_id,
                &source_id,
                config.bootstrap_provider(),
            )
            .await;
        }

        persist_after_operation(&config_persistence, "upserting source").await?;

        return Ok(Json(ApiResponse::success(StatusResponse {
            message: format!("Source '{source_id}' updated successfully"),
        })));
    }

    let (source, plugin_meta) = create_source_locked(&plugin_registry, create_config)
        .await
        .map_err(|e| {
            log::error!("Failed to create source instance: {e}");
            ErrorResponse::new(
                error_codes::SOURCE_CREATE_FAILED,
                format!("Failed to create source: {e}"),
            )
        })?;

    match core.add_source_with_metadata(source, plugin_meta).await {
        Ok(_) => {
            log::info!("Source '{source_id}' created successfully");

            if auto_start {
                if let Err(e) = core.start_source(&source_id).await {
                    log::warn!("Failed to auto-start source '{source_id}': {e}");
                }
            }

            if let Some(p) = &config_persistence {
                p.register_source_identity_provider(
                    &instance_id,
                    &source_id,
                    config.identity_provider(),
                )
                .await;
                p.register_source_bootstrap_provider(
                    &instance_id,
                    &source_id,
                    config.bootstrap_provider(),
                )
                .await;
            }

            persist_after_operation(&config_persistence, "upserting source").await?;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: format!("Source '{source_id}' created successfully"),
            })))
        }
        Err(e) => {
            log::error!("Failed to add source: {e}");
            Err(ErrorResponse::new(
                error_codes::SOURCE_CREATE_FAILED,
                e.to_string(),
            ))
        }
    }
}

/// Get source status by ID
pub async fn get_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(_config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Extension(api_prefix): Extension<ApiPrefix>,
    Query(view): Query<ComponentViewQuery>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ComponentListItem>>, ErrorResponse> {
    // Get source runtime info from ComponentGraph (source of truth)
    let info = core
        .get_source_info(&id)
        .await
        .map_err(ErrorResponse::from)?;

    // Build config from runtime info if view=full
    let config = if view.include_config() {
        let mut config_map = serde_json::Map::new();
        config_map.insert(
            "kind".to_string(),
            serde_json::Value::String(info.source_type.clone()),
        );
        config_map.insert("id".to_string(), serde_json::Value::String(info.id.clone()));
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
        links: component_links(&api_prefix.0, &instance_id, "sources", &id),
        config,
    })))
}

/// Get source lifecycle events (snapshot).
pub async fn get_source_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<ComponentEventDto>>>, ErrorResponse> {
    core.get_source_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let events = core
        .get_source_events(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let collected = events
        .map(ComponentEventDto::from)
        .collect::<Vec<_>>()
        .await;
    let data = apply_limit(collected, query.limit);
    Ok(Json(ApiResponse::success(data)))
}

/// Stream source lifecycle events as SSE.
pub async fn stream_source_events(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_source_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, receiver) = core
        .subscribe_source_events(&id)
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

/// Get source logs (snapshot).
pub async fn get_source_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<ApiResponse<Vec<LogMessageDto>>>, ErrorResponse> {
    core.get_source_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, _) = core
        .subscribe_source_logs(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let data = apply_limit(
        history.into_iter().map(LogMessageDto::from).collect(),
        query.limit,
    );
    Ok(Json(ApiResponse::success(data)))
}

/// Stream source logs as SSE.
pub async fn stream_source_logs(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ErrorResponse> {
    core.get_source_info(&id)
        .await
        .map_err(ErrorResponse::from)?;
    let (history, receiver) = core
        .subscribe_source_logs(&id)
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

/// Delete a source
pub async fn delete_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Extension(read_only): Extension<Arc<bool>>,
    Extension(config_persistence): Extension<Option<Arc<ConfigPersistence>>>,
    Extension(instance_id): Extension<String>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    if *read_only {
        return Err(ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot delete sources.",
        ));
    }

    match core.remove_source(&id, true).await {
        Ok(_) => {
            if let Some(p) = &config_persistence {
                p.unregister_source_identity_provider(&instance_id, &id)
                    .await;
                p.unregister_source_bootstrap_provider(&instance_id, &id)
                    .await;
            }
            persist_after_operation(&config_persistence, "deleting source").await?;

            Ok(Json(ApiResponse::success(StatusResponse {
                message: "Source deleted successfully".to_string(),
            })))
        }
        Err(e) => {
            log::error!("Failed to delete source: {e}");
            Err(ErrorResponse::new(
                error_codes::SOURCE_DELETE_FAILED,
                e.to_string(),
            ))
        }
    }
}

/// Start a source
pub async fn start_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    match core.start_source(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source started successfully".to_string(),
        }))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Stop a source
pub async fn stop_source(
    Extension(core): Extension<Arc<drasi_lib::DrasiLib>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatusResponse>>, ErrorResponse> {
    match core.stop_source(&id).await {
        Ok(_) => Ok(Json(ApiResponse::success(StatusResponse {
            message: "Source stopped successfully".to_string(),
        }))),
        Err(e) => Err(ErrorResponse::from(e)),
    }
}

/// Proxy data push to an HTTP/gRPC source's listening port.
///
/// The browser cannot directly POST to a source's port due to CORS restrictions.
/// This handler reads the source's configured host/port and forwards the request.
pub async fn push_source_data(
    Extension(core): Extension<Arc<DrasiLib>>,
    Extension(http_client): Extension<reqwest::Client>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ErrorResponse> {
    let info = core
        .get_source_info(&id)
        .await
        .map_err(ErrorResponse::from)?;

    let props = info.properties;
    let host = props
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1");
    let port = props.get("port").and_then(|v| v.as_u64()).unwrap_or(8081);
    let endpoint = props.get("endpoint").and_then(|v| v.as_str()).unwrap_or("");

    let base = if endpoint.is_empty() {
        String::new()
    } else {
        format!("/{}", endpoint.trim_start_matches('/'))
    };
    // Use 127.0.0.1 for 0.0.0.0 since we're on the same host
    let effective_host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
    let url = format!("http://{effective_host}:{port}{base}/sources/{id}/events");

    match http_client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            let resp_body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or(serde_json::Value::String("ok".to_string()));
            Ok(Json(ApiResponse::success(resp_body)))
        }
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            log::warn!("Source proxy got {status_code} from {url}: {msg}");
            Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                "Upstream service unavailable",
            ))
        }
        Err(err) => {
            log::warn!("Source proxy failed for {url}: {err}");
            Err(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                "Upstream service unavailable",
            ))
        }
    }
}
