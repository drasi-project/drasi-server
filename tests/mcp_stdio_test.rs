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

//! Integration tests for the `drasi-server mcp` stdio MCP server mode.
//!
//! These tests drive the process over stdin/stdout with JSON-RPC frames and
//! verify:
//! - stdout carries **only** valid JSON-RPC (no banners or log lines leak in),
//! - the tool list is advertised,
//! - `open_admin_ui` boots the runtime on demand and returns an MCP-UI resource
//!   pointing at a private `127.0.0.1` URL,
//! - a CRUD tool round-trips against the live, in-process HTTP API.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::TempDir;

/// Path to the built `drasi-server` binary.
fn binary_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/target/debug/drasi-server")
}

/// A minimal MCP stdio client driving the spawned process.
struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// Every line observed on stdout (used to assert stream cleanliness).
    stdout_lines: Vec<String>,
    /// Responses read out of order are buffered here keyed by their id so a
    /// later `recv_id` can retrieve them (needed for concurrent/single-flight
    /// tests that fire multiple requests before reading any response).
    pending: std::collections::HashMap<i64, Value>,
}

impl McpClient {
    fn spawn() -> Self {
        let mut child = Command::new(binary_path())
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn drasi-server mcp");

        let stdin = child.stdin.take().expect("missing stdin");
        let stdout = BufReader::new(child.stdout.take().expect("missing stdout"));

        Self {
            child,
            stdin,
            stdout,
            stdout_lines: Vec::new(),
            pending: std::collections::HashMap::new(),
        }
    }

    fn send(&mut self, msg: &Value) {
        let line = serde_json::to_string(msg).expect("serialize");
        self.stdin
            .write_all(line.as_bytes())
            .expect("write to stdin");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush stdin");
    }

