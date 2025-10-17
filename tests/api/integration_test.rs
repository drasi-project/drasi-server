//! API Integration Tests
//!
//! These tests validate the complete data flow from API requests to DrasiServerCore operations.
//! They test the full lifecycle of components through the API.

use axum::{
    body::{to_bytes, Body},
    extract::Extension,
    http::{Request, StatusCode},
    Router,
};
use drasi_server::api;
use drasi_server_core::{config::QueryLanguage, QueryConfig, SourceConfig};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

/// Helper to create a test router with all dependencies
async fn create_test_router() -> (Router, Arc<drasi_server_core::DrasiServerCore>) {
    use drasi_server_core::DrasiServerCore;

    // Create a minimal DrasiServerCore using the builder
    let core = DrasiServerCore::builder()
        .with_id("test-server")
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
            axum::routing::post(api::handlers::create_source),
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
            axum::routing::post(api::handlers::create_reaction),
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

    // Create a source
    let source_config = json!({
        "id": "test-source",
        "source_type": "mock",
        "auto_start": false,
        "properties": {
            "interval_ms": 1000,
            "data_type": "counter"
        }
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

    // List sources
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
    assert_eq!(json["data"][0]["id"], "test-source");

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

    // Stop the source - should succeed
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

    // First create a source for the query
    let source_config = SourceConfig {
        id: "query-source".to_string(),
        source_type: "mock".to_string(),
        auto_start: false,
        properties: HashMap::new(),
        bootstrap_provider: None,
    };
    core.create_source(source_config.clone()).await.unwrap();

    // Create a query
    let query_config = json!({
        "id": "test-query",
        "query": "MATCH (n:Node) RETURN n",
        "sources": ["query-source"],
        "auto_start": false
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queries")
                .header("content-type", "application/json")
                .body(Body::from(query_config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);

    // Delete the query
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
    let (router, core) = create_test_router().await;

    // First create a query for the reaction
    let query_config = QueryConfig {
        id: "reaction-query".to_string(),
        query: "MATCH (n) RETURN n".to_string(),
        sources: vec!["source1".to_string()],
        auto_start: false,
        properties: HashMap::new(),
        query_language: QueryLanguage::default(),
        joins: None,
    };
    core.create_query(query_config.clone()).await.unwrap();

    // Create a reaction
    let reaction_config = json!({
        "id": "test-reaction",
        "reaction_type": "log",
        "queries": ["reaction-query"],
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

    // List reactions
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
    assert_eq!(json["data"][0]["id"], "test-reaction");

    // Delete the reaction
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/reactions/test-reaction")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auto_start_behavior() {
    let (router, _) = create_test_router().await;

    // Create source with auto_start=true
    let source_config = json!({
        "id": "auto-source",
        "source_type": "mock",
        "auto_start": true,
        "properties": {
            "interval_ms": 1000
        }
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

    // Wait for auto-start
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Create query with auto_start=true
    let query_config = json!({
        "id": "auto-query",
        "query": "MATCH (n) RETURN n",
        "sources": ["auto-source"],
        "auto_start": true
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/queries")
                .header("content-type", "application/json")
                .body(Body::from(query_config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Wait for auto-start
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Create reaction with auto_start=true
    let reaction_config = json!({
        "id": "auto-reaction",
        "reaction_type": "log",
        "queries": ["auto-query"],
        "auto_start": true
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

    // Wait for auto-start
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
}

#[tokio::test]
async fn test_idempotent_create_operations() {
    let (router, _) = create_test_router().await;

    let source_config = json!({
        "id": "idempotent-source",
        "source_type": "mock",
        "auto_start": false,
        "properties": {}
    });

    // First create
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

    // Second create (should be idempotent)
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
        .contains("already exists"));
}

#[tokio::test]
async fn test_read_only_mode() {
    use drasi_server_core::DrasiServerCore;

    // Create a minimal DrasiServerCore
    let core = DrasiServerCore::builder()
        .with_id("readonly-test-server")
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);
    core.start().await.expect("Failed to start core");

    let read_only = Arc::new(true); // Enable read-only mode
    let config_persistence: Option<Arc<drasi_server::persistence::ConfigPersistence>> = None;

    let router = Router::new()
        .route(
            "/sources",
            axum::routing::post(api::handlers::create_source),
        )
        .route(
            "/sources/:id",
            axum::routing::delete(api::handlers::delete_source),
        )
        .layer(Extension(core))
        .layer(Extension(read_only))
        .layer(Extension(config_persistence));

    // Try to create a source in read-only mode
    let source_config = json!({
        "id": "readonly-test",
        "source_type": "mock",
        "auto_start": false,
        "properties": {}
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
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("read-only mode"));
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

    // Create a query
    let query_config = QueryConfig {
        id: "results-query".to_string(),
        query: "MATCH (n) RETURN n".to_string(),
        sources: vec!["source1".to_string()],
        auto_start: false,
        properties: HashMap::new(),
        query_language: QueryLanguage::default(),
        joins: None,
    };
    core.create_query(query_config.clone()).await.unwrap();

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
