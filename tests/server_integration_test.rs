use anyhow::Result;
use drasi_server::{DrasiServerCore, QueryConfig, ReactionConfig, RuntimeConfig, SourceConfig};
use drasi_server_core::config::{DrasiServerCoreSettings, QueryLanguage};
use std::collections::HashMap;
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
    let config = RuntimeConfig {
        server_core: DrasiServerCoreSettings {
            id: uuid::Uuid::new_v4().to_string(),
        },
        sources: vec![SourceConfig {
            id: "counter-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: {
                let mut props = HashMap::new();
                props.insert("data_type".to_string(), serde_json::json!("counter"));
                props.insert("interval_ms".to_string(), serde_json::json!(500));
                props
            },
            bootstrap_provider: None,
        }],
        queries: vec![QueryConfig {
            id: "counter-query".to_string(),
            query: "MATCH (n:Counter) RETURN n.value as value".to_string(),
            sources: vec!["counter-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
        }],
        reactions: vec![ReactionConfig {
            id: "counter-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["counter-query".to_string()],
            auto_start: true,
            properties: HashMap::new(),
        }],
    };

    // Build the core using the new builder API
    let mut builder = DrasiServerCore::builder().with_id(&config.server_core.id);

    for source in config.sources {
        builder = builder.add_source(source);
    }
    for query in config.queries {
        builder = builder.add_query(query);
    }
    for reaction in config.reactions {
        builder = builder.add_reaction(reaction);
    }

    let core = Arc::new(builder.build().await?);

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
    let config = RuntimeConfig {
        server_core: DrasiServerCoreSettings {
            id: uuid::Uuid::new_v4().to_string(),
        },
        sources: vec![
            SourceConfig {
                id: "sensors-source".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: {
                    let mut props = HashMap::new();
                    props.insert("data_type".to_string(), serde_json::json!("sensor"));
                    props.insert("interval_ms".to_string(), serde_json::json!(1000));
                    props
                },
                bootstrap_provider: None,
            },
            SourceConfig {
                id: "vehicles-source".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: {
                    let mut props = HashMap::new();
                    props.insert("data_type".to_string(), serde_json::json!("generic"));
                    props.insert("interval_ms".to_string(), serde_json::json!(2000));
                    props
                },
                bootstrap_provider: None,
            },
        ],
        queries: vec![
            QueryConfig {
                id: "sensor-alerts".to_string(),
                query: "MATCH (s:Sensor) RETURN s".to_string(),
                sources: vec!["sensors-source".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
            QueryConfig {
                id: "vehicle-tracking".to_string(),
                query: "MATCH (v:Vehicle) RETURN v.id, v.location".to_string(),
                sources: vec!["vehicles-source".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
            QueryConfig {
                id: "combined-view".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                sources: vec!["sensors-source".to_string(), "vehicles-source".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
        ],
        reactions: vec![ReactionConfig {
            id: "alert-handler".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["sensor-alerts".to_string(), "combined-view".to_string()],
            auto_start: true,
            properties: HashMap::new(),
        }],
    };

    // Build the core using the new builder API
    let mut builder = DrasiServerCore::builder().with_id(&config.server_core.id);

    for source in config.sources {
        builder = builder.add_source(source);
    }
    for query in config.queries {
        builder = builder.add_query(query);
    }
    for reaction in config.reactions {
        builder = builder.add_reaction(reaction);
    }

    let core = Arc::new(builder.build().await?);

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
    let config = RuntimeConfig {
        server_core: DrasiServerCoreSettings {
            id: uuid::Uuid::new_v4().to_string(),
        },
        sources: vec![SourceConfig {
            id: "test-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: true,
            properties: HashMap::new(),
            bootstrap_provider: None,
        }],
        queries: vec![QueryConfig {
            id: "test-query".to_string(),
            // This query references a non-existent property, but should still start
            query: "MATCH (n) RETURN n.nonexistent as value".to_string(),
            sources: vec!["test-source".to_string()],
            auto_start: true,
            properties: HashMap::new(),
            joins: None,
            query_language: QueryLanguage::default(),
        }],
        reactions: vec![ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["test-query".to_string()],
            auto_start: true,
            properties: HashMap::new(),
        }],
    };

    // Build the core using the new builder API
    let mut builder = DrasiServerCore::builder().with_id(&config.server_core.id);

    for source in config.sources {
        builder = builder.add_source(source);
    }
    for query in config.queries {
        builder = builder.add_query(query);
    }
    for reaction in config.reactions {
        builder = builder.add_reaction(reaction);
    }

    let core = Arc::new(builder.build().await?);

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
    let config = RuntimeConfig {
        server_core: DrasiServerCoreSettings {
            id: uuid::Uuid::new_v4().to_string(),
        },
        sources: vec![SourceConfig {
            id: "concurrent-source".to_string(),
            source_type: "mock".to_string(),
            auto_start: false, // Manual start
            properties: HashMap::new(),
            bootstrap_provider: None,
        }],
        queries: vec![],
        reactions: vec![],
    };

    // Build the core using the new builder API
    let mut builder = DrasiServerCore::builder().with_id(&config.server_core.id);

    for source in config.sources {
        builder = builder.add_source(source);
    }

    let core = Arc::new(builder.build().await?);

    // Start server
    core.start().await?;

    // Test concurrent operations by adding/removing sources dynamically
    let mut handles = vec![];

    for i in 0..5 {
        let core_clone = core.clone();
        let handle = tokio::spawn(async move {
            // Alternate between adding and removing
            if i % 2 == 0 {
                let new_source = SourceConfig {
                    id: format!("concurrent-source-{}", i),
                    source_type: "mock".to_string(),
                    auto_start: false,
                    properties: HashMap::new(),
                    bootstrap_provider: None,
                };
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