    /// Read JSON-RPC frames until one with the given id arrives. Every line read
    /// is recorded and parsed (a non-JSON line fails the test, guarding against
    /// stray stdout output). Responses for other ids are buffered so they can be
    /// retrieved by a later `recv_id`.
    fn recv_id(&mut self, id: i64) -> Value {
        if let Some(v) = self.pending.remove(&id) {
            return v;
        }
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read stdout");
            assert!(n > 0, "stdout closed before response id={id}");
            let trimmed = line.trim_end().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(&trimmed)
                .unwrap_or_else(|e| panic!("non-JSON line on stdout: {trimmed:?} ({e})"));
            self.stdout_lines.push(trimmed);
            match value.get("id").and_then(Value::as_i64) {
                Some(got) if got == id => return value,
                Some(other) => {
                    self.pending.insert(other, value);
                }
                None => {}
            }
        }
    }

    fn initialize(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }
        }));
        let resp = self.recv_id(1);
        assert!(resp.get("result").is_some(), "initialize failed: {resp}");
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    fn call(&mut self, id: i64, name: &str, args: Value) -> Value {
        self.send_call(id, name, args);
        self.recv_id(id)
    }

    /// Send a `tools/call` without waiting for the response (used to drive
    /// concurrent / single-flight scenarios).
    fn send_call(&mut self, id: i64, name: &str, args: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": args}
        }));
    }

    fn shutdown(mut self) {
        drop(self.stdin);
        // Give the process a moment to exit cleanly; kill if it lingers.
        for _ in 0..50 {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Collect the text content blocks from a successful tool result.
fn result_texts(resp: &Value) -> Vec<String> {
    resp.get("result")
        .and_then(|r| r.get("content"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|c| c.get("text").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the JSON info content block emitted by `open_admin_ui` (carries
/// `baseUrl` / `uiUrl` / `configLoaded`).
fn open_ui_info(resp: &Value) -> Value {
    result_texts(resp)
        .into_iter()
        .find_map(|t| {
            serde_json::from_str::<Value>(&t)
                .ok()
                .filter(|v| v.get("baseUrl").is_some())
        })
        .unwrap_or_else(|| panic!("missing open_admin_ui info block: {resp}"))
}

/// Extract the embedded MCP App resource object from an `open_admin_ui` result.
fn ui_resource(resp: &Value) -> Value {
    resp.get("result")
        .and_then(|r| r.get("content"))
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|c| c.get("type").and_then(Value::as_str) == Some("resource"))
        })
        .and_then(|c| c.get("resource"))
        .cloned()
        .unwrap_or_else(|| panic!("missing UI resource in result: {resp}"))
}

/// Perform a blocking HTTP/1.1 GET against `base_url` (e.g. `http://127.0.0.1:PORT`)
/// using only the standard library. Returns `(status_code, body)`.
fn http_get(base_url: &str, path: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let authority = base_url.strip_prefix("http://").expect("http base url");
    let mut stream = std::net::TcpStream::connect(authority).expect("connect to local Drasi API");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let req = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).expect("write request");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("could not parse status line: {text:?}"));
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    (status, body.to_string())
}

/// Whether a tool result is flagged as an error (`result.isError == true`).
fn is_tool_error(resp: &Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("isError"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Extract the normalized structured-error object emitted by `response_to_result`
/// for non-2xx responses. It is sent as a text content block whose body is JSON
/// carrying `httpStatus` / `code` / `message`.
fn result_error_json(resp: &Value) -> Option<Value> {
    result_texts(resp).into_iter().find_map(|t| {
        serde_json::from_str::<Value>(&t)
            .ok()
            .filter(|v| v.get("httpStatus").is_some() && v.get("code").is_some())
    })
}

fn write_config(dir: &TempDir) -> String {
    let path = dir.path().join("server.yaml");
    std::fs::write(
        &path,
        r#"apiVersion: drasi.io/v1
id: mcp-it
host: "0.0.0.0"
port: 8080
logLevel: info
persistConfig: false
sources: []
queries: []
reactions: []
"#,
    )
    .expect("write config");
    path.to_string_lossy().to_string()
}

#[test]
fn tools_list_is_advertised_and_stdout_is_clean() {
    let mut client = McpClient::spawn();
    client.initialize();

    client.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}));
    let resp = client.recv_id(2);
    let tools = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .collect();

    for expected in [
        "open_admin_ui",
        "list_sources",
        "create_query",
        "list_instances",
        "stop_server",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected}: {names:?}"
        );
    }

    client.shutdown();
}

