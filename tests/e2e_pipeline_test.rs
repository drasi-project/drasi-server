//! End-to-end pipeline tests with registry-backed plugin creation.
//!
//! Tests the full Source → Query → Reaction pipeline using the PluginRegistry
//! to create components from config, simulating how the server starts up.

use drasi_lib::DrasiLib;
use drasi_server::api::models::BootstrapProviderConfig;
use drasi_server::builtin_plugins::register_builtin_plugins;
use drasi_server::config::{ReactionConfig, SourceConfig};
use drasi_server::factories::{create_reaction, create_source};
use drasi_server::plugin_registry::PluginRegistry;
use drasi_server::register_core_plugins;

fn populated_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_core_plugins(&mut registry);
    register_builtin_plugins(&mut registry);
    registry
}

/// Test that a full pipeline can be assembled from registry-created components.
#[tokio::test]
async fn test_mock_source_query_log_reaction_pipeline() {
    let registry = populated_registry();

    // Create a mock source via the registry
    let source_config = SourceConfig {
        kind: "mock".to_string(),
        id: "e2e-source".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: serde_json::json!({
            "dataType": { "type": "generic" },
            "intervalMs": 60000
        }),
    };
    let source = create_source(&registry, source_config)
        .await
        .expect("Failed to create source");

    // Create a log reaction via the registry
    let reaction_config = ReactionConfig {
        kind: "log".to_string(),
        id: "e2e-reaction".to_string(),
        queries: vec!["e2e-query".to_string()],
        auto_start: true,
        config: serde_json::json!({}),
    };
    let reaction = create_reaction(&registry, reaction_config)
        .await
        .expect("Failed to create reaction");

    // Build a DrasiLib instance with these components
    let query = drasi_lib::Query::cypher("e2e-query")
        .query("MATCH (n:Node) RETURN n")
        .from_source("e2e-source")
        .build();

    let core = DrasiLib::builder()
        .with_id("e2e-test")
        .with_source(source)
        .with_query(query)
        .with_reaction(reaction)
        .build()
        .await
        .expect("Failed to build DrasiLib");

    // Verify the components are registered
    let config = core.get_current_config().await.unwrap();
    assert_eq!(config.id, "e2e-test");

    let sources = core.list_sources().await.unwrap();
    assert!(sources.iter().any(|(id, _)| id == "e2e-source"));

    let queries = core.list_queries().await.unwrap();
    assert!(queries.iter().any(|(id, _)| id == "e2e-query"));

    let reactions = core.list_reactions().await.unwrap();
    assert!(reactions.iter().any(|(id, _)| id == "e2e-reaction"));
}

/// Test pipeline with mock source + noop bootstrap + log reaction.
#[tokio::test]
async fn test_pipeline_with_bootstrap_provider() {
    let registry = populated_registry();

    let source_config = SourceConfig {
        kind: "mock".to_string(),
        id: "bootstrap-e2e-source".to_string(),
        auto_start: true,
        bootstrap_provider: Some(BootstrapProviderConfig {
            kind: "noop".to_string(),
            config: serde_json::json!({}),
        }),
        config: serde_json::json!({
            "dataType": { "type": "generic" },
            "intervalMs": 60000
        }),
    };
    let source = create_source(&registry, source_config)
        .await
        .expect("Failed to create source with bootstrap");

    let reaction_config = ReactionConfig {
        kind: "log".to_string(),
        id: "bootstrap-e2e-reaction".to_string(),
        queries: vec!["bootstrap-query".to_string()],
        auto_start: true,
        config: serde_json::json!({}),
    };
    let reaction = create_reaction(&registry, reaction_config)
        .await
        .expect("Failed to create reaction");

    let query = drasi_lib::Query::cypher("bootstrap-query")
        .query("MATCH (n:Sensor) WHERE n.temperature > 75 RETURN n")
        .from_source("bootstrap-e2e-source")
        .build();

    let core = DrasiLib::builder()
        .with_id("bootstrap-e2e")
        .with_source(source)
        .with_query(query)
        .with_reaction(reaction)
        .build()
        .await
        .expect("Failed to build DrasiLib with bootstrap");

    // Start and stop to verify lifecycle works
    core.start().await.expect("Failed to start core");
    core.stop().await.expect("Failed to stop core");
}

/// Test that multiple sources and reactions can be assembled.
#[tokio::test]
async fn test_multi_component_pipeline() {
    let registry = populated_registry();

    // Create two mock sources
    let source1 = create_source(
        &registry,
        SourceConfig {
            kind: "mock".to_string(),
            id: "multi-source-1".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "dataType": { "type": "generic" },
                "intervalMs": 60000
            }),
        },
    )
    .await
    .unwrap();

    let source2 = create_source(
        &registry,
        SourceConfig {
            kind: "mock".to_string(),
            id: "multi-source-2".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: serde_json::json!({
                "dataType": { "type": "generic" },
                "intervalMs": 60000
            }),
        },
    )
    .await
    .unwrap();

    // Create two reactions
    let reaction1 = create_reaction(
        &registry,
        ReactionConfig {
            kind: "log".to_string(),
            id: "multi-reaction-1".to_string(),
            queries: vec!["multi-query".to_string()],
            auto_start: true,
            config: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let reaction2 = create_reaction(
        &registry,
        ReactionConfig {
            kind: "profiler".to_string(),
            id: "multi-reaction-2".to_string(),
            queries: vec!["multi-query".to_string()],
            auto_start: true,
            config: serde_json::json!({
                "windowSize": 100,
                "reportIntervalSecs": 10
            }),
        },
    )
    .await
    .unwrap();

    let query = drasi_lib::Query::cypher("multi-query")
        .query("MATCH (n) RETURN n")
        .from_source("multi-source-1")
        .build();

    let core = DrasiLib::builder()
        .with_id("multi-test")
        .with_source(source1)
        .with_source(source2)
        .with_query(query)
        .with_reaction(reaction1)
        .with_reaction(reaction2)
        .build()
        .await
        .expect("Failed to build multi-component DrasiLib");

    let sources = core.list_sources().await.unwrap();
    assert_eq!(sources.len(), 2);

    let reactions = core.list_reactions().await.unwrap();
    assert_eq!(reactions.len(), 2);
}

/// Test creating components from YAML-like config structures (simulating config file parsing).
#[tokio::test]
async fn test_config_roundtrip_through_registry() {
    let registry = populated_registry();

    // Simulate what happens when a YAML config is loaded
    let yaml_source = r#"
        kind: mock
        id: yaml-source
        autoStart: true
        dataType:
          type: generic
        intervalMs: 5000
    "#;

    // Parse the YAML into our config struct
    let source_config: SourceConfig = serde_yaml::from_str(yaml_source)
        .expect("Failed to parse source YAML");

    assert_eq!(source_config.kind, "mock");
    assert_eq!(source_config.id, "yaml-source");
    assert!(source_config.auto_start);

    // Create the source through the registry
    let source = create_source(&registry, source_config)
        .await
        .expect("Failed to create source from YAML config");
    assert_eq!(source.id(), "yaml-source");

    // Same for reaction
    let yaml_reaction = r#"
        kind: log
        id: yaml-reaction
        queries:
          - yaml-query
        autoStart: true
    "#;

    let reaction_config: ReactionConfig = serde_yaml::from_str(yaml_reaction)
        .expect("Failed to parse reaction YAML");

    let reaction = create_reaction(&registry, reaction_config)
        .await
        .expect("Failed to create reaction from YAML config");
    assert_eq!(reaction.id(), "yaml-reaction");
}
