//! API Integration Tests
//! 
//! These tests validate the complete data flow from API requests to DrasiServerCore operations.
//! They test the full lifecycle of components through the API.

use axum::{
    body::{Body, to_bytes},
    extract::Extension,
    http::{Request, StatusCode},
    Router,
};
use drasi_server::api;
use drasi_server_core::{
    ComponentStatus, QueryConfig, RuntimeConfig, SourceConfig,
    DrasiServerCoreConfig as ServerConfig,
    config::{DrasiServerCoreSettings as ServerSettings, QueryLanguage},
    QueryManager, ReactionManager, SourceManager,
    channels::EventChannels,
    routers::{BootstrapRouter, DataRouter, SubscriptionRouter},
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

/// Helper to create a test router with all dependencies
async fn create_test_router() -> (
    Router,
    Arc<SourceManager>,
    Arc<QueryManager>,
    Arc<ReactionManager>,
) {
    let server_settings = ServerSettings::default();
    let server_config = ServerConfig {
        server: server_settings,
        sources: vec![],
        queries: vec![],
        reactions: vec![],
    };
    let _runtime_config = Arc::new(RuntimeConfig::from(server_config));
    let (channels, _receivers) = EventChannels::new();
    
    let source_manager = Arc::new(SourceManager::new(
        channels.source_change_tx.clone(),
        channels.component_event_tx.clone(),
    ));
    let query_manager = Arc::new(QueryManager::new(
        channels.query_result_tx.clone(),
        channels.component_event_tx.clone(),
        channels.bootstrap_request_tx.clone(),
    ));
    let reaction_manager = Arc::new(ReactionManager::new(
        channels.component_event_tx.clone(),
    ));
    
    let data_router = Arc::new(DataRouter::new());
    let subscription_router = Arc::new(SubscriptionRouter::new());
    let bootstrap_router = Arc::new(BootstrapRouter::new());
    let read_only = Arc::new(false);

    let router = Router::new()
        // Health endpoint
        .route("/health", axum::routing::get(api::handlers::health_check))
        // Source endpoints
        .route("/sources", axum::routing::get(api::handlers::list_sources))
        .route("/sources", axum::routing::post(api::handlers::create_source))
        .route("/sources/:id", axum::routing::get(api::handlers::get_source))
        .route("/sources/:id", axum::routing::put(api::handlers::update_source))
        .route("/sources/:id", axum::routing::delete(api::handlers::delete_source))
        .route("/sources/:id/start", axum::routing::post(api::handlers::start_source))
        .route("/sources/:id/stop", axum::routing::post(api::handlers::stop_source))
        // Query endpoints  
        .route("/queries", axum::routing::get(api::handlers::list_queries))
        .route("/queries", axum::routing::post(api::handlers::create_query))
        .route("/queries/:id", axum::routing::get(api::handlers::get_query))
        .route("/queries/:id", axum::routing::put(api::handlers::update_query))
        .route("/queries/:id", axum::routing::delete(api::handlers::delete_query))
        .route("/queries/:id/start", axum::routing::post(api::handlers::start_query))
        .route("/queries/:id/stop", axum::routing::post(api::handlers::stop_query))
        .route("/queries/:id/results", axum::routing::get(api::handlers::get_query_results))
        // Reaction endpoints
        .route("/reactions", axum::routing::get(api::handlers::list_reactions))
        .route("/reactions", axum::routing::post(api::handlers::create_reaction))
        .route("/reactions/:id", axum::routing::get(api::handlers::get_reaction))
        .route("/reactions/:id", axum::routing::put(api::handlers::update_reaction))
        .route("/reactions/:id", axum::routing::delete(api::handlers::delete_reaction))
        .route("/reactions/:id/start", axum::routing::post(api::handlers::start_reaction))
        .route("/reactions/:id/stop", axum::routing::post(api::handlers::stop_reaction))
        // Add extensions
        .layer(Extension(source_manager.clone()))
        .layer(Extension(query_manager.clone()))
        .layer(Extension(reaction_manager.clone()))
        .layer(Extension(data_router))
        .layer(Extension(subscription_router))
        .layer(Extension(bootstrap_router))
        .layer(Extension(read_only));

    (router, source_manager, query_manager, reaction_manager)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (router, _, _, _) = create_test_router().await;

    let response = router
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
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
    let (router, source_manager, _, _) = create_test_router().await;

    // Create a source
    let source_config = json!({
        "id": "test-source",
        "source_type": "internal.mock",
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

    // Verify source was created in manager
    let source = source_manager.get_source("test-source".to_string()).await.unwrap();
    assert_eq!(source.id, "test-source");
    assert_eq!(source.source_type, "internal.mock");
    assert_eq!(source.status, ComponentStatus::Stopped);

    // List sources
    let response = router
        .clone()
        .oneshot(Request::builder().uri("/sources").body(Body::empty()).unwrap())
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

    // Start the source
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
    
    // Wait a bit for async start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify source is running
    let source = source_manager.get_source("test-source".to_string()).await.unwrap();
    assert!(matches!(source.status, ComponentStatus::Running | ComponentStatus::Starting));

    // Stop the source
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
    
    // Wait for stop
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify source is stopped
    let source = source_manager.get_source("test-source".to_string()).await.unwrap();
    assert!(matches!(source.status, ComponentStatus::Stopped | ComponentStatus::Stopping));

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
    
    // Verify source was deleted
    let result = source_manager.get_source("test-source".to_string()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_query_lifecycle_via_api() {
    let (router, source_manager, query_manager, _) = create_test_router().await;

    // First create a source for the query
    let source_config = SourceConfig {
        id: "query-source".to_string(),
        source_type: "internal.mock".to_string(),
        auto_start: false,
        properties: HashMap::new(),
        bootstrap_provider: None,
    };
    source_manager.add_source(source_config).await.unwrap();

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

    // Verify query was created
    let query = query_manager.get_query("test-query".to_string()).await.unwrap();
    assert_eq!(query.id, "test-query");
    assert_eq!(query.query, "MATCH (n:Node) RETURN n");
    assert_eq!(query.status, ComponentStatus::Stopped);

    // Update the query
    let updated_config = json!({
        "id": "test-query",
        "query": "MATCH (n:Node) WHERE n.active = true RETURN n",
        "sources": ["query-source"],
        "auto_start": false
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/queries/test-query")
                .header("content-type", "application/json")
                .body(Body::from(updated_config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify query was updated
    let query = query_manager.get_query("test-query".to_string()).await.unwrap();
    assert_eq!(query.query, "MATCH (n:Node) WHERE n.active = true RETURN n");

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
    
    // Verify query was deleted
    let result = query_manager.get_query("test-query".to_string()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_reaction_lifecycle_via_api() {
    let (router, _, query_manager, reaction_manager) = create_test_router().await;

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
    query_manager.add_query(query_config).await.unwrap();

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

    // Verify reaction was created
    let reaction = reaction_manager.get_reaction("test-reaction".to_string()).await.unwrap();
    assert_eq!(reaction.id, "test-reaction");
    assert_eq!(reaction.reaction_type, "log");
    assert_eq!(reaction.status, ComponentStatus::Stopped);

    // List reactions
    let response = router
        .clone()
        .oneshot(Request::builder().uri("/reactions").body(Body::empty()).unwrap())
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
    
    // Verify reaction was deleted
    let result = reaction_manager.get_reaction("test-reaction".to_string()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auto_start_behavior() {
    let (router, source_manager, query_manager, reaction_manager) = create_test_router().await;

    // Create source with auto_start=true
    let source_config = json!({
        "id": "auto-source",
        "source_type": "internal.mock",
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
    
    // Verify source auto-started
    let source = source_manager.get_source("auto-source".to_string()).await.unwrap();
    assert!(matches!(source.status, ComponentStatus::Running | ComponentStatus::Starting));

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
    
    // Verify query auto-started
    let query = query_manager.get_query("auto-query".to_string()).await.unwrap();
    assert!(matches!(query.status, ComponentStatus::Running | ComponentStatus::Starting));

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
    
    // Verify reaction auto-started
    let reaction = reaction_manager.get_reaction("auto-reaction".to_string()).await.unwrap();
    assert!(matches!(reaction.status, ComponentStatus::Running | ComponentStatus::Starting));
}

#[tokio::test]
async fn test_idempotent_create_operations() {
    let (router, _, _, _) = create_test_router().await;

    let source_config = json!({
        "id": "idempotent-source",
        "source_type": "internal.mock",
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
    assert!(json["data"]["message"].as_str().unwrap().contains("already exists"));
}

#[tokio::test]
async fn test_read_only_mode() {
    let server_settings = ServerSettings::default();
    let server_config = ServerConfig {
        server: server_settings,
        sources: vec![],
        queries: vec![],
        reactions: vec![],
    };
    let _runtime_config = Arc::new(RuntimeConfig::from(server_config));
    let (channels, _receivers) = EventChannels::new();
    
    let source_manager = Arc::new(SourceManager::new(
        channels.source_change_tx.clone(),
        channels.component_event_tx.clone(),
    ));
    let query_manager = Arc::new(QueryManager::new(
        channels.query_result_tx.clone(),
        channels.component_event_tx.clone(),
        channels.bootstrap_request_tx.clone(),
    ));
    let reaction_manager = Arc::new(ReactionManager::new(
        channels.component_event_tx.clone(),
    ));
    
    let data_router = Arc::new(DataRouter::new());
    let subscription_router = Arc::new(SubscriptionRouter::new());
    let bootstrap_router = Arc::new(BootstrapRouter::new());
    let read_only = Arc::new(true); // Enable read-only mode

    let router = Router::new()
        .route("/sources", axum::routing::post(api::handlers::create_source))
        .route("/sources/:id", axum::routing::put(api::handlers::update_source))
        .route("/sources/:id", axum::routing::delete(api::handlers::delete_source))
        .layer(Extension(source_manager))
        .layer(Extension(query_manager))
        .layer(Extension(reaction_manager))
        .layer(Extension(data_router))
        .layer(Extension(subscription_router))
        .layer(Extension(bootstrap_router))
        .layer(Extension(read_only));

    // Try to create a source in read-only mode
    let source_config = json!({
        "id": "readonly-test",
        "source_type": "internal.mock",
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
    let (router, _, _, _) = create_test_router().await;

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

    // Try to update with mismatched ID
    let config = json!({
        "id": "different-id",
        "source_type": "internal.mock",
        "auto_start": false,
        "properties": {}
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/sources/original-id")
                .header("content-type", "application/json")
                .body(Body::from(config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

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

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].is_string());
}

#[tokio::test]
async fn test_query_results_endpoint() {
    let (router, _, query_manager, _) = create_test_router().await;

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
    query_manager.add_query(query_config).await.unwrap();

    // Try to get results when query is not running
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
    assert!(json["error"].as_str().unwrap().contains("not running"));

    // Try to get results for non-existent query
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