#[test]
fn open_admin_ui_boots_server_and_crud_round_trips() {
    let dir = TempDir::new().expect("tempdir");
    let config = write_config(&dir);

    let mut client = McpClient::spawn();
    client.initialize();

    // Boot on demand and render the admin UI as an MCP App resource.
    let resp = client.call(3, "open_admin_ui", json!({"config_path": config}));

    // The embedded resource is an MCP App HTML document (SEP-1865), served as
    // `text/html;profile=mcp-app` and embedding the admin UI in an iframe.
    let resource = ui_resource(&resp);
    assert_eq!(
        resource.get("mimeType").and_then(Value::as_str),
        Some("text/html;profile=mcp-app"),
        "UI resource is not an MCP App HTML resource: {resource}"
    );
    assert_eq!(
        resource.get("uri").and_then(Value::as_str),
        Some("ui://drasi/admin")
    );
    let html = resource
        .get("text")
        .and_then(Value::as_str)
        .expect("ui resource html");
    assert!(
        html.contains("<iframe"),
        "resource HTML missing iframe: {html}"
    );
    // The CSP must permit framing the local admin origin, else the host blocks it.
    let frame_domains = resource
        .get("_meta")
        .and_then(|m| m.get("ui"))
        .and_then(|u| u.get("csp"))
        .and_then(|c| c.get("frameDomains"))
        .and_then(Value::as_array)
        .expect("resource _meta.ui.csp.frameDomains");
    assert!(
        !frame_domains.is_empty(),
        "frameDomains must declare the local UI origin"
    );

    // The JSON info block carries the localhost UI URL (forced private binding
    // regardless of the 0.0.0.0 config host).
    let info = open_ui_info(&resp);
    let base_url = info["baseUrl"].as_str().expect("baseUrl");
    let ui_url = info["uiUrl"].as_str().expect("uiUrl");
    assert!(
        base_url.starts_with("http://127.0.0.1:"),
        "UI url not localhost: {base_url}"
    );
    assert!(ui_url.ends_with("/ui/"), "unexpected UI url: {ui_url}");

    // The CRITICAL regression check: the advertised UI URL must actually serve
    // the admin SPA (a bare `cargo build` would 404 here).
    let (status, body) = http_get(base_url, "/ui/");
    assert_eq!(
        status, 200,
        "/ui/ did not serve the admin UI (got {status})"
    );
    assert!(
        body.contains("id=\"root\"") || body.to_lowercase().contains("drasi"),
        "/ui/ body is not the admin SPA: {}",
        &body[..body.len().min(200)]
    );

    // CRUD round-trip against the live API.
    let resp = client.call(
        4,
        "create_query",
        json!({"definition": {
            "id": "q1",
            "query": "MATCH (n) RETURN n",
            "queryLanguage": "Cypher",
            "sources": []
        }}),
    );
    let texts = result_texts(&resp);
    assert!(
        texts.iter().any(|t| t.contains("\"success\":true")),
        "create_query did not succeed: {resp}"
    );

    let resp = client.call(5, "list_queries", json!({}));
    let texts = result_texts(&resp);
    assert!(
        texts.iter().any(|t| t.contains("\"id\":\"q1\"")),
        "list_queries missing q1: {resp}"
    );

    let resp = client.call(6, "stop_server", json!({}));
    assert!(resp.get("result").is_some(), "stop_server failed: {resp}");

    // Every line seen on stdout must have parsed as JSON-RPC (enforced in
    // recv_id); assert we actually exercised the stream.
    assert!(
        client.stdout_lines.len() >= 5,
        "expected multiple JSON-RPC frames, saw {}",
        client.stdout_lines.len()
    );

    client.shutdown();
}

#[test]
fn runtime_bind_override_is_not_persisted() {
    // Regression test: MCP mode forces a 127.0.0.1:0 (ephemeral) bind via
    // override_bind(). With persistence enabled, a tool mutation must NOT write
    // that runtime bind address (host 127.0.0.1, port 0) back into the user's
    // config — doing so would corrupt it (port 0 is invalid on reload).
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("server.yaml");
    std::fs::write(
        &path,
        "apiVersion: drasi.io/v1\nid: mcp-persist\nhost: \"0.0.0.0\"\nport: 8080\nlogLevel: info\npersistConfig: true\nsources: []\nqueries: []\nreactions: []\n",
    )
    .expect("write config");
    let config = path.to_string_lossy().to_string();

    let mut client = McpClient::spawn();
    client.initialize();

    let resp = client.call(3, "open_admin_ui", json!({"config_path": config}));
    assert!(resp.get("result").is_some(), "open_admin_ui failed: {resp}");

    // A mutation that triggers persistence.
    let resp = client.call(
        4,
        "create_query",
        json!({"definition": {
            "id": "q1",
            "query": "MATCH (n) RETURN n",
            "queryLanguage": "Cypher",
            "sources": []
        }}),
    );
    let texts = result_texts(&resp);
    assert!(
        texts.iter().any(|t| t.contains("\"success\":true")),
        "create_query did not succeed: {resp}"
    );

    client.call(5, "stop_server", json!({}));
    client.shutdown();

    // The persisted config must retain the authored host/port, not the runtime
    // ephemeral bind override.
    let saved = std::fs::read_to_string(&path).expect("read persisted config");
    assert!(
        saved.contains("port: 8080"),
        "persisted config lost authored port:\n{saved}"
    );
    assert!(
        !saved.contains("port: 0"),
        "persisted config leaked ephemeral port 0:\n{saved}"
    );
    assert!(
        !saved.contains("127.0.0.1"),
        "persisted config leaked runtime bind host:\n{saved}"
    );
    // Sanity: the mutation actually persisted.
    assert!(saved.contains("q1"), "query was not persisted:\n{saved}");
}

