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

//! Plugin management API handlers.
//!
//! These handlers implement the `/api/v1/plugins/` endpoints for listing,
//! inspecting, loading, retiring, upgrading, and querying available plugin kinds.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::instance_registry::InstanceRegistry;
use crate::plugin_orchestrator::PluginOrchestrator;

/// GET /api/v1/plugins — List all loaded plugins with their status.
pub async fn list_plugins(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
) -> impl IntoResponse {
    let plugins = orchestrator.list_plugins().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "plugins": plugins })),
    )
}

/// GET /api/v1/plugins/:id — Get details for a specific plugin.
pub async fn get_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Path(plugin_id): Path<String>,
) -> impl IntoResponse {
    match orchestrator.get_plugin_info(&plugin_id).await {
        Some(info) => (StatusCode::OK, Json(serde_json::json!(info))),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "PluginNotFound",
                "message": format!("Plugin '{}' is not loaded", plugin_id)
            })),
        ),
    }
}

/// POST /api/v1/plugins/:id/retire — Retire a plugin.
pub async fn retire_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Path(plugin_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<RetireParams>,
) -> impl IntoResponse {
    let force = params.force.unwrap_or(false);

    match orchestrator.retire_plugin(&plugin_id, force).await {
        Ok(removed) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": plugin_id,
                "status": "Retired",
                "descriptorsRemoved": removed,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "RetireFailed",
                "message": format!("{e}"),
            })),
        ),
    }
}

/// GET /api/v1/plugins/kinds — List all available kinds from the registry.
pub async fn list_kinds(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
) -> impl IntoResponse {
    let registry = orchestrator.registry();
    let reg = registry.read().await;

    let sources = reg.source_plugin_infos();
    let reactions = reg.reaction_plugin_infos();
    let bootstrappers = reg.bootstrapper_plugin_infos();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "sources": sources,
            "reactions": reactions,
            "bootstrappers": bootstrappers,
        })),
    )
}

/// POST /api/v1/plugins/load — Load a plugin from disk by filename.
///
/// Request body: `{ "filename": "libdrasi_source_postgres.dylib" }`
/// Response: 200 with the PluginInfo on success.
pub async fn load_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Json(body): Json<LoadPluginRequest>,
) -> impl IntoResponse {
    let plugins_dir = match orchestrator.plugins_dir() {
        Some(dir) => dir.to_path_buf(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "NoPluginsDirectory",
                    "message": "Server was not started with a plugins directory"
                })),
            );
        }
    };

    let path = plugins_dir.join(&body.filename);

    // Security: prevent path traversal — resolve to canonical path and verify containment
    let canonical_dir = match plugins_dir.canonicalize() {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "InternalError",
                    "message": format!("Cannot resolve plugins directory: {e}")
                })),
            );
        }
    };
    let canonical_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "FileNotFound",
                    "message": format!("Plugin file '{}' not found in plugins directory", body.filename)
                })),
            );
        }
    };
    if !canonical_path.starts_with(&canonical_dir) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "InvalidPath",
                "message": "Filename must refer to a file within the plugins directory"
            })),
        );
    }

    match orchestrator.load_plugin(&canonical_path, None).await {
        Ok(info) => (StatusCode::OK, Json(serde_json::json!(info))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "LoadFailed",
                "message": format!("{e}")
            })),
        ),
    }
}

/// POST /api/v1/plugins/install — Download and load a plugin from a remote repository.
///
/// Request body: `{ "ref": "source/postgres", "registry": "ghcr.io/drasi-project" }`
/// Response: 201 with the PluginInfo on success.
pub async fn install_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Extension(plugin_ops): Extension<Arc<crate::plugin_operations::PluginOperations>>,
    Json(body): Json<InstallPluginRequest>,
) -> impl IntoResponse {
    // Download from registry
    let plugin_path = match plugin_ops
        .install_from_registry(&body.plugin_ref, body.registry.as_deref())
        .await
    {
        Ok(path) => path,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "InstallFailed",
                    "message": format!("{e}")
                })),
            );
        }
    };

    // Load the downloaded plugin
    match orchestrator.load_plugin(&plugin_path, None).await {
        Ok(info) => (StatusCode::CREATED, Json(serde_json::json!(info))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "LoadFailed",
                "message": format!("{e}")
            })),
        ),
    }
}

