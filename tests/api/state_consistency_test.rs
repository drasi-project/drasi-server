//! API State Consistency Tests
//!
//! These tests ensure API operations maintain consistent state across components,
//! testing the public API for component lifecycle management.

use drasi_server_core::{config::QueryLanguage, DrasiServerCore, QueryConfig, ReactionConfig, SourceConfig};
use std::collections::HashMap;
use std::sync::Arc;

/// Helper function to create test DrasiServerCore with empty config
async fn create_test_core() -> Arc<DrasiServerCore> {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);
    core.start().await.expect("Failed to start core");
    core
}

#[tokio::test]
async fn test_server_start_stop_cycle() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Initially not running
    assert!(!core.is_running().await);

    // Start server
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);

    // Can start again
    core.start().await.expect("Failed to restart");
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_components_with_auto_start() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .add_source(SourceConfig {
            id: "test-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["test-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .add_reaction(ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["test-query".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            priority_queue_capacity: None,
        })
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server - components should auto-start
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Components should be running (we can't check individual status through public API)
    assert!(core.is_running().await);

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);
}

#[tokio::test]
async fn test_components_without_auto_start() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .add_source(SourceConfig {
            id: "test-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: false,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["test-source".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .add_reaction(ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["test-query".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            priority_queue_capacity: None,
        })
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server - components should NOT auto-start
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Components exist but are not started (we can't check individual status through public API)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Server should still be running
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_restart_with_components() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .add_source(SourceConfig {
            id: "restart-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "restart-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["restart-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components start
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Stop server
    core.stop().await.expect("Failed to stop");
    assert!(!core.is_running().await);

    // Restart server
    core.start().await.expect("Failed to restart");
    assert!(core.is_running().await);

    // Let components restart
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Server should still be running after restart
    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_multiple_query_sources() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .add_source(SourceConfig {
            id: "source1".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_source(SourceConfig {
            id: "source2".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "multi-source-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source1".to_string(), "source2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with multiple sources
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_multiple_reaction_queries() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .add_source(SourceConfig {
            id: "test-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "query1".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["test-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .add_query(QueryConfig {
            id: "query2".to_string(),
            query: "MATCH (m) RETURN m".to_string(),
            sources: vec!["test-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .add_reaction(ReactionConfig {
            id: "multi-query-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["query1".to_string(), "query2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            priority_queue_capacity: None,
        })
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with reaction subscribing to multiple queries
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}

#[tokio::test]
async fn test_query_with_joins() {
    let core = DrasiServerCore::builder()
        .with_id("test-server")
        .add_source(SourceConfig {
            id: "join-source1".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_source(SourceConfig {
            id: "join-source2".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "join-query".to_string(),
            query: "MATCH (a:TypeA)<-[:LINKED]-(b:TypeB) RETURN a, b".to_string(),
            sources: vec!["join-source1".to_string(), "join-source2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: Some(vec![drasi_server_core::config::QueryJoinConfig {
                id: "LINKED".to_string(),
                keys: vec![
                    drasi_server_core::config::QueryJoinKeyConfig {
                        label: "TypeA".to_string(),
                        property: "id".to_string(),
                    },
                    drasi_server_core::config::QueryJoinKeyConfig {
                        label: "TypeB".to_string(),
                        property: "type_a_id".to_string(),
                    },
                ],
            }]),
            query_language: QueryLanguage::default(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            priority_queue_capacity: None,
        })
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Start server with query that has joins
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}