#[test]
fn component_tool_before_boot_reports_not_started() {
    let mut client = McpClient::spawn();
    client.initialize();

    // A component tool invoked before open_admin_ui must return a clear MCP
    // error rather than panicking or hanging.
    let resp = client.call(2, "list_sources", json!({}));
    let is_jsonrpc_error = resp.get("error").is_some();
    let is_tool_err = is_tool_error(&resp);
    let mentions_not_started = serde_json::to_string(&resp)
        .unwrap_or_default()
        .to_lowercase()
        .contains("not started");
    assert!(
        is_jsonrpc_error || is_tool_err,
        "expected an error for tool-before-boot: {resp}"
    );
    assert!(
        mentions_not_started,
        "error should mention the server is not started: {resp}"
    );

    client.shutdown();
}

#[test]
fn open_admin_ui_with_bad_config_path_errors_without_panic() {
    let mut client = McpClient::spawn();
    client.initialize();

    let resp = client.call(
        2,
        "open_admin_ui",
        json!({"config_path": "/nonexistent/definitely/not/here.yaml"}),
    );
    let is_jsonrpc_error = resp.get("error").is_some();
    assert!(
        is_jsonrpc_error || is_tool_error(&resp),
        "bad config path should error: {resp}"
    );

    // The process must still be alive and responsive after the failed boot.
    let resp = client.call(3, "list_instances", json!({}));
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "server unresponsive after failed boot: {resp}"
    );

    client.shutdown();
}

#[test]
fn switching_config_while_running_is_rejected() {
    let dir = TempDir::new().expect("tempdir");
    let config_a = write_config(&dir);
    let path_b = dir.path().join("server-b.yaml");
    std::fs::write(
        &path_b,
        "apiVersion: drasi.io/v1\nid: mcp-b\nhost: \"0.0.0.0\"\nport: 8080\nlogLevel: info\npersistConfig: false\nsources: []\nqueries: []\nreactions: []\n",
    )
    .expect("write config b");
    let config_b = path_b.to_string_lossy().to_string();

    let mut client = McpClient::spawn();
    client.initialize();

    let resp = client.call(2, "open_admin_ui", json!({"config_path": config_a}));
    assert!(resp.get("result").is_some(), "first boot failed: {resp}");

    // A second open_admin_ui against a DIFFERENT config must be rejected, asking
    // the caller to stop_server first (no silent reboot).
    let resp = client.call(3, "open_admin_ui", json!({"config_path": config_b}));
    let body = serde_json::to_string(&resp)
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        resp.get("error").is_some() || is_tool_error(&resp),
        "config switch should error: {resp}"
    );
    assert!(
        body.contains("stop_server") || body.contains("already running"),
        "error should instruct to stop first: {resp}"
    );

    // Re-opening with the SAME config is fine (idempotent).
    let resp = client.call(4, "open_admin_ui", json!({"config_path": config_a}));
    assert!(
        resp.get("result").is_some(),
        "idempotent re-open failed: {resp}"
    );

    client.call(5, "stop_server", json!({}));
    client.shutdown();
}

#[test]
fn duplicate_create_query_surfaces_structured_error() {
    let dir = TempDir::new().expect("tempdir");
    let config = write_config(&dir);

    let mut client = McpClient::spawn();
    client.initialize();

    client.call(2, "open_admin_ui", json!({"config_path": config}));

    let def = json!({"definition": {
        "id": "dup",
        "query": "MATCH (n) RETURN n",
        "queryLanguage": "Cypher",
        "sources": []
    }});

    let resp = client.call(3, "create_query", def.clone());
    let texts = result_texts(&resp);
    assert!(
        texts.iter().any(|t| t.contains("\"success\":true")),
        "first create_query should succeed: {resp}"
    );

    // Duplicate id → 409, surfaced as a structured tool error carrying a code.
    let resp = client.call(4, "create_query", def);
    assert!(
        is_tool_error(&resp),
        "duplicate create should be isError: {resp}"
    );
    let err = result_error_json(&resp)
        .unwrap_or_else(|| panic!("expected structured error json: {resp}"));
    assert_eq!(
        err.get("httpStatus").and_then(Value::as_u64),
        Some(409),
        "expected HTTP 409: {err}"
    );
    assert!(
        err.get("code")
            .and_then(Value::as_str)
            .map(|c| !c.is_empty())
            .unwrap_or(false),
        "structured error missing code: {err}"
    );

    client.call(5, "stop_server", json!({}));
    client.shutdown();
}

