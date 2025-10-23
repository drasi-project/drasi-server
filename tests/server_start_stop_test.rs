use anyhow::Result;
use drasi_server::DrasiServerCore;
use drasi_server_core::config::QueryLanguage;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_server_start_stop_cycle() -> Result<()> {
    // Create a minimal runtime config
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let core = DrasiServerCore::builder()
        .with_id(&server_id)
        .build()
        .await?;

    // Convert to Arc for repeated use
    let core = Arc::new(core);

    // Server should not be running initially
    assert!(!core.is_running().await);

    // Start the server
    core.start().await?;
    assert!(core.is_running().await);

    // Try to start again (should fail)
    assert!(core.start().await.is_err());

    // Stop the server
    core.stop().await?;
    assert!(!core.is_running().await);

    // Try to stop again (should fail)
    assert!(core.stop().await.is_err());

    // Start again
    core.start().await?;
    assert!(core.is_running().await);

    // Stop again
    core.stop().await?;
    assert!(!core.is_running().await);

    Ok(())
}

#[tokio::test]
async fn test_auto_start_components() -> Result<()> {
    use drasi_server::{QueryConfig, ReactionConfig, SourceConfig};
    use std::collections::HashMap;

    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with auto-start components
    let core = DrasiServerCore::builder()
        .with_id(&server_id)
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
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
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
        .await?;

    let core = Arc::new(core);

    // Components are configured but not running before server start
    assert!(!core.is_running().await);

    // Start the server
    core.start().await?;

    // Wait a bit for components to start
    sleep(Duration::from_millis(100)).await;

    // All auto-start components should be running
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;

    // All components should be stopped
    assert!(!core.is_running().await);

    // Start again - auto-start components should restart
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    assert!(core.is_running().await);

    Ok(())
}

#[tokio::test]
async fn test_manual_vs_auto_start_components() -> Result<()> {
    use drasi_server::{QueryConfig, SourceConfig};
    use std::collections::HashMap;

    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with mixed auto-start settings
    let core = DrasiServerCore::builder()
        .with_id(&server_id)
        .add_source(SourceConfig {
            id: "auto-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_source(SourceConfig {
            id: "manual-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: false,
            properties: HashMap::new(),
            bootstrap_provider: None,
        })
        .add_query(QueryConfig {
            id: "auto-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["auto-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        })
        .add_query(QueryConfig {
            id: "manual-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["manual-source".to_string()],
            auto_start: false,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        })
        .build()
        .await?;

    let core = Arc::new(core);

    // Start the server
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    // Auto-start components should be running
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;

    // All components should be stopped
    assert!(!core.is_running().await);

    // Start the server again
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    // Auto-start components should restart
    assert!(core.is_running().await);

    Ok(())
}

#[tokio::test]
async fn test_component_startup_sequence() -> Result<()> {
    use drasi_server::{QueryConfig, ReactionConfig, SourceConfig};
    use std::collections::HashMap;

    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with components that have dependencies
    let core = DrasiServerCore::builder()
        .with_id(&server_id)
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
            id: "query1".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source1".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        })
        .add_query(QueryConfig {
            id: "query2".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            query_language: QueryLanguage::default(),
            priority_queue_capacity: None,
        })
        .add_reaction(ReactionConfig {
            id: "reaction1".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["query1".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            priority_queue_capacity: None,
        })
        .add_reaction(ReactionConfig {
            id: "reaction2".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["query2".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            priority_queue_capacity: None,
        })
        .build()
        .await?;

    let core = Arc::new(core);

    // Start the server
    core.start().await?;

    // Give components time to start in sequence
    sleep(Duration::from_millis(200)).await;

    // Verify all components are running
    assert!(core.is_running().await);

    Ok(())
}
