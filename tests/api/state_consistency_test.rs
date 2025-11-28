//! API State Consistency Tests
//!
//! These tests ensure API operations maintain consistent state across components,
//! testing the public API for component lifecycle management.

use crate::test_utils::{create_mock_reaction_registry, create_mock_source_registry};
use drasi_lib::{DrasiLib, Query, ReactionConfig, SourceConfig};
use std::sync::Arc;

#[tokio::test]
async fn test_server_start_stop_cycle() {
    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
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
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Create source and reaction dynamically
    let source = SourceConfig::new("test-source", "mock").with_auto_start(true);
    core.create_source(source).await.expect("Failed to create source");

    let reaction = ReactionConfig::new("test-reaction", "log")
        .with_query("test-query")
        .with_auto_start(true);
    core.create_reaction(reaction).await.expect("Failed to create reaction");

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
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(false)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Create source and reaction dynamically with auto_start=false
    let source = SourceConfig::new("test-source", "mock").with_auto_start(false);
    core.create_source(source).await.expect("Failed to create source");

    let reaction = ReactionConfig::new("test-reaction", "log")
        .with_query("test-query")
        .with_auto_start(false);
    core.create_reaction(reaction).await.expect("Failed to create reaction");

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
    let query = Query::cypher("restart-query")
        .query("MATCH (n) RETURN n")
        .from_source("restart-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Create source dynamically
    let source = SourceConfig::new("restart-source", "mock").with_auto_start(true);
    core.create_source(source).await.expect("Failed to create source");

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
    let query = Query::cypher("multi-source-query")
        .query("MATCH (n) RETURN n")
        .from_source("source1")
        .from_source("source2")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Create sources dynamically
    let source1 = SourceConfig::new("source1", "mock").with_auto_start(true);
    core.create_source(source1).await.expect("Failed to create source1");

    let source2 = SourceConfig::new("source2", "mock").with_auto_start(true);
    core.create_source(source2).await.expect("Failed to create source2");

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
    let query1 = Query::cypher("query1")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();
    let query2 = Query::cypher("query2")
        .query("MATCH (m) RETURN m")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query1)
        .add_query(query2)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Create source dynamically
    let source = SourceConfig::new("test-source", "mock").with_auto_start(true);
    core.create_source(source).await.expect("Failed to create source");

    // Create reaction subscribing to multiple queries
    let reaction = ReactionConfig::new("multi-query-reaction", "log")
        .with_query("query1")
        .with_query("query2")
        .with_auto_start(true);
    core.create_reaction(reaction).await.expect("Failed to create reaction");

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
    // For joins, we need to use the lower-level QueryConfig since the builder API
    // may not support join configuration yet
    use drasi_lib::config::{QueryJoinConfig, QueryJoinKeyConfig};
    let query = Query::cypher("join-query")
        .query("MATCH (a:TypeA)<-[:LINKED]-(b:TypeB) RETURN a, b")
        .from_source("join-source1")
        .from_source("join-source2")
        .auto_start(true)
        .with_joins(vec![QueryJoinConfig {
            id: "LINKED".to_string(),
            keys: vec![
                QueryJoinKeyConfig {
                    label: "TypeA".to_string(),
                    property: "id".to_string(),
                },
                QueryJoinKeyConfig {
                    label: "TypeB".to_string(),
                    property: "type_a_id".to_string(),
                },
            ],
        }])
        .build();

    let core = DrasiLib::builder()
        .with_id("test-server")
        .with_source_registry(create_mock_source_registry())
        .with_reaction_registry(create_mock_reaction_registry())
        .add_query(query)
        .build()
        .await
        .expect("Failed to build test core");

    let core = Arc::new(core);

    // Create sources dynamically
    let source1 = SourceConfig::new("join-source1", "mock").with_auto_start(true);
    core.create_source(source1).await.expect("Failed to create source1");

    let source2 = SourceConfig::new("join-source2", "mock").with_auto_start(true);
    core.create_source(source2).await.expect("Failed to create source2");

    // Start server with query that has joins
    core.start().await.expect("Failed to start");
    assert!(core.is_running().await);

    // Let components initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    assert!(core.is_running().await);

    core.stop().await.ok();
}