/// POST /api/v1/plugins/:id/upgrade — Upgrade a plugin (drain-then-replace).
///
/// Accepts an optional JSON body with a `filename` field pointing to the new plugin
/// file in the plugins directory. If no filename is provided, attempts to find a
/// newer version in the plugins directory matching the plugin's kinds.
///
/// Uses `PluginOrchestrator::upgrade_plugin` to perform the drain-then-replace protocol:
/// load new plugin → find affected components → stop/remove/recreate → retire old plugin.
pub async fn upgrade_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Extension(instances): Extension<InstanceRegistry>,
    Path(plugin_id): Path<String>,
    body: Option<Json<UpgradePluginRequest>>,
) -> impl IntoResponse {
    // Resolve the path to the new plugin file
    let plugins_dir = match orchestrator.plugins_dir() {
        Some(dir) => dir.to_path_buf(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "NoPluginsDirectory",
                    "message": "Server was not started with a plugins directory"
                })),
            );
        }
    };

    let filename = body.and_then(|b| b.filename.clone());
    let new_path = match filename {
        Some(f) => {
            let path = plugins_dir.join(&f);
            if !path.exists() {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "FileNotFound",
                        "message": format!("Plugin file '{}' not found in plugins directory", f)
                    })),
                );
            }
            path
        }
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "MissingFilename",
                    "message": "Request body must include a 'filename' field pointing to the new plugin file"
                })),
            );
        }
    };

    match orchestrator
        .upgrade_plugin(&plugin_id, &new_path, &instances, None)
        .await
    {
        Ok(result) => {
            let status = if result.failed.is_empty() {
                StatusCode::OK
            } else {
                StatusCode::MULTI_STATUS
            };
            (
                status,
                Json(serde_json::json!({
                    "pluginId": result.new_plugin_id,
                    "oldVersion": result.old_version,
                    "newVersion": result.new_version,
                    "migrated": result.migrated,
                    "failed": result.failed.iter().map(|(id, err)| {
                        serde_json::json!({ "componentId": id, "error": err })
                    }).collect::<Vec<_>>(),
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "UpgradeFailed",
                "message": format!("{e}")
            })),
        ),
    }
}

/// POST /api/v1/plugins/:id/promote — Promote a side-by-side version to incumbent.
///
/// The plugin_id must be a versioned key (e.g., "postgres@0.4.2"). The endpoint
/// promotes the versioned descriptor to the unversioned key, making it the default
/// for new component creation.
pub async fn promote_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Path(plugin_id): Path<String>,
) -> impl IntoResponse {
    match orchestrator.promote_plugin(&plugin_id).await {
        Ok(promoted_kinds) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": plugin_id,
                "promotedKinds": promoted_kinds,
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "PromoteFailed",
                "message": format!("{e}")
            })),
        ),
    }
}

