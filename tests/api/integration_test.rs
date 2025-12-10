//! API Integration Tests
//!
//! These tests validate the complete data flow from API requests to DrasiLib operations.
//! They test the full lifecycle of components through the API, including dynamic creation
//! of sources and reactions via the tagged enum config format.

#![allow(clippy::unwrap_used)]

use crate::test_utils::{create_mock_reaction, create_mock_source};
use axum::{
    body::{to_bytes, Body},
    extract::Extension,
    http::{Request, StatusCode},
    Router,
};
use drasi_lib::Query;
use drasi_server::api;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

/// Helper to create a test router with all dependencies
async fn create_test_router() -> (Router, Arc<drasi_lib::DrasiLib>) {
    use drasi_lib::DrasiLib;

    // Create mock source instances
    let test_source = create_mock_source("test-source");
    let query_source = create_mock_source("query-source");
    let auto_source = create_mock_source("auto-source");

    // Create mock reaction instances
    let test_reaction = create_mock_reaction("test-reaction", vec!["reaction-query".to_string()]);
    let auto_reaction = create_mock_reaction("auto-reaction", vec!["auto-query".to_string()]);

    // Create a minimal DrasiLib using the builder with mock instances
    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source(test_source)
        .with_source(query_source)
        .with_source(auto_source)
        .with_reaction(test_reaction)
        .with_reaction(auto_reaction)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start the core
    core.start().await.expect("Failed to start core");

    let read_only = Arc::new(false);
    let config_persistence: Option<Arc<drasi_server::persistence::ConfigPersistence>> = None;

    let router = Router::new()
        // Health endpoint
        .route("/health", axum::routing::get(api::handlers::health_check))
        // Source endpoints
        .route("/sources", axum::routing::get(api::handlers::list_sources))
        .route(
            "/sources",
            axum::routing::post(api::handlers::create_source_handler),
        )
        .route(
            "/sources/:id",
            axum::routing::get(api::handlers::get_source),
        )
        .route(
            "/sources/:id",
            axum::routing::delete(api::handlers::delete_source),
        )
        .route(
            "/sources/:id/start",
            axum::routing::post(api::handlers::start_source),
        )
        .route(
            "/sources/:id/stop",
            axum::routing::post(api::handlers::stop_source),
        )
        // Query endpoints
        .route("/queries", axum::routing::get(api::handlers::list_queries))
        .route("/queries", axum::routing::post(api::handlers::create_query))
        .route("/queries/:id", axum::routing::get(api::handlers::get_query))
        .route(
            "/queries/:id",
            axum::routing::delete(api::handlers::delete_query),
        )
        .route(
            "/queries/:id/start",
            axum::routing::post(api::handlers::start_query),
        )
        .route(
            "/queries/:id/stop",
            axum::routing::post(api::handlers::stop_query),
        )
        .route(
            "/queries/:id/results",
            axum::routing::get(api::handlers::get_query_results),
        )
        // Reaction endpoints
        .route(
            "/reactions",
            axum::routing::get(api::handlers::list_reactions),
        )
        .route(
            "/reactions",
            axum::routing::post(api::handlers::create_reaction_handler),
        )
        .route(
            "/reactions/:id",
            axum::routing::get(api::handlers::get_reaction),
        )
        .route(
            "/reactions/:id",
            axum::routing::delete(api::handlers::delete_reaction),
        )
        .route(
            "/reactions/:id/start",
            axum::routing::post(api::handlers::start_reaction),
        )
        .route(
            "/reactions/:id/stop",
            axum::routing::post(api::handlers::stop_reaction),
        )
        // Add extensions using new architecture
        .layer(Extension(core.clone()))
        .layer(Extension(read_only))
        .layer(Extension(config_persistence));

    (router, core)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (router, _) = create_test_router().await;

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
async fn test_source_lifecycle_via_api() {
    let (router, _) = create_test_router().await;

    // List sources (pre-registered via builder)
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sources")
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

    // Get specific source
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sources/test-source")
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

    // Source is already running (auto-started on first startup)
    // Stop the source first to test lifecycle operations
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sources/test-source/stop")
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
                .uri("/sources/test-source/start")
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
                .uri("/sources/test-source/stop")
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
                .uri("/sources/test-source")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_query_lifecycle_via_api() {
    let (router, core) = create_test_router().await;

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
                .uri("/queries")
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
                .uri("/queries/test-query")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_reaction_lifecycle_via_api() {
    let (router, _core) = create_test_router().await;

    // Reactions are pre-registered via builder, test listing them
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/reactions")
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

    // Get specific reaction
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/reactions/test-reaction")
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
}

#[tokio::test]
async fn test_dynamic_source_creation_via_api() {
    let (router, _) = create_test_router().await;

    // Create a mock source via API using the tagged enum format
    let source_config = json!({
        "kind": "mock",
        "id": "dynamic-source",
        "auto_start": false
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sources")
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
    let (router, _) = create_test_router().await;

    // Create a log reaction via API using the tagged enum format
    let reaction_config = json!({
        "kind": "log",
        "id": "dynamic-reaction",
        "queries": ["some-query"],
        "auto_start": false
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/reactions")
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
    let (router, _) = create_test_router().await;

    // Try to get non-existent source
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sources/non-existent")
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
                .uri("/sources/non-existent/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_query_results_endpoint() {
    let (router, core) = create_test_router().await;

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
                .uri("/queries/results-query/results")
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
                .uri("/queries/non-existent/results")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
