use drasi_server::{ComponentStatus, DrasiServerBuilder, SourceConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_basic_server_lifecycle() {
    // Create a basic server
    let mut server = DrasiServerBuilder::new()
        .build_core()
        .await
        .expect("Failed to build server");

    // Initialize and start the server
    server
        .initialize()
        .await
        .expect("Failed to initialize server");
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Verify it's running
    assert!(server.is_running().await);

    // Shutdown
    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_components() {
    // Create server with components
    let mut server = DrasiServerBuilder::new()
        .with_simple_source("test_source", "mock")
        .with_simple_query(
            "test_query",
            "MATCH (n) RETURN n",
            vec!["test_source".to_string()],
        )
        .with_log_reaction("test_reaction", vec!["test_query".to_string()])
        .build_core()
        .await
        .expect("Failed to build server");

    server
        .initialize()
        .await
        .expect("Failed to initialize server");
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Wait a bit for components to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check component statuses
    let sources = server.source_manager().list_sources().await;
    assert_eq!(sources.len(), 1);
    assert!(sources.iter().any(|(name, status)| {
        name == "test_source" && matches!(status, ComponentStatus::Running)
    }));

    let queries = server.query_manager().list_queries().await;
    assert_eq!(queries.len(), 1);

    let reactions = server.reaction_manager().list_reactions().await;
    assert_eq!(reactions.len(), 1);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_dynamic_component_management() {
    // Start with empty server
    let mut server = DrasiServerBuilder::new()
        .build_core()
        .await
        .expect("Failed to build server");

    server
        .initialize()
        .await
        .expect("Failed to initialize server");
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Add source dynamically
    let source_config = SourceConfig {
        id: "dynamic_source".to_string(),
        source_type: "mock".to_string(),
        auto_start: false,
        properties: HashMap::new(),
        bootstrap_provider: None,
    };

    server
        .source_manager()
        .add_source(source_config)
        .await
        .expect("Failed to add source");

    // Verify source was added
    let sources = server.source_manager().list_sources().await;
    assert_eq!(sources.len(), 1);

    // Start the source
    server
        .source_manager()
        .start_source("dynamic_source".to_string())
        .await
        .expect("Failed to start source");

    // Wait for status update
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Check it's running
    let status = server
        .source_manager()
        .get_source_status("dynamic_source".to_string())
        .await
        .expect("Failed to get status");
    assert_eq!(status, ComponentStatus::Running);

    // Stop the source
    server
        .source_manager()
        .stop_source("dynamic_source".to_string())
        .await
        .expect("Failed to stop source");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_api() {
    // Create server with API
    let _server = DrasiServerBuilder::new()
        .enable_api_with_port(0) // Use port 0 for random available port
        .build()
        .await
        .expect("Failed to build server");

    // Note: We can't easily test the full API server in unit tests
    // as it requires running the blocking server.run() method.
    // This test just verifies the builder works correctly.

    // Test passes if builder completes without error
}

#[tokio::test]
async fn test_config_persistence() {
    let config_file = "test_config_persistence.yaml";

    // Create server with config persistence
    let _server = DrasiServerBuilder::new()
        .with_simple_source("persist_source", "mock")
        .enable_config_persistence(config_file)
        .build()
        .await
        .expect("Failed to build server");

    // Clean up test file if it exists
    let _ = std::fs::remove_file(config_file);

    // Note: Full persistence testing would require more setup
    // Test passes if builder completes without error
}

#[tokio::test]
async fn test_concurrent_operations() {
    let mut server = DrasiServerBuilder::new()
        .build_core()
        .await
        .expect("Failed to build server");

    server
        .initialize()
        .await
        .expect("Failed to initialize server");
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Spawn multiple tasks that add sources concurrently
    let mut tasks = vec![];
    for i in 0..5 {
        let source_manager = server.source_manager().clone();
        let task = tokio::spawn(async move {
            let config = SourceConfig {
                id: format!("concurrent_source_{}", i),
                source_type: "mock".to_string(),
                auto_start: false,
                properties: HashMap::new(),
                bootstrap_provider: None,
            };
            source_manager.add_source(config).await
        });
        tasks.push(task);
    }

    // Wait for all tasks
    for task in tasks {
        task.await
            .expect("Task panicked")
            .expect("Failed to add source");
    }

    // Verify all sources were added
    let sources = server.source_manager().list_sources().await;
    assert_eq!(sources.len(), 5);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_graceful_shutdown_timeout() {
    let mut server = DrasiServerBuilder::new()
        .with_simple_source("timeout_source", "mock")
        .build_core()
        .await
        .expect("Failed to build server");

    server
        .initialize()
        .await
        .expect("Failed to initialize server");
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Shutdown should complete within reasonable time
    let shutdown_result = timeout(Duration::from_secs(5), server.stop()).await;

    assert!(shutdown_result.is_ok(), "Shutdown timed out");
    shutdown_result.expect("Timeout").expect("Shutdown failed");
}
