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

//! Stdio-based MCP (Model Context Protocol) server mode for Drasi Server.
//!
//! When launched with the `mcp` subcommand, the process speaks JSON-RPC over
//! stdin/stdout instead of starting the HTTP API directly. The DrasiLib runtime
//! and the web API/UI are booted **on demand** when the `open_admin_ui` tool is
//! invoked with a config path; that tool returns an MCP-UI resource pointing at
//! the local admin UI so MCP-UI-capable hosts can render it as an app. The
//! remaining tools wrap the REST API for programmatic management.
//!
//! ## stdout hygiene
//!
//! The MCP protocol owns the process's real stdout. drasi-lib installs a tracing
//! layer that writes to stdout, so before any logging is initialised
//! [`run_mcp_server`] duplicates the real stdout for the MCP transport and
//! redirects file descriptor 1 to stderr. All stray stdout (logs, banners) then
//! lands on stderr, keeping the JSON-RPC stream clean.

mod tools;

#[cfg(unix)]
use std::os::unix::io::FromRawFd;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rmcp::handler::server::tool::{ToolCallContext, ToolRouter};
use rmcp::model::{
    AnnotateAble, CallToolRequestParam, CallToolResult, Content, Implementation,
    ListResourcesResult, ListToolsResult, Meta, PaginatedRequestParam, ProtocolVersion,
    RawResource, ReadResourceRequestParam, ReadResourceResult, Resource, ResourceContents,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use tokio::sync::{watch, Mutex};

use crate::{DrasiServer, RunningServer};

/// URI advertised for the admin UI MCP App resource.
const ADMIN_UI_RESOURCE_URI: &str = "ui://drasi/admin";

/// Human-readable name for the admin UI resource.
const ADMIN_UI_RESOURCE_NAME: &str = "drasi_admin_ui";

/// MIME type for MCP Apps HTML resources (SEP-1865 / `io.modelcontextprotocol/ui`).
const MCP_APP_MIME_TYPE: &str = "text/html;profile=mcp-app";

/// Convert a server base URL into one safe to embed in the MCP App resource.
///
/// Hosts (e.g. Claude Desktop) canonicalize `127.0.0.1` to `localhost` when
/// constructing the sandbox CSP from `_meta.ui.csp`. If the embedded app then
/// addresses the server as `http://127.0.0.1:<port>`, its requests fail the
/// `connect-src http://localhost:<port>` / `script-src ...` checks because
/// `127.0.0.1` and `localhost` are distinct CSP origins. We therefore present
/// the server to the sandbox as `localhost` so the app's origin matches the CSP
/// the host derives. The server binds *both* loopback families (`127.0.0.1` and
/// `[::1]`, see `server.rs`), so `localhost` is reachable however the host's
/// browser resolves it.
fn sandbox_base_url(base_url: &str) -> String {
    base_url.replace("127.0.0.1", "localhost")
}

/// Build the HTML for the admin UI MCP App resource.
///
/// MCP App hosts (e.g. Claude Desktop) render this HTML in a sandboxed iframe on
/// a *different* origin than the Drasi Server. Observed Claude Desktop behavior
/// (from its renderer logs) drives the addressing split below:
///
/// * The host loads cross-origin **subresources** (the entry `<script src>`,
///   the stylesheet, the bridge) fine from an explicit `127.0.0.1` origin — this
///   is the configuration that provably executed the SPA.
/// * The host **normalizes `connect-src` to `localhost`**, so the app's runtime
///   API/SSE calls must target `localhost` (not `127.0.0.1`) or they are blocked.
///
/// So the assets load via static tags from `base_url` (the explicit `127.0.0.1`
/// origin, declared in `_meta.ui.csp.resourceDomains`), and the injected bridge
/// rewrites the app's root-relative API/SSE requests to the `localhost` variant
/// (declared in `connectDomains`). The server binds *both* loopback families
/// (`127.0.0.1` and `[::1]`, see `server.rs`) and answers with permissive CORS
/// (`Access-Control-Allow-Origin: *`), so both succeed.
fn admin_ui_resource_html(base_url: &str) -> String {
    match crate::ui_assets::index_html() {
        Some(raw) => inline_admin_ui_html(&raw, base_url),
        None => fallback_admin_ui_html(base_url),
    }
}

/// Transform the SPA `index.html` into an MCP-App document: rewrite
/// root-relative `/ui/` asset URLs to absolute `<base_url>/ui/...` (loaded from
/// the explicit `127.0.0.1` origin), strip the bare inline bootstrap `<script>`
/// (the bridge reproduces its behavior), and inject the external bridge
/// `<script>` so it runs before the deferred app module.
fn inline_admin_ui_html(raw: &str, base_url: &str) -> String {
    let abs_assets = raw.replace("\"/ui/", &format!("\"{base_url}/ui/"));
    let no_inline = strip_bare_inline_scripts(&abs_assets);
    let bridge_tag = format!(
        "<head>\n  <script src=\"{base_url}{path}\"></script>",
        path = crate::ui_assets::MCP_BRIDGE_PATH
    );
    if no_inline.contains("<head>") {
        no_inline.replacen("<head>", &bridge_tag, 1)
    } else {
        format!("{bridge_tag}\n{no_inline}")
    }
}

/// Remove every bare `<script>...</script>` block (one with no attributes).
/// Attributed scripts such as `<script type="module" ...>` and
/// `<script src=...>` are preserved.
fn strip_bare_inline_scripts(html: &str) -> String {
    const OPEN: &str = "<script>";
    const CLOSE: &str = "</script>";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(CLOSE) {
            Some(end_rel) => {
                let end = start + end_rel + CLOSE.len();
                rest = &rest[end..];
            }
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Fallback shell used when the SPA assets are not present (e.g. a bare
/// `cargo build` that never built the UI). `open_admin_ui` already fails loudly
/// in that case, but `resources/read` must still return valid HTML.
fn fallback_admin_ui_html(base_url: &str) -> String {
    let ui_url = format!("{base_url}/ui/");
    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<title>Drasi Server Admin</title>\n\
</head>\n\
<body style=\"font-family:system-ui,sans-serif;padding:16px;background:#171717;color:#fafafa\">\n\
<p>The admin UI assets were not found. Build the UI (<code>make build-ui</code>) and restart, \
then open <a href=\"{ui_url}\">{ui_url}</a>.</p>\n\
</body>\n\
</html>\n"
    )
}

/// Build the `_meta` for the admin UI resource. Scripts/styles load from the
/// explicit `127.0.0.1` origin (`base_url`, in `resourceDomains`) because the
/// host honors that for subresources; API/SSE target the `localhost` variant
/// (in `connectDomains`) because the host normalizes `connect-src` to
/// `localhost`. Both origins are listed in each field for robustness.
fn admin_ui_resource_meta(base_url: &str) -> Meta {
    let api_base = sandbox_base_url(base_url);
    let mut meta = Meta::new();
    meta.insert(
        "ui".to_string(),
        serde_json::json!({
            "csp": {
                "connectDomains": [api_base, base_url],
                "resourceDomains": [base_url, api_base],
            },
            "prefersBorder": false
        }),
    );
    meta
}

/// Build the embedded resource content block for the admin UI MCP App.
///
/// `base_url` is the raw server origin (`http://127.0.0.1:<port>`); assets load
/// from it directly while the bridge/meta derive the `localhost` API origin.
fn admin_ui_resource_contents(base_url: &str) -> ResourceContents {
    ResourceContents::TextResourceContents {
        uri: ADMIN_UI_RESOURCE_URI.to_string(),
        mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
        text: admin_ui_resource_html(base_url),
        meta: Some(admin_ui_resource_meta(base_url)),
    }
}

/// Startup options for MCP mode, sourced from the CLI.
#[derive(Debug, Clone)]
pub struct McpServerOptions {
    /// Default config path used when a tool does not provide one.
    pub config: Option<PathBuf>,
    /// Port for the local HTTP API/UI (0 = OS-assigned ephemeral port).
    pub port: u16,
    /// Directory to scan for plugin shared libraries.
    pub plugins_dir: PathBuf,
    /// Disable cosign signature verification for plugins.
    pub skip_verification: bool,
}

/// Lifecycle slot for the lazily-booted Drasi runtime.
///
/// A single-flight state machine: only one boot is ever in progress
/// (`Starting`), concurrent callers await the transition rather than racing a
/// second boot, and the slot only becomes `Running` once the API is health-ready.
#[derive(Default)]
enum ServerSlot {
    /// No server booted.
    #[default]
    Stopped,
    /// A boot is in progress; other callers wait for the next transition.
    Starting,
    /// A booted, health-ready server.
    Running {
        running: RunningServer,
        base_url: String,
        config_path: Option<PathBuf>,
    },
}

/// The MCP server handler for Drasi Server.
#[derive(Clone)]
pub struct DrasiMcpServer {
    tool_router: ToolRouter<Self>,
    http: reqwest::Client,
    options: Arc<McpServerOptions>,
    state: Arc<Mutex<ServerSlot>>,
    /// Bumped on every [`ServerSlot`] transition to wake callers awaiting a
    /// `Starting -> Running/Stopped` change. Subscribers are taken while holding
    /// the state lock to avoid lost wakeups.
    progress: Arc<watch::Sender<u64>>,
}

impl DrasiMcpServer {
    /// Create a new handler. Does not boot the underlying server.
    pub fn new(options: McpServerOptions) -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let (progress, _) = watch::channel(0u64);

        Self {
            tool_router: Self::tool_router(),
            http,
            options: Arc::new(options),
            state: Arc::new(Mutex::new(ServerSlot::Stopped)),
            progress: Arc::new(progress),
        }
    }

    /// Wake any callers awaiting a state transition.
    fn notify_change(&self) {
        self.progress.send_modify(|v| *v = v.wrapping_add(1));
    }

    /// Resolve the effective config path from the tool argument or CLI default.
    /// Returns `None` when neither is set, in which case the server boots from an
    /// in-memory default configuration (empty, non-persistent).
    fn effective_config(&self, config_path: Option<String>) -> Option<PathBuf> {
        config_path
            .map(PathBuf::from)
            .or_else(|| self.options.config.clone())
    }

    /// Boot the server against `config_path` if it is not already running, and
    /// return its base URL.
    ///
    /// Single-flight: if a boot is already in progress, callers await it rather
    /// than starting a second one. If a server is already running against a
    /// *different* config, this returns an error asking the caller to
    /// `stop_server` first (no implicit reboot, which would abort in-flight
    /// requests). The slot only becomes `Running` once the API is health-ready.
    async fn ensure_started(&self, config_path: Option<String>) -> Result<String, McpError> {
        let effective = self.effective_config(config_path);

        loop {
            let mut slot = self.state.lock().await;
            match &*slot {
                ServerSlot::Running {
                    base_url,
                    config_path,
                    ..
                } => {
                    if config_path == &effective {
                        return Ok(base_url.clone());
                    }
                    let running_desc = match config_path {
                        Some(p) => format!("'{}'", p.display()),
                        None => "an in-memory default configuration".to_string(),
                    };
                    return Err(McpError::invalid_request(
                        format!(
                            "A Drasi Server is already running against {running_desc}. Call stop_server before starting a different config.",
                        ),
                        None,
                    ));
                }
                ServerSlot::Starting => {
                    // Subscribe while holding the lock so we cannot miss the
                    // transition the booting task publishes after we release it.
                    let mut rx = self.progress.subscribe();
                    drop(slot);
                    let _ = rx.changed().await;
                    continue;
                }
                ServerSlot::Stopped => {
                    *slot = ServerSlot::Starting;
                    self.notify_change();
                    drop(slot);

                    let booted = self.boot(effective.as_deref()).await;

                    let mut slot = self.state.lock().await;
                    match booted {
                        Ok((running, base_url)) => {
                            *slot = ServerSlot::Running {
                                running,
                                base_url: base_url.clone(),
                                config_path: effective.clone(),
                            };
                            self.notify_change();
                            return Ok(base_url);
                        }
                        Err(e) => {
                            *slot = ServerSlot::Stopped;
                            self.notify_change();
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    /// Build, start and health-check a server. On readiness failure the
    /// partially-started server is shut down before returning the error.
    async fn boot(
        &self,
        effective: Option<&std::path::Path>,
    ) -> Result<(RunningServer, String), McpError> {
        let server = DrasiServer::new_with_bind_override(
            effective.map(|p| p.to_path_buf()),
            self.options.plugins_dir.clone(),
            self.options.skip_verification,
            true, // UI is required for the admin-UI tool
            "127.0.0.1",
            self.options.port,
        )
        .await
        .map_err(|e| {
            McpError::internal_error(
                "Failed to initialise Drasi Server from config",
                Some(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

        let running = server.start().await.map_err(|e| {
            McpError::internal_error(
                "Failed to start Drasi Server",
                Some(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

        let base_url = match running.base_url() {
            Some(url) => url,
            None => {
                let _ = running.shutdown().await;
                return Err(McpError::internal_error(
                    "Drasi Server started without a web API",
                    None,
                ));
            }
        };

        if let Err(e) = self.wait_for_ready(&base_url).await {
            let _ = running.shutdown().await;
            return Err(e);
        }

        Ok((running, base_url))
    }

    /// Poll the health endpoint until the API responds or a timeout elapses.
    async fn wait_for_ready(&self, base_url: &str) -> Result<(), McpError> {
        let url = format!("{base_url}/health");
        let mut last_err: Option<String> = None;
        for _ in 0..100 {
            match self.http.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                Ok(resp) => last_err = Some(format!("HTTP {}", resp.status().as_u16())),
                Err(e) => last_err = Some(e.to_string()),
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(McpError::internal_error(
            "Drasi Server did not become ready within the timeout",
            Some(serde_json::json!({ "lastError": last_err })),
        ))
    }

    /// Check whether the admin UI is actually served (assets are present).
    /// A bare `cargo build` leaves `ui/dist` empty, so `/ui/` would 404.
    async fn ui_is_available(&self, base_url: &str) -> bool {
        let url = format!("{base_url}/ui/");
        matches!(self.http.get(&url).send().await, Ok(resp) if resp.status().is_success())
    }

    /// Return the base URL of the running server. If a boot is in progress this
    /// awaits it; if nothing is running it instructs the caller to start first.
    async fn require_base_url(&self) -> Result<String, McpError> {
        loop {
            let slot = self.state.lock().await;
            match &*slot {
                ServerSlot::Running { base_url, .. } => return Ok(base_url.clone()),
                ServerSlot::Starting => {
                    let mut rx = self.progress.subscribe();
                    drop(slot);
                    let _ = rx.changed().await;
                    continue;
                }
                ServerSlot::Stopped => {
                    return Err(McpError::invalid_request(
                        "Drasi Server is not started. Call open_admin_ui with a config_path first.",
                        None,
                    ))
                }
            }
        }
    }

    /// Return the base URL only if a server is currently running, without
    /// waiting for an in-progress boot. Used by `resources/read`, which a host
    /// may call before the boot tool completes (returns `None` in that case).
    async fn current_base_url(&self) -> Option<String> {
        let slot = self.state.lock().await;
        match &*slot {
            ServerSlot::Running { base_url, .. } => Some(base_url.clone()),
            _ => None,
        }
    }

    /// Implementation of the `open_admin_ui` tool.
    async fn open_admin_ui_impl(
        &self,
        config_path: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let base_url = self.ensure_started(config_path).await?;
        let ui_url = format!("{base_url}/ui/");

        // Guard against returning a dead UI URL: a bare `cargo build` does not
        // build the web assets, so `/ui/` would 404. Fail loudly with build
        // instructions instead of handing the host a broken app.
        if !self.ui_is_available(&base_url).await {
            return Err(McpError::internal_error(
                "The admin UI assets are not available, so the UI cannot be rendered. \
                 Build the UI first (run `make build-ui`, or use a release build via \
                 `make build-release`) and restart the MCP server.",
                Some(serde_json::json!({ "uiUrl": ui_url, "baseUrl": base_url })),
            ));
        }

        let config_loaded = {
            let slot = self.state.lock().await;
            match &*slot {
                ServerSlot::Running { config_path, .. } => match config_path {
                    Some(p) => p.display().to_string(),
                    None => "(in-memory default configuration)".to_string(),
                },
                _ => String::new(),
            }
        };

        // Rendering is driven by the `open_admin_ui` tool's
        // `_meta.ui.resourceUri` (declared in `list_tools`): the host fetches
        // `ui://drasi/admin` via `resources/read` and renders it as an MCP App.
        // The tool result itself is plain text/JSON (matching the MCP Apps
        // reference servers), with `_meta.ui.resourceUri` also set on the result
        // so hosts that associate the view via the tool call can find it.
        let summary = Content::text(format!(
            "Drasi Server admin UI is ready at {ui_url} (config: {config_loaded})."
        ));
        let info = Content::json(serde_json::json!({
            "uiUrl": ui_url,
            "baseUrl": base_url,
            "configLoaded": config_loaded,
        }))?;

        let mut result = CallToolResult::success(vec![summary, info]);
        let mut meta = Meta::new();
        meta.insert(
            "ui".to_string(),
            serde_json::json!({ "resourceUri": ADMIN_UI_RESOURCE_URI }),
        );
        result.meta = Some(meta);
        Ok(result)
    }

    /// Implementation of the `stop_server` tool.
    async fn stop_server_impl(&self) -> Result<CallToolResult, McpError> {
        loop {
            let mut slot = self.state.lock().await;
            match &*slot {
                ServerSlot::Starting => {
                    // Wait for the in-progress boot to settle before stopping.
                    let mut rx = self.progress.subscribe();
                    drop(slot);
                    let _ = rx.changed().await;
                    continue;
                }
                ServerSlot::Running { .. } => {
                    let taken = std::mem::replace(&mut *slot, ServerSlot::Stopped);
                    self.notify_change();
                    drop(slot);
                    if let ServerSlot::Running { running, .. } = taken {
                        running.shutdown().await.map_err(|e| {
                            McpError::internal_error(
                                "Failed to stop Drasi Server",
                                Some(serde_json::json!({ "error": e.to_string() })),
                            )
                        })?;
                    }
                    return Ok(CallToolResult::success(vec![Content::text(
                        "Drasi Server stopped.",
                    )]));
                }
                ServerSlot::Stopped => {
                    return Ok(CallToolResult::success(vec![Content::text(
                        "Drasi Server was not running.",
                    )]))
                }
            }
        }
    }

    /// Perform a GET against the local API and wrap the response as a tool result.
    async fn api_get(&self, path: &str) -> Result<CallToolResult, McpError> {
        let base = self.require_base_url().await?;
        let resp = self
            .http
            .get(format!("{base}{path}"))
            .send()
            .await
            .map_err(map_reqwest_err)?;
        response_to_result(resp).await
    }

    /// Perform a POST against the local API and wrap the response as a tool result.
    async fn api_post(
        &self,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<CallToolResult, McpError> {
        let base = self.require_base_url().await?;
        let mut req = self.http.post(format!("{base}{path}"));
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await.map_err(map_reqwest_err)?;
        response_to_result(resp).await
    }

    /// Perform a PUT against the local API and wrap the response as a tool result.
    async fn api_put(
        &self,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<CallToolResult, McpError> {
        let base = self.require_base_url().await?;
        let mut req = self.http.put(format!("{base}{path}"));
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await.map_err(map_reqwest_err)?;
        response_to_result(resp).await
    }

    /// Perform a DELETE against the local API and wrap the response as a tool result.
    async fn api_delete(&self, path: &str) -> Result<CallToolResult, McpError> {
        let base = self.require_base_url().await?;
        let resp = self
            .http
            .delete(format!("{base}{path}"))
            .send()
            .await
            .map_err(map_reqwest_err)?;
        response_to_result(resp).await
    }
}

/// Convert an HTTP response into a [`CallToolResult`], marking non-2xx
/// responses as tool errors while still surfacing the response body.
///
/// On non-2xx responses the body is parsed as the API `ErrorResponse`
/// (`{ code, message, details }`) and re-emitted as a normalized JSON object
/// (`{ httpStatus, code, message, details }`) preserving the full `details`,
/// alongside a short text summary. If the body isn't the expected shape the raw
/// text + status is surfaced instead.
async fn response_to_result(resp: reqwest::Response) -> Result<CallToolResult, McpError> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if status.is_success() {
        let content = if text.is_empty() {
            Content::text(format!("HTTP {}", status.as_u16()))
        } else {
            Content::text(text)
        };
        return Ok(CallToolResult::success(vec![content]));
    }

    // Non-2xx: try to surface the structured API error shape.
    let http_status = status.as_u16();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
        // Recognize the API ErrorResponse shape: an object carrying at least a
        // `code` and `message`.
        let has_error_shape = parsed
            .as_object()
            .map(|o| o.contains_key("code") && o.contains_key("message"))
            .unwrap_or(false);
        if has_error_shape {
            let code = parsed.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let message = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("");
            let normalized = serde_json::json!({
                "httpStatus": http_status,
                "code": code,
                "message": message,
                "details": parsed.get("details").cloned().unwrap_or(serde_json::Value::Null),
            });
            let summary = Content::text(format!("HTTP {http_status} [{code}]: {message}"));
            let json = Content::json(normalized).map_err(|e| {
                McpError::internal_error(
                    "Failed to serialize structured error",
                    Some(serde_json::json!({ "error": e.to_string() })),
                )
            })?;
            return Ok(CallToolResult::error(vec![summary, json]));
        }
    }

    // Fallback: raw body (or just the status when empty).
    let content = if text.is_empty() {
        Content::text(format!("HTTP {http_status}"))
    } else {
        Content::text(format!("HTTP {http_status}: {text}"))
    };
    Ok(CallToolResult::error(vec![content]))
}

fn map_reqwest_err(e: reqwest::Error) -> McpError {
    McpError::internal_error(
        "Request to local Drasi API failed",
        Some(serde_json::json!({ "error": e.to_string() })),
    )
}

impl ServerHandler for DrasiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Drasi Server MCP. Call `open_admin_ui` (optionally with a `config_path`) first to \
                 boot the server and render its admin UI as an app. Then use the source/query/\
                 reaction, plugin, and solution tools to manage it."
                    .to_string(),
            ),
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        // Link the `open_admin_ui` tool to its MCP App resource so that
        // `io.modelcontextprotocol/ui` hosts render the admin UI when the tool
        // is invoked (SEP-1865 `_meta.ui.resourceUri`).
        for tool in &mut tools {
            if tool.name.as_ref() == "open_admin_ui" {
                let mut meta = Meta::new();
                meta.insert(
                    "ui".to_string(),
                    serde_json::json!({ "resourceUri": ADMIN_UI_RESOURCE_URI }),
                );
                tool.meta = Some(meta);
            }
        }
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resource: Resource = RawResource {
            uri: ADMIN_UI_RESOURCE_URI.to_string(),
            name: ADMIN_UI_RESOURCE_NAME.to_string(),
            title: Some("Drasi Server Admin UI".to_string()),
            description: Some(
                "Interactive admin UI for the running Drasi Server, rendered as an MCP App."
                    .to_string(),
            ),
            mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
            size: None,
            icons: None,
        }
        .no_annotation();
        Ok(ListResourcesResult::with_all_items(vec![resource]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if request.uri != ADMIN_UI_RESOURCE_URI {
            return Err(McpError::resource_not_found(
                "Unknown resource",
                Some(serde_json::json!({ "uri": request.uri })),
            ));
        }

        let contents = match self.current_base_url().await {
            // Pass the raw 127.0.0.1 origin: assets load from it directly, while
            // the bridge/meta derive the localhost origin for API calls.
            Some(base_url) => admin_ui_resource_contents(&base_url),
            // The server hasn't been booted yet (host prefetched the resource).
            // Return a valid MCP App page that prompts the user to start it.
            None => ResourceContents::TextResourceContents {
                uri: ADMIN_UI_RESOURCE_URI.to_string(),
                mime_type: Some(MCP_APP_MIME_TYPE.to_string()),
                text: "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
                       <title>Drasi Server</title></head>\
                       <body style=\"font-family:system-ui,sans-serif;padding:16px;\
                       background:#171717;color:#fafafa\">\
                       <p>The Drasi Server is not running yet. Invoke the \
                       <code>open_admin_ui</code> tool to start it and load the admin UI.</p>\
                       </body></html>"
                    .to_string(),
                meta: None,
            },
        };

        Ok(ReadResourceResult {
            contents: vec![contents],
        })
    }
}

/// Run the Drasi Server in stdio MCP mode.
///
/// Redirects the real stdout to stderr (handing the original stdout to the MCP
/// transport) so logging never corrupts the JSON-RPC stream, then serves the
/// MCP protocol until the transport closes.
pub async fn run_mcp_server(options: McpServerOptions) -> Result<()> {
    // Reserve the real stdout for the JSON-RPC stream and redirect fd 1 to
    // stderr so any library/tracing output goes to stderr instead.
    let mcp_stdout = redirect_stdout_to_stderr().context("failed to set up MCP stdio")?;

    // Initialise logging AFTER the redirect so the tracing fmt layer (which
    // writes to fd 1) lands on stderr.
    if std::env::var("RUST_LOG").is_err() {
        // SAFETY: set_var runs before other threads are spawned.
        unsafe {
            std::env::set_var("RUST_LOG", "info,oci_client=error");
        }
    }
    // Log every HTTP request the booted server receives. In MCP mode the server
    // exists only to back the admin UI MCP App, so per-request logging (to
    // stderr, captured by the host's MCP server log) is the ground-truth view of
    // what the host's webview actually fetches (assets, bridge, API).
    // SAFETY: set_var runs before other threads are spawned.
    unsafe {
        std::env::set_var("DRASI_HTTP_LOG", "1");
    }
    drasi_lib::get_or_init_global_registry();

    log::info!("Starting Drasi Server in MCP (stdio) mode");

    let server = DrasiMcpServer::new(options);
    let state = server.state.clone();

    let reader = tokio::io::stdin();
    let service = server
        .serve((reader, mcp_stdout))
        .await
        .context("failed to start MCP service")?;

    let quit_reason = service.waiting().await.context("MCP service error")?;
    log::info!("MCP transport closed: {quit_reason:?}");

    // Gracefully tear down any booted runtime when the transport closes so the
    // DrasiLib instances, API task and plugin watcher stop cleanly.
    let taken = {
        let mut slot = state.lock().await;
        std::mem::replace(&mut *slot, ServerSlot::Stopped)
    };
    if let ServerSlot::Running { running, .. } = taken {
        if let Err(e) = running.shutdown().await {
            log::warn!("Error shutting down Drasi Server on MCP exit: {e}");
        }
    }

    Ok(())
}

/// Duplicate the current stdout (fd 1) into a fresh, close-on-exec descriptor to
/// be used by the MCP transport, then point fd 1 at stderr (fd 2). Returns a
/// tokio file wrapping the saved original stdout.
#[cfg(unix)]
fn redirect_stdout_to_stderr() -> Result<tokio::fs::File> {
    // F_DUPFD_CLOEXEC returns the lowest available fd >= the third arg, with
    // close-on-exec set so spawned plugins/children don't inherit it.
    let saved_fd = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
    if saved_fd < 0 {
        return Err(anyhow!(
            "fcntl(F_DUPFD_CLOEXEC) on stdout failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // Point fd 1 at stderr so stray stdout writes go to stderr.
    let rc = unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(saved_fd) };
        return Err(anyhow!("dup2(stderr -> stdout) failed: {err}"));
    }

    // SAFETY: saved_fd is a valid, owned fd produced by fcntl above.
    let std_file = unsafe { std::fs::File::from_raw_fd(saved_fd) };
    Ok(tokio::fs::File::from_std(std_file))
}

/// Windows equivalent of the Unix stdout redirect.
///
/// Rust's `std::io::stdout()` (and thus `tracing`) writes through the Win32
/// `STD_OUTPUT_HANDLE`, while C dependencies may write through the CRT's stdout
/// file descriptor. To keep the JSON-RPC stream clean we redirect *both*:
///
/// 1. duplicate the real stdout `HANDLE` (for the MCP transport) before
///    redirecting,
/// 2. point `STD_OUTPUT_HANDLE` at stderr's handle (covers Rust std / Win32
///    writers),
/// 3. `dup2(2, 1)` at the CRT layer (covers C-runtime writers).
///
/// This must run before any logging is initialised or stdout is first used.
#[cfg(windows)]
fn redirect_stdout_to_stderr() -> Result<tokio::fs::File> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{
        DuplicateHandle, DUPLICATE_SAME_ACCESS, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: all calls below are standard Win32 console/handle APIs operating
    // on this process's own standard handles.
    unsafe {
        let cur_stdout = GetStdHandle(STD_OUTPUT_HANDLE);
        if cur_stdout == INVALID_HANDLE_VALUE || cur_stdout.is_null() {
            return Err(anyhow!(
                "GetStdHandle(STD_OUTPUT_HANDLE) failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Duplicate the real stdout for the MCP transport (non-inheritable so
        // spawned plugins/children don't get a copy).
        let proc = GetCurrentProcess();
        let mut saved = std::ptr::null_mut();
        let ok = DuplicateHandle(
            proc,
            cur_stdout,
            proc,
            &mut saved,
            0,
            0, // bInheritHandle = false
            DUPLICATE_SAME_ACCESS,
        );
        if ok == 0 {
            return Err(anyhow!(
                "DuplicateHandle(stdout) failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Point the Win32 std output handle at stderr (Rust std / Win32 writers).
        let herr = GetStdHandle(STD_ERROR_HANDLE);
        if herr != INVALID_HANDLE_VALUE && !herr.is_null() {
            SetStdHandle(STD_OUTPUT_HANDLE, herr);
        }

        // Belt-and-braces: also redirect the CRT stdout fd to stderr so any C
        // dependency writing through file descriptor 1 lands on stderr too.
        let _ = libc::dup2(2, 1);

        // SAFETY: `saved` is a valid handle we own exactly once; wrapping it in
        // File transfers ownership (it must not be closed separately).
        let std_file = std::fs::File::from_raw_handle(saved as _);
        Ok(tokio::fs::File::from_std(std_file))
    }
}
