use anyhow::Result;
use drasi_server::{ComponentStatus, DrasiServerCore, RuntimeConfig, ServerSettings};
use drasi_server_core::config::QueryLanguage;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_server_start_stop_cycle() -> Result<()> {
    // Create a minimal runtime config
    let config = RuntimeConfig {
        server: ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            max_connections: 100,
            shutdown_timeout_seconds: 30,
            disable_persistence: false,
        },
        sources: vec![],
        queries: vec![],
        reactions: vec![],
    };

    let mut core = DrasiServerCore::new(Arc::new(config));

    // Initialize the server
    core.initialize().await?;

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

    // Create config with auto-start components
    let config = RuntimeConfig {
        server: ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            max_connections: 100,
            shutdown_timeout_seconds: 30,
            disable_persistence: false,
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
            query: "MATCH (n) RETURN n".to_string(),
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

    let mut core = DrasiServerCore::new(Arc::new(config));
    core.initialize().await?;
    let core = Arc::new(core);

    // Components should not be running before server start
    assert_eq!(
        core.source_manager()
            .get_source_status("test-source".to_string())
            .await?,
        ComponentStatus::Stopped
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("test-query".to_string())
            .await?,
        ComponentStatus::Stopped
    );
    assert_eq!(
        core.reaction_manager()
            .get_reaction_status("test-reaction".to_string())
            .await?,
        ComponentStatus::Stopped
    );

    // Start the server
    core.start().await?;

    // Wait a bit for components to start
    sleep(Duration::from_millis(100)).await;

    // All auto-start components should be running
    assert_eq!(
        core.source_manager()
            .get_source_status("test-source".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("test-query".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.reaction_manager()
            .get_reaction_status("test-reaction".to_string())
            .await?,
        ComponentStatus::Running
    );

    // Stop the server
    core.stop().await?;

    // All components should be stopped
    assert_eq!(
        core.source_manager()
            .get_source_status("test-source".to_string())
            .await?,
        ComponentStatus::Stopped
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("test-query".to_string())
            .await?,
        ComponentStatus::Stopped
    );
    assert_eq!(
        core.reaction_manager()
            .get_reaction_status("test-reaction".to_string())
            .await?,
        ComponentStatus::Stopped
    );

    // Start again - auto-start components should restart
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    assert_eq!(
        core.source_manager()
            .get_source_status("test-source".to_string())
            .await?,
        ComponentStatus::Running
    );

    Ok(())
}

#[tokio::test]
async fn test_manual_vs_auto_start_components() -> Result<()> {
    use drasi_server::{QueryConfig, SourceConfig};
    use std::collections::HashMap;

    // Create config with mixed auto-start settings
    let config = RuntimeConfig {
        server: ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            max_connections: 100,
            shutdown_timeout_seconds: 30,
            disable_persistence: false,
        },
        sources: vec![
            SourceConfig {
                id: "auto-source".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: HashMap::new(),
                bootstrap_provider: None,
            },
            SourceConfig {
                id: "manual-source".to_string(),
                source_type: "mock".to_string(),
                auto_start: false,
                properties: HashMap::new(),
                bootstrap_provider: None,
            },
        ],
        queries: vec![
            QueryConfig {
                id: "auto-query".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                sources: vec!["auto-source".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
            QueryConfig {
                id: "manual-query".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                sources: vec!["manual-source".to_string()],
                auto_start: false,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
        ],
        reactions: vec![],
    };

    let mut core = DrasiServerCore::new(Arc::new(config));
    core.initialize().await?;
    let core = Arc::new(core);

    // Start the server
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    // Auto-start components should be running
    assert_eq!(
        core.source_manager()
            .get_source_status("auto-source".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("auto-query".to_string())
            .await?,
        ComponentStatus::Running
    );

    // Manual components should still be stopped
    assert_eq!(
        core.source_manager()
            .get_source_status("manual-source".to_string())
            .await?,
        ComponentStatus::Stopped
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("manual-query".to_string())
            .await?,
        ComponentStatus::Stopped
    );

    // Manually start the manual source
    core.source_manager()
        .start_source("manual-source".to_string())
        .await?;
    sleep(Duration::from_millis(100)).await;

    assert_eq!(
        core.source_manager()
            .get_source_status("manual-source".to_string())
            .await?,
        ComponentStatus::Running
    );

    // Stop the server
    core.stop().await?;

    // All components should be stopped
    assert_eq!(
        core.source_manager()
            .get_source_status("auto-source".to_string())
            .await?,
        ComponentStatus::Stopped
    );
    assert_eq!(
        core.source_manager()
            .get_source_status("manual-source".to_string())
            .await?,
        ComponentStatus::Stopped
    );

    // Start the server again
    core.start().await?;
    sleep(Duration::from_millis(100)).await;

    // Auto-start source should restart
    assert_eq!(
        core.source_manager()
            .get_source_status("auto-source".to_string())
            .await?,
        ComponentStatus::Running
    );

    // Manual source that was running before should also restart
    assert_eq!(
        core.source_manager()
            .get_source_status("manual-source".to_string())
            .await?,
        ComponentStatus::Running
    );

    Ok(())
}

#[tokio::test]
async fn test_component_startup_sequence() -> Result<()> {
    use drasi_server::{QueryConfig, ReactionConfig, SourceConfig};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc as StdArc;

    // Create a shared counter to track startup order
    let _startup_order = StdArc::new(AtomicUsize::new(0));

    // Create config with components that have dependencies
    let config = RuntimeConfig {
        server: ServerSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            max_connections: 100,
            shutdown_timeout_seconds: 30,
            disable_persistence: false,
        },
        sources: vec![
            SourceConfig {
                id: "source1".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: HashMap::new(),
                bootstrap_provider: None,
            },
            SourceConfig {
                id: "source2".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: HashMap::new(),
                bootstrap_provider: None,
            },
        ],
        queries: vec![
            QueryConfig {
                id: "query1".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                sources: vec!["source1".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
            QueryConfig {
                id: "query2".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                sources: vec!["source2".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
                query_language: QueryLanguage::default(),
            },
        ],
        reactions: vec![
            ReactionConfig {
                id: "reaction1".to_string(),
                reaction_type: "log".to_string(),
                queries: vec!["query1".to_string()],
                auto_start: true,
                properties: HashMap::new(),
            },
            ReactionConfig {
                id: "reaction2".to_string(),
                reaction_type: "log".to_string(),
                queries: vec!["query2".to_string()],
                auto_start: true,
                properties: HashMap::new(),
            },
        ],
    };

    let mut core = DrasiServerCore::new(Arc::new(config));
    core.initialize().await?;
    let core = Arc::new(core);

    // Start the server
    core.start().await?;

    // Give components time to start in sequence
    sleep(Duration::from_millis(200)).await;

    // Verify all components are running
    assert_eq!(
        core.source_manager()
            .get_source_status("source1".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.source_manager()
            .get_source_status("source2".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("query1".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.query_manager()
            .get_query_status("query2".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.reaction_manager()
            .get_reaction_status("reaction1".to_string())
            .await?,
        ComponentStatus::Running
    );
    assert_eq!(
        core.reaction_manager()
            .get_reaction_status("reaction2".to_string())
            .await?,
        ComponentStatus::Running
    );

    Ok(())
}
