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

//! API Integration Tests
//!
//! These tests validate the complete data flow from API requests to DrasiLib operations.
//! They test the full lifecycle of components through the API, including dynamic creation
//! of sources and reactions via the tagged enum config format.

#![allow(clippy::unwrap_used)]

mod test_support;

use test_support::{create_mock_reaction, create_mock_source};

use axum::{
    body::{to_bytes, Body},
    extract::Extension,
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::Query;
use drasi_server::api;
use drasi_server::api::shared::handlers;
use futures_util::StreamExt;
use std::time::Duration;
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, timeout};
use tower::ServiceExt;

/// Helper to create a test router with all dependencies
async fn create_test_router() -> (Router, Arc<drasi_lib::DrasiLib>, TestComponentRegistry) {
    use drasi_lib::DrasiLib;

    // Create mock source instances
    let test_source = create_mock_source("test-source");
    let query_source = create_mock_source("query-source");
    let auto_source = create_mock_source("auto-source");

    // Create mock reaction instances
    let test_reaction = create_mock_reaction(
        "test-reaction",
        vec!["reaction-query".to_string()],
    );
    let auto_reaction = create_mock_reaction("auto-reaction", vec!["auto-query".to_string()]);

    // Create a minimal DrasiLib using the builder with mock instances
    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(test_source.clone())
        .with_source(query_source.clone())
        .with_source(auto_source.clone())
        .with_reaction(test_reaction.clone())
        .with_reaction(auto_reaction.clone())
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start the core
    core.start().await.expect("Failed to start core");

    let read_only = Arc::new(false);
    let config_persistence: Option<Arc<drasi_server::persistence::ConfigPersistence>> = None;

    let instance_id = "test-server";

    let instance_router = Router::new()
        // Source endpoints
        .route("/sources", axum::routing::get(handlers::list_sources))
        .route(
            "/sources",
            axum::routing::post(handlers::create_source_handler),
        )
        .route("/sources/:id", axum::routing::get(handlers::get_source))
        .route("/sources/:id/logs", axum::routing::get(handlers::get_source_logs))
        .route(
            "/sources/:id/logs/stream",
            axum::routing::get(handlers::stream_source_logs),
        )
        .route(
            "/sources/:id",
            axum::routing::delete(handlers::delete_source),
        )
        .route(
            "/sources/:id/start",
            axum::routing::post(handlers::start_source),
        )
        .route(
            "/sources/:id/stop",
            axum::routing::post(handlers::stop_source),
        )
        // Query endpoints
        .route("/queries", axum::routing::get(handlers::list_queries))
        .route("/queries", axum::routing::post(handlers::create_query))
        .route("/queries/:id", axum::routing::get(handlers::get_query))
        .route("/queries/:id/logs", axum::routing::get(handlers::get_query_logs))
        .route(
            "/queries/:id/logs/stream",
            axum::routing::get(handlers::stream_query_logs),
        )
        .route(
            "/queries/:id",
            axum::routing::delete(handlers::delete_query),
        )
        .route(
            "/queries/:id/start",
            axum::routing::post(handlers::start_query),
        )
        .route(
            "/queries/:id/stop",
            axum::routing::post(handlers::stop_query),
        )
        .route(
            "/queries/:id/results",
            axum::routing::get(handlers::get_query_results),
        )
        .route(
            "/queries/:id/attach",
            axum::routing::get(handlers::attach_query_stream),
        )
        // Reaction endpoints
        .route("/reactions", axum::routing::get(handlers::list_reactions))
        .route(
            "/reactions",
            axum::routing::post(handlers::create_reaction_handler),
        )
        .route("/reactions/:id", axum::routing::get(handlers::get_reaction))
        .route("/reactions/:id/logs", axum::routing::get(handlers::get_reaction_logs))
        .route(
            "/reactions/:id/logs/stream",
            axum::routing::get(handlers::stream_reaction_logs),
        )
        .route(
            "/reactions/:id",
            axum::routing::delete(handlers::delete_reaction),
        )
        .route(
            "/reactions/:id/start",
            axum::routing::post(handlers::start_reaction),
        )
        .route(
            "/reactions/:id/stop",
            axum::routing::post(handlers::stop_reaction),
        )
        // Add extensions using new architecture
        .layer(Extension(core.clone()))
        .layer(Extension(read_only))
        .layer(Extension(config_persistence))
        .layer(Extension(instance_id.to_string()));

    // Create instances map for list_instances endpoint
    let mut instances_map = indexmap::IndexMap::new();
    instances_map.insert(instance_id.to_string(), core.clone());
    let instances = Arc::new(instances_map);

    let router = Router::new()
        // Health endpoint
        .route("/health", axum::routing::get(handlers::health_check))
        .route("/instances", axum::routing::get(handlers::list_instances))
        .nest(&format!("/instances/{instance_id}"), instance_router)
        .layer(Extension(instances));

    let registry = TestComponentRegistry {
        source: test_source,
        reaction: test_reaction,
    };

    (router, core, registry)
}

