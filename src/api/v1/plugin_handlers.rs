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
//! inspecting, loading, and querying available plugin kinds.

use std::sync::Arc;

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use drasi_lib::component_graph::ComponentKind;

use crate::api::shared::error::{error_codes, ErrorDetail, ErrorResponse};
use crate::api::shared::extractor::ConfigBody;
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
    pub computation: Vec<ComputationFactoryInfo>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComputationFactoryInfo {
    pub plugin_id: String,
    #[schema(value_type = serde_json::Value)]
    pub metadata: drasi_host_sdk::computation::FactoryMetadata,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ComputationPluginMetadataResponse {
    /// Independent ABI/wire versions, plugin identities, factory interfaces and schemas.
    #[schema(value_type = Vec<serde_json::Value>)]
    pub plugins: Vec<drasi_host_sdk::computation::PluginMetadata>,
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
    /// Native graph scope (absent for legacy component dependents).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<String>,
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
    Extension(instances): Extension<InstanceRegistry>,
) -> impl IntoResponse {
    let mut plugins = orchestrator.list_plugins().await;
    for plugin in &mut plugins {
        match collect_plugin_dependents(&instances, &plugin.id).await {
            Ok(dependents) => {
                plugin.dependent_count = dependents.len();
                orchestrator
                    .update_dependent_count(&plugin.id, dependents.len())
                    .await;
                if let Some(updated) = orchestrator.get_plugin_info(&plugin.id).await {
                    plugin.status = updated.status;
                }
            }
            Err(error) => return error.into_json_response(),
        }
    }
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
    Extension(instances): Extension<InstanceRegistry>,
    Path(plugin_id): Path<String>,
) -> impl IntoResponse {
    match orchestrator.get_plugin_info(&plugin_id).await {
        Some(mut info) => match collect_plugin_dependents(&instances, &plugin_id).await {
            Ok(dependents) => {
                orchestrator
                    .update_dependent_count(&plugin_id, dependents.len())
                    .await;
                info.dependent_count = dependents.len();
                if let Some(updated) = orchestrator.get_plugin_info(&plugin_id).await {
                    info.status = updated.status;
                }
                (StatusCode::OK, Json(serde_json::json!(info)))
            }
            Err(error) => error.into_json_response(),
        },
        None => ErrorResponse::new(
            error_codes::PLUGIN_NOT_FOUND,
            format!("Plugin '{plugin_id}' is not loaded"),
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
    let computation: Vec<_> = reg
        .computation_plugin_metadata()
        .into_iter()
        .flat_map(|plugin| {
            let plugin_id = drasi_host_sdk::plugin_registry::computation_plugin_id(&plugin);
            plugin
                .factories
                .into_iter()
                .map(move |metadata| ComputationFactoryInfo {
                    plugin_id: plugin_id.clone(),
                    metadata,
                })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "sources": sources,
            "reactions": reactions,
            "bootstrappers": bootstrappers,
            "computation": computation,
        })),
    )
}

#[utoipa::path(
    get, path = "/api/v1/plugins/computation", tag = "Plugins",
    responses((status = 200, description = "Native plugin and factory metadata (no instance configuration)", body = ComputationPluginMetadataResponse))
)]
pub async fn computation_plugin_metadata(
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
) -> Json<ComputationPluginMetadataResponse> {
    Json(ComputationPluginMetadataResponse {
        plugins: orchestrator
            .registry()
            .read()
            .await
            .computation_plugin_metadata(),
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/load",
    tag = "Plugins",
    request_body = LoadPluginRequest,
    responses(
        (status = 200, description = "Plugin loaded", body = PluginInfoDto),
        (status = 400, description = "Invalid path"),
        (status = 403, description = "Server is in read-only mode"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Load failed")
    )
)]
/// Load a plugin shared library from the plugins directory by filename.
pub async fn load_plugin(
    Extension(read_only): Extension<Arc<bool>>,
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    ConfigBody(body): ConfigBody<LoadPluginRequest>,
) -> impl IntoResponse {
    if *read_only {
        return ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot load plugins.",
        )
        .into_json_response();
    }
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
            return plugin_operation_error(
                error_codes::INTERNAL_ERROR,
                "Cannot resolve plugins directory",
                e,
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

    match orchestrator.load_plugin_locked(&canonical_path, None).await {
        Ok(info) => (StatusCode::OK, Json(serde_json::json!(info))),
        Err(e) => {
            plugin_operation_error(error_codes::PLUGIN_LOAD_FAILED, "Plugin loading failed", e)
                .into_json_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/plugins/install",
    tag = "Plugins",
    request_body = InstallPluginRequest,
    responses(
        (status = 201, description = "Plugin installed and loaded", body = PluginInfoDto),
        (status = 403, description = "Server is in read-only mode"),
        (status = 500, description = "Install or load failed")
    )
)]
/// Download and load a plugin from an OCI registry.
pub async fn install_plugin(
    Extension(read_only): Extension<Arc<bool>>,
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    ConfigBody(body): ConfigBody<InstallPluginRequest>,
) -> impl IntoResponse {
    if *read_only {
        return ErrorResponse::new(
            error_codes::CONFIG_READ_ONLY,
            "Server is in read-only mode. Cannot install plugins.",
        )
        .into_json_response();
    }
    // Atomic install + verify + load via the orchestrator
    match orchestrator
        .install_and_load(&body.plugin_ref, body.registry.as_deref(), None)
        .await
    {
        Ok(info) => (StatusCode::CREATED, Json(serde_json::json!(info))),
        Err(e) => plugin_operation_error(
            error_codes::PLUGIN_INSTALL_FAILED,
            "Plugin installation or loading failed",
            e,
        )
        .into_json_response(),
    }
}

fn plugin_operation_error(
    code: &str,
    message: &str,
    error: impl std::fmt::Display,
) -> ErrorResponse {
    log::error!("{message}: {error}");
    ErrorResponse::new(code, message).with_details(ErrorDetail {
        component_type: Some("plugin".to_string()),
        component_id: None,
        technical_details: Some(error.to_string()),
    })
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
    if orchestrator.get_plugin_info(&plugin_id).await.is_none() {
        return ErrorResponse::new(
            error_codes::PLUGIN_NOT_FOUND,
            format!("Plugin '{plugin_id}' is not loaded"),
        )
        .into_json_response();
    }

    let dependents = match collect_plugin_dependents(&instances, &plugin_id).await {
        Ok(dependents) => dependents,
        Err(error) => return error.into_json_response(),
    };
    orchestrator
        .update_dependent_count(&plugin_id, dependents.len())
        .await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "pluginId": plugin_id,
            "dependentCount": dependents.len(),
            "dependents": dependents,
        })),
    )
}

