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

use crate::api::shared::error::{error_codes, ErrorResponse};
use crate::instance_registry::InstanceRegistry;
use crate::plugin_orchestrator::PluginOrchestrator;

// ---- Plugin API DTO types (for OpenAPI schema generation) ----

/// Response for GET /api/v1/plugins
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginInfoDto>,
}

/// Plugin information DTO for OpenAPI documentation.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfoDto {
    pub id: String,
    pub status: String,
    pub plugin_version: String,
    pub sdk_version: String,
    pub file_path: String,
    pub loaded_at: String,
    pub library_generation: u64,
    pub dependent_count: usize,
    pub kinds: Vec<PluginKindDto>,
}

/// A single kind provided by a plugin.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginKindDto {
    pub category: String,
    pub kind: String,
    pub config_version: String,
}

/// Available plugin kinds grouped by category.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PluginKindsResponse {
    pub sources: Vec<PluginKindInfoDto>,
    pub reactions: Vec<PluginKindInfoDto>,
    pub bootstrappers: Vec<PluginKindInfoDto>,
}

/// Information about a specific plugin kind.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginKindInfoDto {
    pub kind: String,
    pub config_version: String,
    pub config_schema_json: String,
    pub config_schema_name: String,
    pub plugin_id: String,
}

/// Response for GET /api/v1/plugins/{pluginId}/dependents
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDependentsResponse {
    pub plugin_id: String,
    pub dependent_count: usize,
    pub dependents: Vec<PluginDependentDto>,
}

/// A component that depends on a plugin.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDependentDto {
    pub instance_id: String,
    pub component_id: String,
    pub component_type: String,
    pub kind: String,
    pub running: bool,
}

/// Result of a plugin upgrade operation.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpgradeResponse {
    pub plugin_id: String,
    pub old_version: String,
    pub new_version: String,
    pub migrated: Vec<String>,
    pub failed: Vec<serde_json::Value>,
}

/// Result of a plugin promote operation.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginPromoteResponse {
    pub id: String,
    pub promoted_kinds: Vec<String>,
}

/// Result of a plugin retire operation.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PluginRetireResponse {
    pub id: String,
    pub status: String,
    #[serde(rename = "descriptorsRemoved")]
    pub descriptors_removed: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins",
    tag = "Plugins",
    responses(
        (status = 200, description = "Plugin list", body = PluginListResponse)
    )
)]
/// List all loaded plugins with their status, kinds, and metadata.
pub async fn list_plugins(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
) -> impl IntoResponse {
    let plugins = orchestrator.list_plugins().await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "plugins": plugins })),
    )
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{pluginId}",
    tag = "Plugins",
    params(
        ("pluginId" = String, Path, description = "Plugin identifier")
    ),
    responses(
        (status = 200, description = "Plugin details", body = PluginInfoDto),
        (status = 404, description = "Plugin not found")
    )
)]
/// Get details for a specific loaded plugin.
pub async fn get_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Path(plugin_id): Path<String>,
) -> impl IntoResponse {
    match orchestrator.get_plugin_info(&plugin_id).await {
        Some(info) => (StatusCode::OK, Json(serde_json::json!(info))),
        None => ErrorResponse::new(
            error_codes::PLUGIN_NOT_FOUND,
            format!("Plugin '{plugin_id}' is not loaded"),
        )
        .into_json_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/{pluginId}/retire",
    tag = "Plugins",
    params(
        ("pluginId" = String, Path, description = "Plugin identifier"),
        ("force" = Option<bool>, Query, description = "Force retire even with active dependents")
    ),
    responses(
        (status = 200, description = "Plugin retired", body = PluginRetireResponse),
        (status = 500, description = "Retire failed")
    )
)]
/// Retire a loaded plugin, removing its descriptors from the registry.
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
        Err(e) => ErrorResponse::new(
            error_codes::PLUGIN_RETIRE_FAILED,
            format!("{e}"),
        )
        .into_json_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/kinds",
    tag = "Plugins",
    responses(
        (status = 200, description = "Available kinds", body = PluginKindsResponse)
    )
)]
/// List all available source, reaction, and bootstrapper kinds from the plugin registry.
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