/// GET /api/v1/plugins/:id/dependents — List components that depend on this plugin.
///
/// Scans all instances' component graphs for sources and reactions whose metadata
/// contains a `pluginId` matching the requested plugin. Returns the list of
/// dependent components with their instance, type, kind, and running status.
pub async fn list_dependents(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Extension(instances): Extension<InstanceRegistry>,
    Path(plugin_id): Path<String>,
) -> impl IntoResponse {
    // Verify the plugin exists
    let plugin_info = match orchestrator.get_plugin_info(&plugin_id).await {
        Some(info) => info,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "PluginNotFound",
                    "message": format!("Plugin '{}' is not loaded", plugin_id)
                })),
            );
        }
    };

    // Scan all instances for components using this plugin
    let mut dependents = Vec::new();

    for (instance_id, core) in instances.list().await {
        let graph = core.component_graph();
        let graph_read = graph.read().await;

        // Check sources
        for (source_id, _status) in
            graph_read.list_by_kind(&drasi_lib::component_graph::ComponentKind::Source)
        {
            if let Some(node) = graph_read.get_component(&source_id) {
                if node.metadata.get("pluginId").map(|s| s.as_str()) == Some(&plugin_id) {
                    let kind = node.metadata.get("kind").cloned().unwrap_or_default();
                    let is_running = node.status == drasi_lib::channels::ComponentStatus::Running;
                    dependents.push(serde_json::json!({
                        "instanceId": instance_id,
                        "componentId": source_id,
                        "componentType": "source",
                        "kind": kind,
                        "running": is_running,
                    }));
                }
            }
        }

        // Check reactions
        for (reaction_id, _status) in
            graph_read.list_by_kind(&drasi_lib::component_graph::ComponentKind::Reaction)
        {
            if let Some(node) = graph_read.get_component(&reaction_id) {
                if node.metadata.get("pluginId").map(|s| s.as_str()) == Some(&plugin_id) {
                    let kind = node.metadata.get("kind").cloned().unwrap_or_default();
                    let is_running = node.status == drasi_lib::channels::ComponentStatus::Running;
                    dependents.push(serde_json::json!({
                        "instanceId": instance_id,
                        "componentId": reaction_id,
                        "componentType": "reaction",
                        "kind": kind,
                        "running": is_running,
                    }));
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "pluginId": plugin_id,
            "dependentCount": plugin_info.dependent_count,
            "dependents": dependents,
        })),
    )
}

/// GET /api/v1/plugins/kinds/:category/:kind/schema — Return the config schema for a kind.
///
/// Looks up the kind in the registry and returns its config_schema_json.
pub async fn get_kind_schema(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Path((category, kind)): Path<(String, String)>,
) -> impl IntoResponse {
    let registry = orchestrator.registry();
    let reg = registry.read().await;

    let infos = match category.as_str() {
        "source" | "sources" => reg.source_plugin_infos(),
        "reaction" | "reactions" => reg.reaction_plugin_infos(),
        "bootstrap" | "bootstrappers" => reg.bootstrapper_plugin_infos(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "InvalidCategory",
                    "message": format!(
                        "Unknown category '{}'. Valid categories: source, reaction, bootstrap",
                        category
                    )
                })),
            );
        }
    };

    match infos.into_iter().find(|i| i.kind == kind) {
        Some(info) => {
            if info.config_schema_json.is_empty() {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "kind": kind,
                        "category": category,
                        "schema": null
                    })),
                )
            } else {
                // Parse the schema JSON string into a value for clean output
                let schema_value: serde_json::Value =
                    serde_json::from_str(&info.config_schema_json).unwrap_or_else(|_| {
                        serde_json::Value::String(info.config_schema_json.clone())
                    });
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "kind": kind,
                        "category": category,
                        "configVersion": info.config_version,
                        "schema": schema_value
                    })),
                )
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "KindNotFound",
                "message": format!("Kind '{}' not found in category '{}'", kind, category)
            })),
        ),
    }
}

