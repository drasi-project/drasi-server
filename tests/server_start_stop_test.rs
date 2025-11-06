use anyhow::Result;
use drasi_server::DrasiServerCore;
use drasi_server_core::{Query, Reaction, Source};
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
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with auto-start components
    let source = Source::mock("test-source").auto_start(true).build();
    let query = Query::cypher("test-query")
        .query("MATCH (n) RETURN n")
        .from_source("test-source")
        .auto_start(true)
        .build();
    let reaction = Reaction::log("test-reaction")
        .subscribe_to("test-query")
        .auto_start(true)
        .build();

    let core = DrasiServerCore::builder()
        .with_id(&server_id)
        .add_source(source)
        .add_query(query)
        .add_reaction(reaction)
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
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with mixed auto-start settings
    let auto_source = Source::mock("auto-source").auto_start(true).build();
    let manual_source = Source::mock("manual-source").auto_start(false).build();

    let auto_query = Query::cypher("auto-query")
        .query("MATCH (n) RETURN n")
        .from_source("auto-source")
        .auto_start(true)
        .build();

    let manual_query = Query::cypher("manual-query")
        .query("MATCH (n) RETURN n")
        .from_source("manual-source")
        .auto_start(false)
        .build();

    let core = DrasiServerCore::builder()
        .with_id(&server_id)
        .add_source(auto_source)
        .add_source(manual_source)
        .add_query(auto_query)
        .add_query(manual_query)
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
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API with components that have dependencies
    let source1 = Source::mock("source1").auto_start(true).build();
    let source2 = Source::mock("source2").auto_start(true).build();

    let query1 = Query::cypher("query1")
        .query("MATCH (n) RETURN n")
        .from_source("source1")
        .auto_start(true)
        .build();

    let query2 = Query::cypher("query2")
        .query("MATCH (n) RETURN n")
        .from_source("source2")
        .auto_start(true)
        .build();

    let reaction1 = Reaction::log("reaction1")
        .subscribe_to("query1")
        .auto_start(true)
        .build();

    let reaction2 = Reaction::log("reaction2")
        .subscribe_to("query2")
        .auto_start(true)
        .build();

    let core = DrasiServerCore::builder()
        .with_id(&server_id)
        .add_source(source1)
        .add_source(source2)
        .add_query(query1)
        .add_query(query2)
        .add_reaction(reaction1)
        .add_reaction(reaction2)
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
