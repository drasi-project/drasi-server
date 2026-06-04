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
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ResourceContents, ServerCapabilities,
    ServerInfo,
};
use rmcp::{tool_handler, ErrorData as McpError, ServerHandler, ServiceExt};
use tokio::sync::{watch, Mutex};

use crate::{DrasiServer, RunningServer};

/// URI advertised for the admin UI MCP-UI resource.
const ADMIN_UI_RESOURCE_URI: &str = "ui://drasi/admin";

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

    /// Implementation of the `open_admin_ui` tool.
    async fn open_admin_ui_impl(
        &self,
        config_path: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        let base_url = self.ensure_started(config_path).await?;
        let ui_url = format!("{base_url}/ui/");

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

        // MCP-UI resource (text/uri-list) for hosts that render UI apps.
        let ui_resource = Content::resource(ResourceContents::TextResourceContents {
            uri: ADMIN_UI_RESOURCE_URI.to_string(),
            mime_type: Some("text/uri-list".to_string()),
            text: ui_url.clone(),
            meta: None,
        });

        // JSON fallback for hosts that don't render UI resources.
        let info = Content::json(serde_json::json!({
            "uiUrl": ui_url,
            "baseUrl": base_url,
            "configLoaded": config_loaded,
        }))?;

        Ok(CallToolResult::success(vec![ui_resource, info]))
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

#[tool_handler]
impl ServerHandler for DrasiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Drasi Server MCP. Call `open_admin_ui` (optionally with a `config_path`) first to \
                 boot the server and render its admin UI as an app. Then use the source/query/\
                 reaction, plugin, and solution tools to manage it."
                    .to_string(),
            ),
        }
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