#[test]
fn single_flight_concurrent_boots_share_one_server() {
    let dir = TempDir::new().expect("tempdir");
    let config = write_config(&dir);

    let mut client = McpClient::spawn();
    client.initialize();

    // Fire two open_admin_ui calls concurrently (same config) WITHOUT awaiting
    // the first. Regardless of which task acquires the state lock first, the
    // single-flight machine guarantees exactly one server boots: one call boots
    // it, the other observes `Starting` and waits for the same server. Both must
    // succeed and report the SAME base URL (proving no double-boot).
    client.send_call(2, "open_admin_ui", json!({"config_path": config}));
    client.send_call(3, "open_admin_ui", json!({"config_path": config}));

    let first = client.recv_id(2);
    let second = client.recv_id(3);
    assert!(first.get("result").is_some(), "first boot failed: {first}");
    assert!(
        second.get("result").is_some(),
        "second boot failed: {second}"
    );

    let base_of = |resp: &Value| -> String {
        result_texts(resp)
            .into_iter()
            .find_map(|t| {
                serde_json::from_str::<Value>(&t)
                    .ok()
                    .and_then(|v| v.get("baseUrl").and_then(Value::as_str).map(str::to_string))
            })
            .unwrap_or_default()
    };
    let base_a = base_of(&first);
    let base_b = base_of(&second);
    assert!(!base_a.is_empty(), "missing baseUrl in first: {first}");
    assert_eq!(
        base_a, base_b,
        "concurrent boots produced different servers (double-boot): {base_a} vs {base_b}"
    );

    // After the boot has settled, a normal awaited tool call succeeds.
    let list = client.call(4, "list_sources", json!({}));
    assert!(
        list.get("result").is_some(),
        "list_sources after boot failed: {list}"
    );

    client.call(5, "stop_server", json!({}));
    client.shutdown();
}

#[test]
fn failed_boot_allows_retry_with_good_config() {
    let dir = TempDir::new().expect("tempdir");
    let good = write_config(&dir);

    let mut client = McpClient::spawn();
    client.initialize();

    // First boot fails (bad path) and must leave the slot recoverable.
    let resp = client.call(
        2,
        "open_admin_ui",
        json!({"config_path": "/no/such/config.yaml"}),
    );
    assert!(
        resp.get("error").is_some() || is_tool_error(&resp),
        "bad boot should error: {resp}"
    );

    // A subsequent boot with a good config must succeed.
    let resp = client.call(3, "open_admin_ui", json!({"config_path": good}));
    assert!(
        resp.get("result").is_some(),
        "retry boot with good config failed: {resp}"
    );

    client.call(4, "stop_server", json!({}));
    client.shutdown();
}

#[test]
fn stop_server_after_boot_does_not_hang() {
    let dir = TempDir::new().expect("tempdir");
    let config = write_config(&dir);

    let mut client = McpClient::spawn();
    client.initialize();

    client.call(2, "open_admin_ui", json!({"config_path": config}));
    let resp = client.call(3, "stop_server", json!({}));
    assert!(resp.get("result").is_some(), "stop_server failed: {resp}");

    // After stop, a component tool reports not-started again.
    let resp = client.call(4, "list_sources", json!({}));
    let body = serde_json::to_string(&resp)
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        resp.get("error").is_some() || is_tool_error(&resp),
        "list after stop should error: {resp}"
    );
    assert!(body.contains("not started"), "expected not-started: {resp}");

    client.shutdown();
}