struct TestComponentRegistry {
    source: test_support::mock_components::MockSource,
    reaction: test_support::mock_components::MockReaction,
}

#[tokio::test]
async fn test_health_endpoint() {
    let (router, _, _registry) = create_test_router().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json["timestamp"].is_string());
}

#[tokio::test]
async fn test_instances_endpoint() {
    let (router, _, _registry) = create_test_router().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/instances")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    assert!(json["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"] == "test-server"));
}

#[tokio::test]
async fn test_source_lifecycle_via_api() {
    let (router, _, _registry) = create_test_router().await;
    let base = format!("/instances/{}", "test-server");

    // List sources (pre-registered via builder)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/sources"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"].is_array());
    // Should have pre-registered sources
    assert!(!json["data"].as_array().unwrap().is_empty());
    assert!(json["data"][0]["links"]["self"].is_string());
    assert!(json["data"][0]["links"]["full"].is_string());
    assert!(json["data"][0]["links"]["self"].is_string());
    assert!(json["data"][0]["links"]["full"].is_string());

    // Get specific source
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/sources/test-source"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["id"], "test-source");
    assert!(json["data"]["links"]["self"].is_string());
    assert!(json["data"]["links"]["full"].is_string());

    // Source is already running (auto-started on first startup)
    // Stop the source first to test lifecycle operations
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/sources/test-source/stop"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);

    // Start the source - should succeed (mock sources support lifecycle operations)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/sources/test-source/start"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);

    // Stop the source - should succeed again
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/sources/test-source/stop"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);

    // Delete the source
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("{base}/sources/test-source"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_query_lifecycle_via_api() {
    let (router, core, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Create a query using DrasiLib (not via API - queries can still be created dynamically)
    let query_config = Query::cypher("test-query")
        .query("MATCH (n:Node) RETURN n")
        .from_source("query-source")
        .auto_start(false)
        .build();
    core.add_query(query_config.clone()).await.unwrap();

    // List queries via API
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/queries"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"].is_array());

    // Delete the query via API
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("{base}/queries/test-query"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_reaction_lifecycle_via_api() {
    let (router, _core, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Reactions are pre-registered via builder, test listing them
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/reactions"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].is_array());
    // Should have pre-registered reactions
    assert!(!json["data"].as_array().unwrap().is_empty());
    assert!(json["data"][0]["links"]["self"].is_string());
    assert!(json["data"][0]["links"]["full"].is_string());

    // Get specific reaction
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/reactions/test-reaction"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["id"], "test-reaction");
    assert!(json["data"]["links"]["self"].is_string());
    assert!(json["data"]["links"]["full"].is_string());
}

#[tokio::test]
async fn test_source_logs_snapshot_via_api() {
    let (router, _core, registry) = create_test_router().await;
    registry.source.emit_log("source log entry").await;
    sleep(Duration::from_millis(50)).await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/instances/test-server/sources/test-source/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["message"] == "source log entry"));
}

#[tokio::test]
async fn test_reaction_logs_snapshot_via_api() {
    let (router, _core, registry) = create_test_router().await;
    registry.reaction.emit_log("reaction log entry").await;
    sleep(Duration::from_millis(50)).await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/instances/test-server/reactions/test-reaction/logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["message"] == "reaction log entry"));
}

