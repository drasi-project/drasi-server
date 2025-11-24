use drasi_lib::Source;
use drasi_server::DrasiServerBuilder;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_basic_server_lifecycle() {
    // Create a basic server using the new builder API
    let server = DrasiServerBuilder::new()
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Verify it's running
    assert!(server.is_running().await);

    // Shutdown
    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_components() {
    // Create server with components using the new builder API
    let server = DrasiServerBuilder::new()
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

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Wait a bit for components to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check that server is running with all components
    assert!(server.is_running().await);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_dynamic_component_management() {
    // Start with empty server using the new builder API
    let server = DrasiServerBuilder::new()
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Add source dynamically using the new runtime API
    let source_config = Source::mock("dynamic_source").auto_start(true).build();

    server
        .create_source(source_config)
        .await
        .expect("Failed to add source");

    // Wait for source to start (auto_start is true)
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Server should still be running with the new source
    assert!(server.is_running().await);

    // Remove the source using the new API
    server
        .remove_source("dynamic_source")
        .await
        .expect("Failed to remove source");

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_server_with_api() {
    // Create server with API
    let _server = DrasiServerBuilder::new()
        .with_port(0) // Use port 0 for random available port
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
        .with_config_file(config_file)
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
    // Start with empty server using the new builder API
    let server = DrasiServerBuilder::new()
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Spawn multiple tasks that add sources concurrently using the new runtime API
    let mut tasks = vec![];
    for i in 0..5 {
        let server_clone = server.clone();
        let task = tokio::spawn(async move {
            let config = Source::mock(format!("concurrent_source_{}", i))
                .auto_start(false)
                .build();
            server_clone.create_source(config).await
        });
        tasks.push(task);
    }

    // Wait for all tasks
    for task in tasks {
        task.await
            .expect("Task panicked")
            .expect("Failed to add source");
    }

    // Server should still be running with all sources
    assert!(server.is_running().await);

    server.stop().await.expect("Failed to stop server");
}

#[tokio::test]
async fn test_graceful_shutdown_timeout() {
    // Create server with a source using the new builder API
    let server = DrasiServerBuilder::new()
        .with_simple_source("timeout_source", "mock")
        .build_core()
        .await
        .expect("Failed to build server");

    // The builder already initializes the server, just start it
    let server = Arc::new(server);
    server.start().await.expect("Failed to start server");

    // Shutdown should complete within reasonable time
    let shutdown_result = timeout(Duration::from_secs(5), server.stop()).await;

    assert!(shutdown_result.is_ok(), "Shutdown timed out");
    shutdown_result.expect("Timeout").expect("Shutdown failed");
}