#[test]
fn upsert_tool_routing_and_id_check() {
    // Plugins (mock source/reaction) aren't available in this environment, so we
    // can't assert a fully successful component instantiation. Instead we verify
    // the upsert plumbing that is deterministic without plugins:
    //  - a path/body id mismatch is rejected client-side before hitting the API,
    //  - a well-formed upsert is routed as a PUT and actually reaches the API
    //    (it returns a defined result, never "not started" or a panic).
    let dir = TempDir::new().expect("tempdir");
    let config = write_config(&dir);

    let mut client = McpClient::spawn();
    client.initialize();

    client.call(2, "open_admin_ui", json!({"config_path": config}));

    // Mismatched id between path and body must be rejected before the API call.
    let resp = client.call(
        3,
        "upsert_source",
        json!({
            "id": "s1",
            "definition": {"id": "other", "kind": "mock"}
        }),
    );
    assert!(
        resp.get("error").is_some() || is_tool_error(&resp),
        "id mismatch should error: {resp}"
    );
    let body = serde_json::to_string(&resp)
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        body.contains("does not match") || body.contains("match"),
        "mismatch error should explain the id discrepancy: {resp}"
    );

    // A well-formed upsert reaches the API. Without the mock plugin loaded the
    // API will reject the unknown kind, but the response must be a defined
    // structured result (reached the API), not a "not started" error.
    let resp = client.call(
        4,
        "upsert_reaction",
        json!({
            "id": "r1",
            "definition": {"id": "r1", "kind": "log", "queries": []}
        }),
    );
    assert!(
        resp.get("result").is_some(),
        "upsert_reaction should return a tool result: {resp}"
    );
    let body = serde_json::to_string(&resp)
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        !body.contains("not started"),
        "upsert_reaction should reach the API, not report not-started: {resp}"
    );

    client.call(5, "stop_server", json!({}));
    client.shutdown();
}

