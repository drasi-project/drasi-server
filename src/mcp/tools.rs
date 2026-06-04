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

//! MCP tool definitions for Drasi Server.
//!
//! Each tool maps to one or more REST API operations on the locally running
//! Drasi Server. Tools are thin wrappers that forward to the HTTP API (started
//! on demand by [`super::DrasiMcpServer::ensure_started`]) so they reuse all of
//! the existing validation, persistence, and error-mapping logic.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router, ErrorData as McpError};
use schemars::JsonSchema;
use serde::Deserialize;

use super::DrasiMcpServer;

/// Arguments for the `open_admin_ui` startup tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenAdminUiArgs {
    /// Path to the Drasi Server configuration file to boot the server against.
    /// If omitted, the `--config` value provided when the MCP server was
    /// launched is used.
    #[serde(default)]
    pub config_path: Option<String>,
}

/// Identifies a target instance for component operations. When `instance_id` is
/// omitted, the first (default) instance is targeted via the convenience routes.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct InstanceScope {
    /// Optional DrasiLib instance id. Defaults to the first instance.
    #[serde(default)]
    pub instance_id: Option<String>,
}

/// Targets a single named component within an (optional) instance.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComponentRef {
    /// Optional DrasiLib instance id. Defaults to the first instance.
    #[serde(default)]
    pub instance_id: Option<String>,
    /// The id of the component (source / query / reaction).
    pub id: String,
}

/// Creates a component from a raw definition body.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateComponentArgs {
    /// Optional DrasiLib instance id. Defaults to the first instance.
    #[serde(default)]
    pub instance_id: Option<String>,
    /// The component definition (same JSON body accepted by the REST API
    /// `POST` endpoint for this component type).
    pub definition: serde_json::Value,
}

/// Upserts a component (create or update) at a specific id via PUT.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertComponentArgs {
    /// Optional DrasiLib instance id. Defaults to the first instance.
    #[serde(default)]
    pub instance_id: Option<String>,
    /// The id of the component to upsert. Must match the `id` in `definition`
    /// if that body carries one.
    pub id: String,
    /// The component definition (same JSON body accepted by the REST API
    /// `PUT` endpoint for this component type).
    pub definition: serde_json::Value,
}

/// Identifies a catalog solution by id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SolutionRef {
    /// The catalog solution id.
    pub id: String,
}

/// Deploys a catalog solution into an (optional) instance.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeploySolutionArgs {
    /// Optional DrasiLib instance id. Defaults to the first instance.
    #[serde(default)]
    pub instance_id: Option<String>,
    /// The solution deployment body accepted by the REST API.
    pub body: serde_json::Value,
}

/// Installs a plugin from a remote OCI registry.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct InstallPluginArgs {
    /// The plugin install body accepted by `POST /api/v1/plugins/install`.
    pub body: serde_json::Value,
}

/// Builds the component route prefix for an optional instance scope.
fn comp_prefix(instance_id: &Option<String>) -> String {
    match instance_id {
        Some(id) => format!("/api/v1/instances/{id}"),
        None => "/api/v1".to_string(),
    }
}

/// Validates that a `definition` body's `id` field (if present) matches the
/// path `id` used for an upsert. Returns a clear MCP error on mismatch so the
/// caller fails fast before hitting the API.
fn check_upsert_id(definition: &serde_json::Value, id: &str) -> Result<(), McpError> {
    if let Some(body_id) = definition.get("id").and_then(|v| v.as_str()) {
        if body_id != id {
            return Err(McpError::invalid_params(
                "The `id` in the definition body does not match the target `id`",
                Some(serde_json::json!({ "targetId": id, "bodyId": body_id })),
            ));
        }
    }
    Ok(())
}

#[tool_router(vis = "pub(crate)")]
impl DrasiMcpServer {
    // ---- UI app / lifecycle -------------------------------------------------