async fn collect_plugin_dependents(
    instances: &InstanceRegistry,
    plugin_id: &str,
) -> Result<Vec<serde_json::Value>, ErrorResponse> {
    let mut dependents = Vec::new();
    for (instance_id, core) in instances.list().await {
        for node in core.get_graph().await.nodes {
            let component_type = match node.kind {
                ComponentKind::Source => "source",
                ComponentKind::Reaction => "reaction",
                _ => continue,
            };
            if node.metadata.get("pluginId").map(String::as_str) != Some(plugin_id) {
                continue;
            }
            dependents.push(serde_json::json!({
                "instanceId": instance_id,
                "componentId": node.id,
                "componentType": component_type,
                "kind": node.metadata.get("kind").cloned().unwrap_or_default(),
                "running": node.status == drasi_lib::ComponentStatus::Running,
            }));
        }
        if plugin_id.starts_with("computation:") {
            use drasi_lib::computation::v1::{
                ComponentEntityConstruction, ComponentLifecycle, GraphEntity,
            };
            let inventory = core.inspect_computation_inventory().await?;
            for (scope, snapshot) in &inventory.scopes {
                for entity in snapshot.topology.nodes.values() {
                    let GraphEntity::Component(component) = entity else {
                        continue;
                    };
                    let Some(plugin) = component.plugin_identity() else {
                        continue;
                    };
                    if format!("computation:{}@{}", plugin.id, plugin.version) != plugin_id {
                        continue;
                    }
                    let kind = match &component.construction {
                        ComponentEntityConstruction::Factory { implementation, .. } => {
                            Some(implementation.name.as_ref())
                        }
                        ComponentEntityConstruction::External => None,
                    };
                    dependents.push(serde_json::json!({
                        "instanceId": instance_id,
                        "graphId": scope.root_graph,
                        "owners": scope.owners,
                        "componentId": component.desired.descriptor.id(),
                        "componentType": "computation",
                        "kind": kind,
                        "running": component.observed.as_ref().is_some_and(|state| state.lifecycle == ComponentLifecycle::Running),
                    }));
                }
            }
        }
    }
    Ok(dependents)
}