#[utoipa::path(
    post,
    path = "/api/v1/plugins/load",
    tag = "Plugins",
    request_body = LoadPluginRequest,
    responses(
        (status = 200, description = "Plugin loaded", body = PluginInfoDto),
        (status = 400, description = "Invalid path"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Load failed")
    )
)]
/// Load a plugin shared library from the plugins directory by filename.
pub async fn load_plugin(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Json(body): Json<LoadPluginRequest>,
) -> impl IntoResponse {
    let plugins_dir = match orchestrator.plugins_dir() {
        Some(dir) => dir.to_path_buf(),
        None => {
            return ErrorResponse::new(
                error_codes::PLUGIN_NO_DIRECTORY,
                "Server was not started with a plugins directory",
            )
            .into_json_response();
        }
    };

    let path = plugins_dir.join(&body.filename);

    // Security: prevent path traversal — resolve to canonical path and verify containment
    let canonical_dir = match plugins_dir.canonicalize() {
        Ok(d) => d,
        Err(e) => {
            return ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Cannot resolve plugins directory: {e}"),
            )
            .into_json_response();
        }
    };
    let canonical_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return ErrorResponse::new(
                error_codes::PLUGIN_FILE_NOT_FOUND,
                format!(
                    "Plugin file '{}' not found in plugins directory",
                    body.filename
                ),
            )
            .into_json_response();
        }
    };
    if !canonical_path.starts_with(&canonical_dir) {
        return ErrorResponse::new(
            error_codes::PLUGIN_INVALID_PATH,
            "Filename must refer to a file within the plugins directory",
        )
        .into_json_response();
    }

    match orchestrator.load_plugin(&canonical_path, None).await {
        Ok(info) => (StatusCode::OK, Json(serde_json::json!(info))),
        Err(e) => ErrorResponse::new(error_codes::PLUGIN_LOAD_FAILED, format!("{e}")).into_json_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/install",
    tag = "Plugins",
    request_body = InstallPluginRequest,
    responses(
        (status = 201, description = "Plugin installed and loaded", body = PluginInfoDto),
        (status = 500, description = "Install or load failed")
    )
)]
/// Download and load a plugin from an OCI registry.
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
            return ErrorResponse::new(error_codes::PLUGIN_INSTALL_FAILED, format!("{e}"))
                .into_json_response();
        }
    };

    // Load the downloaded plugin
    match orchestrator.load_plugin(&plugin_path, None).await {
        Ok(info) => (StatusCode::CREATED, Json(serde_json::json!(info))),
        Err(e) => ErrorResponse::new(error_codes::PLUGIN_LOAD_FAILED, format!("{e}")).into_json_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/{pluginId}/upgrade",
    tag = "Plugins",
    params(
        ("pluginId" = String, Path, description = "Plugin identifier to upgrade")
    ),
    request_body = UpgradePluginRequest,
    responses(
        (status = 200, description = "Upgrade successful", body = PluginUpgradeResponse),
        (status = 207, description = "Upgrade partially failed"),
        (status = 400, description = "Missing filename"),
        (status = 404, description = "Plugin file not found"),
        (status = 500, description = "Upgrade failed")
    )
)]
/// Upgrade a plugin via drain-then-replace.
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
            return ErrorResponse::new(
                error_codes::PLUGIN_NO_DIRECTORY,
                "Server was not started with a plugins directory",
            )
            .into_json_response();
        }
    };

    let filename = body.and_then(|b| b.filename.clone());
    let new_path = match filename {
        Some(f) => {
            let path = plugins_dir.join(&f);
            if !path.exists() {
                return ErrorResponse::new(
                    error_codes::PLUGIN_FILE_NOT_FOUND,
                    format!("Plugin file '{f}' not found in plugins directory"),
                )
                .into_json_response();
            }
            path
        }
        None => {
            return ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                "Request body must include a 'filename' field pointing to the new plugin file",
            )
            .into_json_response();
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
        Err(e) => {
            ErrorResponse::new(error_codes::PLUGIN_UPGRADE_FAILED, format!("{e}")).into_json_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/{pluginId}/promote",
    tag = "Plugins",
    params(
        ("pluginId" = String, Path, description = "Versioned plugin identifier to promote")
    ),
    responses(
        (status = 200, description = "Plugin promoted", body = PluginPromoteResponse),
        (status = 400, description = "Promotion failed")
    )
)]
/// Promote a versioned plugin to be the incumbent for its kinds.
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
        Err(e) => {
            ErrorResponse::new(error_codes::PLUGIN_PROMOTE_FAILED, format!("{e}")).into_json_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{pluginId}/dependents",
    tag = "Plugins",
    params(
        ("pluginId" = String, Path, description = "Plugin identifier")
    ),
    responses(
        (status = 200, description = "Dependent components", body = PluginDependentsResponse),
        (status = 404, description = "Plugin not found")
    )
)]
/// List all source and reaction components that depend on this plugin.
pub async fn list_dependents(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    Extension(instances): Extension<InstanceRegistry>,
    Path(plugin_id): Path<String>,
) -> impl IntoResponse {
    // Verify the plugin exists
    let plugin_info = match orchestrator.get_plugin_info(&plugin_id).await {
        Some(info) => info,
        None => {
            return ErrorResponse::new(
                error_codes::PLUGIN_NOT_FOUND,
                format!("Plugin '{plugin_id}' is not loaded"),
            )
            .into_json_response();
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

#[utoipa::path(
    get,
    path = "/api/v1/plugins/kinds/{category}/{kind}/schema",
    tag = "Plugins",
    params(
        ("category" = String, Path, description = "Plugin category (source, reaction, bootstrap)"),
        ("kind" = String, Path, description = "Plugin kind name")
    ),
    responses(
        (status = 200, description = "Config schema"),
        (status = 400, description = "Invalid category"),
        (status = 404, description = "Kind not found")
    )
)]
/// Return the JSON Schema for the configuration of a specific plugin kind.
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
            return ErrorResponse::new(
                error_codes::PLUGIN_INVALID_CATEGORY,
                format!(
                    "Unknown category '{category}'. Valid categories: source, reaction, bootstrap"
                ),
            )
            .into_json_response();
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
        None => ErrorResponse::new(
            error_codes::PLUGIN_KIND_NOT_FOUND,
            format!("Kind '{kind}' not found in category '{category}'"),
        )
        .into_json_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/events",
    tag = "Plugins",
    responses(
        (status = 200, description = "SSE event stream", content_type = "text/event-stream")
    )
)]
/// Stream plugin lifecycle events via Server-Sent Events.
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
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct RetireParams {
    pub force: Option<bool>,
}