#[test]
fn validate_rejects_port_zero_for_normal_config() {
    // Regression: the bind-validation bypass is scoped to MCP mode only. A normal
    // config authored with port 0 must still fail `validate`.
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("port0.yaml");
    std::fs::write(
        &path,
        "apiVersion: drasi.io/v1\nid: port0\nhost: \"0.0.0.0\"\nport: 0\nlogLevel: info\nsources: []\nqueries: []\nreactions: []\n",
    )
    .expect("write config");

    let output = Command::new(binary_path())
        .arg("validate")
        .arg("--config")
        .arg(&path)
        .output()
        .expect("run validate");

    assert!(
        !output.status.success(),
        "validate should reject port 0 in a normal config; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn open_admin_ui_without_config_boots_empty_default() {
    // Regression: launched with no --config and called with no config_path
    // (as Claude Desktop does), open_admin_ui must boot an empty in-memory
    // configuration and render the UI, not fail trying to read a missing file.
    let mut client = McpClient::spawn(); // spawned with just `mcp`, no --config
    client.initialize();

    let resp = client.call(2, "open_admin_ui", json!({}));

    // The embedded resource is the MCP App HTML; the URL lives in the JSON info
    // block now (not the resource text, which is the app HTML).
    let resource = ui_resource(&resp);
    assert_eq!(
        resource.get("mimeType").and_then(Value::as_str),
        Some("text/html;profile=mcp-app"),
        "no-config boot did not render an MCP App resource: {resource}"
    );
    let info = open_ui_info(&resp);
    let ui_url = info["uiUrl"].as_str().expect("uiUrl");
    assert!(
        ui_url.starts_with("http://127.0.0.1:") && ui_url.ends_with("/ui/"),
        "unexpected UI url: {ui_url}"
    );

    // The empty default has no queries; listing should succeed (mutations are
    // allowed in memory even without a config file).
    let resp = client.call(3, "list_queries", json!({}));
    assert!(
        resp.get("result").is_some(),
        "list_queries failed on empty default: {resp}"
    );

    // And a query can be created in the in-memory config.
    let resp = client.call(
        4,
        "create_query",
        json!({"definition": {
            "id": "q1",
            "query": "MATCH (n) RETURN n",
            "queryLanguage": "Cypher",
            "sources": []
        }}),
    );
    let texts = result_texts(&resp);
    assert!(
        texts.iter().any(|t| t.contains("\"success\":true")),
        "create_query on empty default failed: {resp}"
    );

    client.call(5, "stop_server", json!({}));
    client.shutdown();
}

/// Full MCP Apps (SEP-1865 / `io.modelcontextprotocol/ui`) contract: the server
/// advertises the `resources` capability, links `open_admin_ui` to the UI
/// resource via `_meta.ui.resourceUri`, lists the `ui://drasi/admin` resource,
/// and serves it as a `text/html;profile=mcp-app` document via `resources/read`.
#[test]
fn mcp_apps_resource_contract_is_satisfied() {
    let mut client = McpClient::spawn();

    // Initialize advertising the MCP Apps extension, like Claude Desktop does.
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {
                "extensions": {
                    "io.modelcontextprotocol/ui": {
                        "mimeTypes": ["text/html;profile=mcp-app"]
                    }
                }
            },
            "clientInfo": {"name": "test", "version": "0"}
        }
    }));
    let init = client.recv_id(1);
    // The server must advertise the resources capability so the host will call
    // resources/read for the UI resource.
    assert!(
        init.pointer("/result/capabilities/resources").is_some(),
        "server did not advertise resources capability: {init}"
    );
    client.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    // tools/list: open_admin_ui must declare _meta.ui.resourceUri.
    client.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}));
    let tl = client.recv_id(2);
    let tools = tl
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .expect("tools array");
    let open_ui = tools
        .iter()
        .find(|t| t.get("name").and_then(Value::as_str) == Some("open_admin_ui"))
        .expect("open_admin_ui tool");
    assert_eq!(
        open_ui
            .pointer("/_meta/ui/resourceUri")
            .and_then(Value::as_str),
        Some("ui://drasi/admin"),
        "open_admin_ui missing _meta.ui.resourceUri: {open_ui}"
    );

    // Boot the server so the resource can reference the live port.
    let resp = client.call(3, "open_admin_ui", json!({}));
    assert!(resp.get("result").is_some(), "open_admin_ui failed: {resp}");

    // resources/list must include the UI resource.
    client.send(&json!({"jsonrpc": "2.0", "id": 4, "method": "resources/list", "params": {}}));
    let rl = client.recv_id(4);
    let resources = rl
        .pointer("/result/resources")
        .and_then(Value::as_array)
        .expect("resources array");
    assert!(
        resources
            .iter()
            .any(|r| r.get("uri").and_then(Value::as_str) == Some("ui://drasi/admin")),
        "resources/list missing ui://drasi/admin: {rl}"
    );

    // resources/read must return the MCP App HTML.
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "resources/read",
        "params": {"uri": "ui://drasi/admin"}
    }));
    let rr = client.recv_id(5);
    let contents = rr
        .pointer("/result/contents/0")
        .expect("read resource contents");
    assert_eq!(
        contents.get("mimeType").and_then(Value::as_str),
        Some("text/html;profile=mcp-app"),
        "resources/read returned wrong mimeType: {contents}"
    );
    let html = contents
        .get("text")
        .and_then(Value::as_str)
        .expect("html text");
    assert!(html.contains("<iframe"), "read HTML missing iframe: {html}");
    assert!(
        contents
            .pointer("/_meta/ui/csp/frameDomains")
            .and_then(Value::as_array)
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "read resource missing _meta.ui.csp.frameDomains: {contents}"
    );

    // An unknown resource URI is rejected, not panicked.
    client.send(&json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "resources/read",
        "params": {"uri": "ui://drasi/does-not-exist"}
    }));
    let bad = client.recv_id(6);
    assert!(
        bad.get("error").is_some(),
        "unknown resource read should error: {bad}"
    );

    client.call(7, "stop_server", json!({}));
    client.shutdown();
}