    #[tool(
        description = "Boot the Drasi Server (if not already running) against a config file and \
        render its admin web UI as an MCP app. Returns an MCP-UI resource pointing at the local \
        admin UI plus the base URL. Call this first before using other tools."
    )]
    pub async fn open_admin_ui(
        &self,
        Parameters(args): Parameters<OpenAdminUiArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.open_admin_ui_impl(args.config_path).await
    }

    #[tool(description = "Stop the running Drasi Server instance started via open_admin_ui.")]
    pub async fn stop_server(&self) -> Result<CallToolResult, McpError> {
        self.stop_server_impl().await
    }

    // ---- Instances ----------------------------------------------------------

    #[tool(description = "List all DrasiLib instances on the running server.")]
    pub async fn list_instances(&self) -> Result<CallToolResult, McpError> {
        self.api_get("/api/v1/instances").await
    }

    #[tool(description = "Get the configuration snapshot of a DrasiLib instance.")]
    pub async fn get_instance_snapshot(
        &self,
        Parameters(args): Parameters<InstanceScope>,
    ) -> Result<CallToolResult, McpError> {
        let id = args.instance_id.unwrap_or_default();
        self.api_get(&format!("/api/v1/instances/{id}/snapshot"))
            .await
    }

    // ---- Sources ------------------------------------------------------------

    #[tool(description = "List sources for an instance (default instance if not specified).")]
    pub async fn list_sources(
        &self,
        Parameters(args): Parameters<InstanceScope>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!("{}/sources", comp_prefix(&args.instance_id)))
            .await
    }

    #[tool(description = "Get a single source's status and configuration.")]
    pub async fn get_source(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!(
            "{}/sources/{}",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    #[tool(description = "Create a source from a definition body.")]
    pub async fn create_source(
        &self,
        Parameters(args): Parameters<CreateComponentArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!("{}/sources", comp_prefix(&args.instance_id)),
            Some(args.definition),
        )
        .await
    }

    #[tool(
        description = "Upsert a source (create or update) at a specific id via PUT. The definition \
        body's id, if present, must match the target id."
    )]
    pub async fn upsert_source(
        &self,
        Parameters(args): Parameters<UpsertComponentArgs>,
    ) -> Result<CallToolResult, McpError> {
        check_upsert_id(&args.definition, &args.id)?;
        self.api_put(
            &format!("{}/sources/{}", comp_prefix(&args.instance_id), args.id),
            Some(args.definition),
        )
        .await
    }

    #[tool(description = "Delete a source by id.")]
    pub async fn delete_source(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_delete(&format!(
            "{}/sources/{}",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    #[tool(description = "Start a source by id.")]
    pub async fn start_source(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!(
                "{}/sources/{}/start",
                comp_prefix(&args.instance_id),
                args.id
            ),
            None,
        )
        .await
    }

    #[tool(description = "Stop a source by id.")]
    pub async fn stop_source(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!(
                "{}/sources/{}/stop",
                comp_prefix(&args.instance_id),
                args.id
            ),
            None,
        )
        .await
    }

    // ---- Queries ------------------------------------------------------------

    #[tool(description = "List queries for an instance (default instance if not specified).")]
    pub async fn list_queries(
        &self,
        Parameters(args): Parameters<InstanceScope>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!("{}/queries", comp_prefix(&args.instance_id)))
            .await
    }

    #[tool(description = "Get a single query's configuration.")]
    pub async fn get_query(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!(
            "{}/queries/{}",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    #[tool(description = "Create a query from a definition body.")]
    pub async fn create_query(
        &self,
        Parameters(args): Parameters<CreateComponentArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!("{}/queries", comp_prefix(&args.instance_id)),
            Some(args.definition),
        )
        .await
    }

    #[tool(description = "Delete a query by id.")]
    pub async fn delete_query(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_delete(&format!(
            "{}/queries/{}",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    #[tool(description = "Start a query by id.")]
    pub async fn start_query(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!(
                "{}/queries/{}/start",
                comp_prefix(&args.instance_id),
                args.id
            ),
            None,
        )
        .await
    }

    #[tool(description = "Stop a query by id.")]
    pub async fn stop_query(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!(
                "{}/queries/{}/stop",
                comp_prefix(&args.instance_id),
                args.id
            ),
            None,
        )
        .await
    }

    #[tool(description = "Get the current result set of a query by id.")]
    pub async fn get_query_results(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!(
            "{}/queries/{}/results",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    // ---- Reactions ----------------------------------------------------------

    #[tool(description = "List reactions for an instance (default instance if not specified).")]
    pub async fn list_reactions(
        &self,
        Parameters(args): Parameters<InstanceScope>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!("{}/reactions", comp_prefix(&args.instance_id)))
            .await
    }

    #[tool(description = "Get a single reaction's status and configuration.")]
    pub async fn get_reaction(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!(
            "{}/reactions/{}",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    #[tool(description = "Create a reaction from a definition body.")]
    pub async fn create_reaction(
        &self,
        Parameters(args): Parameters<CreateComponentArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!("{}/reactions", comp_prefix(&args.instance_id)),
            Some(args.definition),
        )
        .await
    }

    #[tool(
        description = "Upsert a reaction (create or update) at a specific id via PUT. The \
        definition body's id, if present, must match the target id."
    )]
    pub async fn upsert_reaction(
        &self,
        Parameters(args): Parameters<UpsertComponentArgs>,
    ) -> Result<CallToolResult, McpError> {
        check_upsert_id(&args.definition, &args.id)?;
        self.api_put(
            &format!("{}/reactions/{}", comp_prefix(&args.instance_id), args.id),
            Some(args.definition),
        )
        .await
    }

    #[tool(description = "Delete a reaction by id.")]
    pub async fn delete_reaction(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_delete(&format!(
            "{}/reactions/{}",
            comp_prefix(&args.instance_id),
            args.id
        ))
        .await
    }

    #[tool(description = "Start a reaction by id.")]
    pub async fn start_reaction(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!(
                "{}/reactions/{}/start",
                comp_prefix(&args.instance_id),
                args.id
            ),
            None,
        )
        .await
    }

    #[tool(description = "Stop a reaction by id.")]
    pub async fn stop_reaction(
        &self,
        Parameters(args): Parameters<ComponentRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!(
                "{}/reactions/{}/stop",
                comp_prefix(&args.instance_id),
                args.id
            ),
            None,
        )
        .await
    }

    // ---- Plugins ------------------------------------------------------------

    #[tool(description = "List all loaded plugins with their status.")]
    pub async fn list_plugins(&self) -> Result<CallToolResult, McpError> {
        self.api_get("/api/v1/plugins").await
    }

    #[tool(description = "List all available plugin kinds (sources, reactions, bootstrappers).")]
    pub async fn list_plugin_kinds(&self) -> Result<CallToolResult, McpError> {
        self.api_get("/api/v1/plugins/kinds").await
    }

    #[tool(description = "Install a plugin from a remote OCI registry.")]
    pub async fn install_plugin(
        &self,
        Parameters(args): Parameters<InstallPluginArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post("/api/v1/plugins/install", Some(args.body))
            .await
    }

    // ---- Solutions catalog --------------------------------------------------

    #[tool(description = "List available catalog solutions.")]
    pub async fn list_solutions(&self) -> Result<CallToolResult, McpError> {
        self.api_get("/api/v1/catalog/solutions").await
    }

    #[tool(description = "Get a catalog solution by id.")]
    pub async fn get_solution(
        &self,
        Parameters(args): Parameters<SolutionRef>,
    ) -> Result<CallToolResult, McpError> {
        self.api_get(&format!("/api/v1/catalog/solutions/{}", args.id))
            .await
    }

    #[tool(
        description = "Deploy a catalog solution into an instance (default instance if not specified)."
    )]
    pub async fn deploy_solution(
        &self,
        Parameters(args): Parameters<DeploySolutionArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.api_post(
            &format!("{}/solutions", comp_prefix(&args.instance_id)),
            Some(args.body),
        )
        .await
    }
}