/// Request body for POST /api/v1/plugins/load.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct LoadPluginRequest {
    pub filename: String,
}

/// Request body for POST /api/v1/plugins/install.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct InstallPluginRequest {
    /// Plugin reference, e.g. "source/postgres".
    #[serde(rename = "ref")]
    pub plugin_ref: String,
    /// OCI registry URL, e.g. "ghcr.io/drasi-project".
    pub registry: Option<String>,
}

/// Request body for POST /api/v1/plugins/:id/upgrade.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct UpgradePluginRequest {
    /// Filename of the new plugin file in the plugins directory.
    pub filename: Option<String>,
}

/// Query parameters for GET /api/v1/plugins/registry/search.
#[derive(Debug, serde::Deserialize)]
pub struct SearchRegistryParams {
    /// Search query (default: "*" for all available plugins).
    #[serde(default = "default_search_query")]
    pub q: String,
    /// Optional registry override (OCI URL or local directory path).
    pub registry: Option<String>,
}

fn default_search_query() -> String {
    "*".to_string()
}

/// A plugin available in the registry (not yet installed).
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPluginDto {
    /// Short plugin reference, e.g. "source/postgres".
    pub reference: String,
    /// Full reference (OCI URL or file:// path).
    pub full_reference: String,
    /// Latest available version.
    pub version: String,
    /// Filename (populated for local sources).
    pub filename: String,
    /// Source kind: "local" or "oci".
    pub source: String,
}

/// Search a plugin registry for available plugins.
///
/// Queries the configured (or overridden) plugin registry and returns
/// a list of available plugins that match the search query.
#[utoipa::path(
    get,
    path = "/api/v1/plugins/registry/search",
    tag = "Plugins",
    params(
        ("q" = Option<String>, Query, description = "Search query (default: * for all)"),
        ("registry" = Option<String>, Query, description = "Registry override (OCI URL or local path)")
    ),
    responses(
        (status = 200, description = "Search results", body = Vec<RegistryPluginDto>),
        (status = 500, description = "Search failed")
    )
)]
pub async fn search_registry(
    Extension(plugin_ops): Extension<Arc<crate::plugin_operations::PluginOperations>>,
    axum::extract::Query(params): axum::extract::Query<SearchRegistryParams>,
) -> impl IntoResponse {
    match plugin_ops
        .search_registry(&params.q, params.registry.as_deref())
        .await
    {
        Ok(results) => {
            let dtos: Vec<RegistryPluginDto> = results
                .into_iter()
                .map(|r| RegistryPluginDto {
                    reference: r.reference,
                    full_reference: r.full_reference,
                    version: r.version,
                    filename: r.filename,
                    source: r.source,
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(dtos)))
        }
        Err(e) => {
            ErrorResponse::new(error_codes::PLUGIN_SEARCH_FAILED, format!("{e}")).into_json_response()
        },
    }
}

/// Build the plugin API router.
pub fn plugin_routes() -> axum::Router {
    // Schema subrouter — needs to be separate to avoid {plugin_id} conflict
    let kinds_router = axum::Router::new()
        .route("/", axum::routing::get(list_kinds))
        .route(
            "/:category/:kind/schema",
            axum::routing::get(get_kind_schema),
        );

    axum::Router::new()
        .route("/", axum::routing::get(list_plugins))
        .nest("/kinds", kinds_router)
        .route("/load", axum::routing::post(load_plugin))
        .route("/install", axum::routing::post(install_plugin))
        .route("/registry/search", axum::routing::get(search_registry))
        .route("/events", axum::routing::get(plugin_event_stream))
        .route("/:plugin_id", axum::routing::get(get_plugin))
        .route("/:plugin_id/retire", axum::routing::post(retire_plugin))
        .route("/:plugin_id/upgrade", axum::routing::post(upgrade_plugin))
        .route("/:plugin_id/promote", axum::routing::post(promote_plugin))
        .route(
            "/:plugin_id/dependents",
            axum::routing::get(list_dependents),
        )
}
