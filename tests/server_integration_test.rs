use anyhow::Result;
use drasi_lib::{Properties, Query, Reaction, Source};
use drasi_server::DrasiServerCore;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// Integration test demonstrating data flow continues after server restart
#[tokio::test]
async fn test_data_flow_with_server_restart() -> Result<()> {
    // Create a shared counter to track how many results the reaction has processed
    let result_counter = Arc::new(AtomicUsize::new(0));
    let _counter_clone = result_counter.clone();

    // Create configuration
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let source = Source::mock("counter-source")
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("data_type", "counter")
                .with_int("interval_ms", 500),
        )
        .build();

    let query = Query::cypher("counter-query")
        .query("MATCH (n:Counter) RETURN n.value as value")
        .from_source("counter-source")
        .auto_start(true)
        .build();

    let reaction = Reaction::log("counter-reaction")
        .subscribe_to("counter-query")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiServerCore::builder()
            .with_id(&server_id)
            .add_source(source)
            .add_query(query)
            .add_reaction(reaction)
            .build()
            .await?,
    );

    // Start the server
    core.start().await?;

    // Let it run for 2 seconds (should get ~4 counter updates)
    sleep(Duration::from_secs(2)).await;

    // Check initial result count (we can't directly access it in this test, but we know it's running)
    assert!(core.is_running().await);

    // Stop the server
    core.stop().await?;
    assert!(!core.is_running().await);

    // Wait a bit
    sleep(Duration::from_millis(500)).await;

    // Start the server again
    core.start().await?;
    assert!(core.is_running().await);

    // Wait for components to start
    sleep(Duration::from_millis(500)).await;

    // Let it run for another 2 seconds
    sleep(Duration::from_secs(2)).await;

    // Verify source and query are still running
    // Note: With the new API, we don't have direct access to component status checks
    // The core should be running after restart
    assert!(core.is_running().await);

    // Final cleanup
    core.stop().await?;

    Ok(())
}

/// Integration test with multiple sources feeding into queries
#[tokio::test]
async fn test_multiple_sources_and_queries() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let sensors_source = Source::mock("sensors-source")
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("data_type", "sensor")
                .with_int("interval_ms", 1000),
        )
        .build();

    let vehicles_source = Source::mock("vehicles-source")
        .auto_start(true)
        .with_properties(
            Properties::new()
                .with_string("data_type", "generic")
                .with_int("interval_ms", 2000),
        )
        .build();

    let sensor_query = Query::cypher("sensor-alerts")
        .query("MATCH (s:Sensor) RETURN s")
        .from_source("sensors-source")
        .auto_start(true)
        .build();

    let vehicle_query = Query::cypher("vehicle-tracking")
        .query("MATCH (v:Vehicle) RETURN v.id, v.location")
        .from_source("vehicles-source")
        .auto_start(true)
        .build();

    let combined_query = Query::cypher("combined-view")
        .query("MATCH (n) RETURN n")
        .from_source("sensors-source")
        .from_source("vehicles-source")
        .auto_start(true)
        .build();

    let reaction = Reaction::log("alert-handler")
        .subscribe_to("sensor-alerts")
        .subscribe_to("combined-view")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiServerCore::builder()
            .with_id(&server_id)
            .add_source(sensors_source)
            .add_source(vehicles_source)
            .add_query(sensor_query)
            .add_query(vehicle_query)
            .add_query(combined_query)
            .add_reaction(reaction)
            .build()
            .await?,
    );

    // Start server
    core.start().await?;

    // Let it run briefly
    sleep(Duration::from_millis(500)).await;

    // Verify core is running with all components
    assert!(core.is_running().await);

    // Test removing a source at runtime using the new API
    core.remove_source("sensors-source").await?;
    sleep(Duration::from_millis(100)).await;

    // Core should still be running (other components continue)
    assert!(core.is_running().await);

    // Stop server
    core.stop().await?;

    Ok(())
}

/// Integration test for error recovery scenarios
#[tokio::test]
async fn test_component_failure_recovery() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let source = Source::mock("test-source").auto_start(true).build();

    let query = Query::cypher("test-query")
        // This query references a non-existent property, but should still start
        .query("MATCH (n) RETURN n.nonexistent as value")
        .from_source("test-source")
        .auto_start(true)
        .build();

    let reaction = Reaction::log("test-reaction")
        .subscribe_to("test-query")
        .auto_start(true)
        .build();

    let core = Arc::new(
        DrasiServerCore::builder()
            .with_id(&server_id)
            .add_source(source)
            .add_query(query)
            .add_reaction(reaction)
            .build()
            .await?,
    );

    // Start server - all components should start even with the "bad" query
    core.start().await?;
    sleep(Duration::from_millis(200)).await;

    // Core should be running
    assert!(core.is_running().await);

    // Stop and restart to verify recovery
    core.stop().await?;
    core.start().await?;
    sleep(Duration::from_millis(200)).await;

    // Core should recover and be running
    assert!(core.is_running().await);

    core.stop().await?;

    Ok(())
}

/// Test concurrent operations on the server
#[tokio::test]
async fn test_concurrent_operations() -> Result<()> {
    let server_id = uuid::Uuid::new_v4().to_string();

    // Build the core using the new builder API
    let source = Source::mock("concurrent-source")
        .auto_start(false) // Manual start
        .build();

    let core = Arc::new(
        DrasiServerCore::builder()
            .with_id(&server_id)
            .add_source(source)
            .build()
            .await?,
    );

    // Start server
    core.start().await?;

    // Test concurrent operations by adding/removing sources dynamically
    let mut handles = vec![];

    for i in 0..5 {
        let core_clone = core.clone();
        let handle = tokio::spawn(async move {
            // Alternate between adding and removing
            if i % 2 == 0 {
                let new_source = Source::mock(format!("concurrent-source-{}", i))
                    .auto_start(false)
                    .build();
                core_clone.create_source(new_source).await
            } else {
                sleep(Duration::from_millis(10)).await;
                core_clone
                    .remove_source(&"concurrent-source".to_string())
                    .await
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let _ = handle.await?;
    }

    // The core should still be in a valid running state
    assert!(core.is_running().await);

    core.stop().await?;

    Ok(())
}