#[tokio::test]
async fn test_source_logs_stream_via_api() {
    let (router, _core, registry) = create_test_router().await;
    let mut response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/instances/test-server/sources/test-source/logs/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    registry.source.emit_log("streamed source log").await;

    let body = response.into_body();
    let mut stream = body.into_data_stream();
    let payload = timeout(Duration::from_secs(2), async move {
        let mut collected = String::new();
        while let Some(Ok(chunk)) = stream.next().await {
            collected.push_str(&String::from_utf8_lossy(&chunk));
            if collected.contains("streamed source log") {
                break;
            }
        }
        collected
    })
    .await
    .expect("Timed out waiting for log stream");

    assert!(payload.contains("streamed source log"));
}

#[tokio::test]
async fn test_dynamic_source_creation_via_api() {
    let (router, _, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Create a mock source via API using the tagged enum format
    let source_config = json!({
        "kind": "mock",
        "id": "dynamic-source",
        "autoStart": false
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/sources"))
                .header("content-type", "application/json")
                .body(Body::from(source_config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]["message"]
        .as_str()
        .unwrap()
        .contains("created successfully"));
}

#[tokio::test]
async fn test_dynamic_reaction_creation_via_api() {
    let (router, _, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Create a log reaction via API using the tagged enum format
    // Use empty queries list since autoStart is false - queries can be added later
    let reaction_config = json!({
        "kind": "log",
        "id": "dynamic-reaction",
        "queries": [],
        "autoStart": false
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/reactions"))
                .header("content-type", "application/json")
                .body(Body::from(reaction_config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]["message"]
        .as_str()
        .unwrap()
        .contains("created successfully"));
}

#[tokio::test]
async fn test_error_handling() {
    let (router, _, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Try to get non-existent source
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/sources/non-existent"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Try to start non-existent source
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/sources/non-existent/start"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_query_results_endpoint() {
    let (router, core, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Add a query
    let query_config = Query::cypher("results-query")
        .query("MATCH (n) RETURN n")
        .from_source("query-source")
        .auto_start(false)
        .build();
    core.add_query(query_config.clone()).await.unwrap();

    // Try to get results - should return error (not exposed in public API)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/queries/results-query/results"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    // The error should contain some information about why results can't be fetched
    assert!(json["error"].is_string());

    // Try to get results for non-existent query - should return 404
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/queries/non-existent/results"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_query_attach_sse_stream() {
    let (router, core, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Add a query to attach to
    let query_config = Query::cypher("attach-query")
        .query("MATCH (n) RETURN n")
        .from_source("query-source")
        .auto_start(false)
        .build();
    core.add_query(query_config.clone()).await.unwrap();

    // Start an attach stream request
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/queries/attach-query/attach"))
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should succeed
    assert_eq!(response.status(), StatusCode::OK);

    // Content-Type should be text/event-stream
    let content_type = response.headers().get("content-type").unwrap();
    assert!(content_type.to_str().unwrap().contains("text/event-stream"));
}

#[tokio::test]
async fn test_query_attach_not_found() {
    let (router, _core, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Try to attach to non-existent query
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/queries/non-existent/attach"))
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_query_attach_creates_temporary_reaction() {
    let (router, core, _registry) = create_test_router().await;
    let base = "/instances/test-server";

    // Add a query to attach to
    let query_config = Query::cypher("attach-reaction-test")
        .query("MATCH (n) RETURN n")
        .from_source("query-source")
        .auto_start(false)
        .build();
    core.add_query(query_config.clone()).await.unwrap();

    // Count reactions before attach
    let reactions_before = core.list_reactions().await.unwrap_or_default().len();

    // Start an attach stream request
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/queries/attach-reaction-test/attach"))
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // A temporary reaction should have been created
    let reactions_after = core.list_reactions().await.unwrap_or_default();
    assert!(reactions_after.len() > reactions_before);

    // Verify the temporary reaction exists with __attach_ prefix
    let attach_reaction = reactions_after
        .iter()
        .find(|(id, _)| id.starts_with("__attach_attach-reaction-test_"));
    assert!(attach_reaction.is_some(), "Expected temporary attach reaction to be created");
}
