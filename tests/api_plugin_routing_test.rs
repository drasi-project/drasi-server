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

//! Plugin sub-router extension wiring regression tests.
//!
//! These tests exist to catch the class of bug where `/api/v1/plugins/*`
//! routes are nested under the root app router separately from the rest of
//! the v1 router and therefore do not inherit Axum `Extension` layers added
//! to the v1 router. Without these tests, handlers like `install_plugin`
//! and `load_plugin` (which extract `Extension<Arc<bool>>` for read-only
//! enforcement) would fail at request time with:
//!
//!     Missing request extension: Extension of type `alloc::sync::Arc<bool>`
//!     was not found.
//!
//! Any future handler added to `plugin_routes()` that extracts a new
//! `Extension<T>` MUST also have `T` added in `build_plugin_router` —
//! these tests will fail until that happens.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use drasi_host_sdk::lifecycle::PluginLifecycleManager;
use drasi_host_sdk::plugin_registry::PluginRegistry;
use drasi_server::api::v1::build_plugin_router;
use drasi_server::instance_registry::InstanceRegistry;
use drasi_server::plugin_orchestrator::PluginOrchestrator;
use tokio::sync::RwLock;
use tower::ServiceExt;

/// Build a router that mirrors the production wiring in `server.rs`:
/// the plugin sub-router is nested separately under `/api/v1/plugins`,
/// reproducing the conditions that caused the missing-extension bug.
fn build_test_app(read_only: bool) -> Router {
    let registry = Arc::new(RwLock::new(PluginRegistry::new()));
    let lifecycle = Arc::new(PluginLifecycleManager::new(registry));
    let orchestrator = Arc::new(PluginOrchestrator::new(lifecycle));
    let instances = InstanceRegistry::new();

    let plugin_router = build_plugin_router(orchestrator, instances, Arc::new(read_only));

    Router::new().nest("/api/v1/plugins", plugin_router)
}

/// Regression test: `POST /api/v1/plugins/install` must not fail with the
/// "Missing request extension" error introduced when the plugin sub-router
/// was nested separately from the v1 router without re-adding the
/// `Arc<bool>` (read-only) extension. When the server is in read-only mode
/// the handler must return a structured `CONFIG_READ_ONLY` error.
#[tokio::test]
async fn install_plugin_returns_read_only_error_when_read_only() {
    let app = build_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plugins/install")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ref":"reaction/sse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        !body_str.contains("Missing request extension"),
        "install endpoint is missing a required Extension layer: {body_str}"
    );

    // CONFIG_READ_ONLY maps to HTTP 409 CONFLICT per the error code table.
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "expected 409 CONFLICT (CONFIG_READ_ONLY), got {status} body={body_str}"
    );

    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "CONFIG_READ_ONLY", "body={body_str}");
}

/// Regression test: `POST /api/v1/plugins/install` must not fail with the
/// "Missing request extension" error when read-only mode is disabled. The
/// handler is expected to reach the orchestrator and fail with
/// `PLUGIN_INSTALL_FAILED` (because the test orchestrator has no
/// `PluginOperations` configured) — proving the extension is plumbed AND the
/// handler actually executed.
#[tokio::test]
async fn install_plugin_reaches_handler_when_writable() {
    let app = build_test_app(false);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plugins/install")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ref":"reaction/sse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        !body_str.contains("Missing request extension"),
        "install endpoint is missing a required Extension layer: {body_str}"
    );

    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "expected 502 BAD_GATEWAY from orchestrator without PluginOperations, got {status} body={body_str}"
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "PLUGIN_INSTALL_FAILED", "body={body_str}");
}

/// Regression test: `POST /api/v1/plugins/load` is the second handler that
/// extracts `Extension<Arc<bool>>` and would have hit the same missing-
/// extension bug. Verify it also reaches the handler successfully.
#[tokio::test]
async fn load_plugin_returns_read_only_error_when_read_only() {
    let app = build_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plugins/load")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"filename":"libdrasi_reaction_sse.so"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        !body_str.contains("Missing request extension"),
        "load endpoint is missing a required Extension layer: {body_str}"
    );

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "expected 409 CONFLICT (CONFIG_READ_ONLY), got {status} body={body_str}"
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "CONFIG_READ_ONLY", "body={body_str}");
}

/// `GET /api/v1/plugins` must remain reachable through the plugin sub-router
/// (this handler does not need `read_only`, but it relies on the orchestrator
/// extension being present).
#[tokio::test]
async fn list_plugins_reaches_handler() {
    let app = build_test_app(false);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/plugins")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    assert!(
        !body_str.contains("Missing request extension"),
        "list endpoint is missing a required Extension layer: {body_str}"
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 OK, got {status} body={body_str}"
    );
}