/// GET /api/v1/plugins/events — Stream plugin events via Server-Sent Events.
///
/// Subscribes to the orchestrator's broadcast channel and forwards
/// `PluginEvent`s as SSE events. The stream stays open until the client
/// disconnects.
pub async fn plugin_event_stream(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = orchestrator.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let (event_type, data) = plugin_event_to_sse(&event);
            Some(Ok(Event::default().event(event_type).data(data)))
        }
        // Lagged receiver — skip lost events
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Convert a PluginEvent to an SSE event type and JSON data string.
fn plugin_event_to_sse(event: &drasi_host_sdk::plugin_types::PluginEvent) -> (String, String) {
    use drasi_host_sdk::plugin_types::PluginEvent;

    match event {
        PluginEvent::Loaded {
            plugin_id,
            version,
            kinds,
        } => (
            "plugin.loaded".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
                "version": version,
                "kinds": kinds,
            })
            .to_string(),
        ),
        PluginEvent::LoadedSideBySide {
            plugin_id,
            version,
            incumbent_plugin_id,
            versioned_kinds,
        } => (
            "plugin.loaded_side_by_side".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
                "version": version,
                "incumbentPluginId": incumbent_plugin_id,
                "versionedKinds": versioned_kinds,
            })
            .to_string(),
        ),
        PluginEvent::Upgraded {
            plugin_id,
            old_version,
            new_version,
            migrated_components,
        } => (
            "plugin.upgraded".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
                "oldVersion": old_version,
                "newVersion": new_version,
                "migratedComponents": migrated_components,
            })
            .to_string(),
        ),
        PluginEvent::UpgradePartialFailure {
            plugin_id,
            old_version,
            new_version,
            migrated,
            failed,
        } => (
            "plugin.upgrade_partial_failure".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
                "oldVersion": old_version,
                "newVersion": new_version,
                "migrated": migrated,
                "failed": failed,
            })
            .to_string(),
        ),
        PluginEvent::Draining {
            plugin_id,
            affected_components,
        } => (
            "plugin.draining".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
                "affectedComponents": affected_components,
            })
            .to_string(),
        ),
        PluginEvent::Promoted {
            plugin_id,
            promoted_kinds,
            previous_incumbent,
        } => (
            "plugin.promoted".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
                "promotedKinds": promoted_kinds,
                "previousIncumbent": previous_incumbent,
            })
            .to_string(),
        ),
        PluginEvent::Retired { plugin_id } => (
            "plugin.retired".to_string(),
            serde_json::json!({
                "pluginId": plugin_id,
            })
            .to_string(),
        ),
        PluginEvent::LoadFailed { path, error } => (
            "plugin.load_failed".to_string(),
            serde_json::json!({
                "path": path.display().to_string(),
                "error": error,
            })
            .to_string(),
        ),
    }
}

/// Query parameters for the retire endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct RetireParams {
    pub force: Option<bool>,
}

/// Request body for POST /api/v1/plugins/load.
#[derive(Debug, serde::Deserialize)]
pub struct LoadPluginRequest {
    pub filename: String,
}

/// Request body for POST /api/v1/plugins/install.
#[derive(Debug, serde::Deserialize)]
pub struct InstallPluginRequest {
    /// Plugin reference, e.g. "source/postgres".
    #[serde(rename = "ref")]
    pub plugin_ref: String,
    /// OCI registry URL, e.g. "ghcr.io/drasi-project".
    pub registry: Option<String>,
}

/// Request body for POST /api/v1/plugins/:id/upgrade.
#[derive(Debug, serde::Deserialize)]
pub struct UpgradePluginRequest {
    /// Filename of the new plugin file in the plugins directory.
    pub filename: Option<String>,
}

/// Build the plugin API router.
pub fn plugin_routes() -> axum::Router {
    axum::Router::new()
        .route("/", axum::routing::get(list_plugins))
        .route("/kinds", axum::routing::get(list_kinds))
        .route(
            "/kinds/{category}/{kind}/schema",
            axum::routing::get(get_kind_schema),
        )
        .route("/load", axum::routing::post(load_plugin))
        .route("/install", axum::routing::post(install_plugin))
        .route("/events", axum::routing::get(plugin_event_stream))
        .route("/{plugin_id}", axum::routing::get(get_plugin))
        .route("/{plugin_id}/retire", axum::routing::post(retire_plugin))
        .route("/{plugin_id}/upgrade", axum::routing::post(upgrade_plugin))
        .route("/{plugin_id}/promote", axum::routing::post(promote_plugin))
        .route(
            "/{plugin_id}/dependents",
            axum::routing::get(list_dependents),
        )
}