pub(crate) fn native_configuration_schema(
    schema: &drasi_host_sdk::computation::ConfigSchema,
) -> serde_json::Value {
    use drasi_host_sdk::computation::ConfigType;
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, field) in &schema.fields {
        let mut property = serde_json::Map::new();
        let kind = match field.value_type {
            ConfigType::Boolean => Some("boolean"),
            ConfigType::Integer => Some("integer"),
            ConfigType::String => Some("string"),
            ConfigType::Object => Some("object"),
            ConfigType::Array => Some("array"),
            ConfigType::Json => None,
        };
        if let Some(kind) = kind {
            property.insert("type".into(), serde_json::json!(kind));
            if kind == "array" {
                property.insert("items".into(), serde_json::json!({}));
            }
        }
        if field.secret {
            property.insert("writeOnly".into(), serde_json::json!(true));
        }
        properties.insert(name.clone(), serde_json::Value::Object(property));
        if field.required {
            required.push(name.clone());
        }
    }
    serde_json::json!({"type":"object", "properties":properties, "required":required, "additionalProperties":schema.allow_additional})
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

    if category == "computation" {
        let matches: Vec<_> = reg
            .computation_plugin_metadata()
            .into_iter()
            .flat_map(|plugin| plugin.factories)
            .filter(|factory| factory.implementation.name.as_ref() == kind)
            .collect();
        return match matches.as_slice() {
            [factory] => (StatusCode::OK, Json(serde_json::json!({
                "kind":kind, "category":category,
                "implementation":factory.implementation,
                "configVersion":factory.configuration_version,
                "schema":native_configuration_schema(&factory.configuration),
            }))),
            [] => ErrorResponse::new(error_codes::PLUGIN_KIND_NOT_FOUND, "Native factory not found").into_json_response(),
            _ => ErrorResponse::new(error_codes::INVALID_REQUEST, "Multiple native factory versions match; use /plugins/computation for version-qualified metadata").into_json_response(),
        };
    }

    let infos = match category.as_str() {
        "source" | "sources" => reg.source_plugin_infos(),
        "reaction" | "reactions" => reg.reaction_plugin_infos(),
        "bootstrap" | "bootstrappers" => reg.bootstrapper_plugin_infos(),
        _ => {
            return ErrorResponse::new(
                error_codes::PLUGIN_INVALID_CATEGORY,
                format!(
                    "Unknown category '{category}'. Valid categories: source, reaction, bootstrap, computation"
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
    Extension(orchestrator): Extension<Arc<PluginOrchestrator>>,
    axum::extract::Query(params): axum::extract::Query<SearchRegistryParams>,
) -> impl IntoResponse {
    let plugin_ops = match orchestrator.ops() {
        Some(ops) => ops,
        None => {
            return ErrorResponse::new(
                error_codes::PLUGIN_NO_DIRECTORY,
                "Server was not started with plugin operations configured",
            )
            .into_json_response();
        }
    };
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
        Err(e) => ErrorResponse::new(error_codes::PLUGIN_SEARCH_FAILED, format!("{e}"))
            .into_json_response(),
    }
}

/// Build the plugin API router (without extensions).
///
/// Prefer [`build_plugin_router`] which wires the required extensions in the
/// same place as the routes. Callers using `plugin_routes()` directly must add
/// every extension that any plugin handler extracts, including
/// `Extension<Arc<PluginOrchestrator>>`, `Extension<InstanceRegistry>`, and
/// `Extension<Arc<bool>>` (the read-only flag used by `load_plugin` /
/// `install_plugin`).
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
        .route(
            "/computation",
            axum::routing::get(computation_plugin_metadata),
        )
        .nest("/kinds", kinds_router)
        .route("/load", axum::routing::post(load_plugin))
        .route("/install", axum::routing::post(install_plugin))
        .route("/registry/search", axum::routing::get(search_registry))
        .route("/:plugin_id", axum::routing::get(get_plugin))
        .route(
            "/:plugin_id/dependents",
            axum::routing::get(list_dependents),
        )
}

/// Build the plugin API router with all required extensions layered.
///
/// This is the preferred constructor: it collocates the route definitions
/// with the extensions that the handlers extract so that adding a new
/// extension to a handler is caught here rather than at request time with a
/// `Missing request extension` error.
pub fn build_plugin_router(
    orchestrator: Arc<PluginOrchestrator>,
    instances: InstanceRegistry,
    read_only: Arc<bool>,
) -> axum::Router {
    plugin_routes()
        .layer(Extension(orchestrator))
        .layer(Extension(instances))
        .layer(Extension(read_only))
}

#[cfg(test)]
mod tests {
    use super::*;
    use drasi_reaction_application::ApplicationReaction;
    use drasi_source_application::{ApplicationSource, ApplicationSourceConfig};
    use serde_json::json;
    use std::{collections::HashMap, time::Duration};

    #[tokio::test]
    async fn dependents_use_graph_metadata_and_preserve_instance_scope() {
        let registry = InstanceRegistry::new();
        for instance_id in ["first", "second"] {
            let core = Arc::new(
                drasi_lib::DrasiLib::builder()
                    .with_id(instance_id)
                    .build()
                    .await
                    .unwrap(),
            );
            let mut metadata = HashMap::from([
                ("pluginId".into(), "fixture-plugin".into()),
                ("pluginVersion".into(), "1.2.3".into()),
                ("kind".into(), "application".into()),
            ]);
            if instance_id == "second" {
                metadata.remove("pluginVersion");
            }
            let (source, _) = ApplicationSource::new(
                "shared-source",
                ApplicationSourceConfig {
                    properties: HashMap::from([(
                        "password".into(),
                        json!("plugin-secret-do-not-echo"),
                    )]),
                    durability: None,
                },
            )
            .unwrap();
            core.add_source_with_metadata(source, metadata.clone())
                .await
                .unwrap();
            let (reaction, _) = ApplicationReaction::builder("shared-reaction")
                .with_queries(Vec::new())
                .with_auto_start(false)
                .build();
            core.add_reaction_with_metadata(reaction, metadata)
                .await
                .unwrap();
            let (unrelated, _) = ApplicationSource::new(
                "unrelated",
                ApplicationSourceConfig {
                    properties: HashMap::new(),
                    durability: None,
                },
            )
            .unwrap();
            core.add_source_with_metadata(
                unrelated,
                HashMap::from([
                    ("pluginId".into(), "other-plugin".into()),
                    ("pluginVersion".into(), "1.2.3".into()),
                ]),
            )
            .await
            .unwrap();
            core.start().await.unwrap();
            tokio::time::timeout(
                Duration::from_secs(10),
                core.computation_component("shared-source")
                    .unwrap()
                    .wait_started(),
            )
            .await
            .unwrap()
            .unwrap();
            tokio::time::timeout(
                Duration::from_secs(10),
                core.computation_component("shared-reaction")
                    .unwrap()
                    .wait_created(),
            )
            .await
            .unwrap()
            .unwrap();
            registry.add(instance_id.into(), core).await.unwrap();
        }
        let dependents = collect_plugin_dependents(&registry, "fixture-plugin")
            .await
            .unwrap();
        assert_eq!(dependents.len(), 4);
        for instance_id in ["first", "second"] {
            for (kind, id, running) in [
                ("source", "shared-source", true),
                ("reaction", "shared-reaction", false),
            ] {
                assert!(
                    dependents.contains(&json!({
                        "instanceId":instance_id, "componentId":id, "componentType":kind,
                        "kind":"application", "running":running
                    })),
                    "{dependents:?}"
                );
            }
        }
        assert!(!serde_json::to_string(&dependents)
            .unwrap()
            .contains("plugin-secret-do-not-echo"));
        for (_, core) in registry.list().await {
            core.shutdown().await.unwrap();
        }
    }
}
