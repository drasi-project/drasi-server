//! Error resilience tests for the plugin system.
//!
//! Verifies graceful behavior when plugins receive bad config, unknown kinds
//! are requested, or dynamic loading encounters problems.

use drasi_server::api::models::BootstrapProviderConfig;
use drasi_server::config::{ReactionConfig, SourceConfig};
use drasi_server::factories::{create_reaction, create_source};
use drasi_server::plugin_registry::PluginRegistry;
use drasi_server::register_core_plugins;

fn core_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_core_plugins(&mut registry);
    registry
}

// ==========================================================================
// Unknown kind errors
// ==========================================================================

#[tokio::test]
async fn test_unknown_source_kind_returns_helpful_error() {
    let registry = core_registry();
    let config = SourceConfig {
        kind: "nosql-fantasy".to_string(),
        id: "test".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: serde_json::json!({}),
    };

    let err = create_source(&registry, config)
        .await
        .err()
        .expect("Expected error");
    let msg = err.to_string();
    assert!(msg.contains("Unknown source kind"), "Error: {msg}");
    assert!(
        msg.contains("nosql-fantasy"),
        "Error should include the requested kind: {msg}"
    );
}

#[tokio::test]
async fn test_unknown_reaction_kind_returns_helpful_error() {
    let registry = core_registry();
    let config = ReactionConfig {
        kind: "email-blast".to_string(),
        id: "test".to_string(),
        queries: vec!["q1".to_string()],
        auto_start: true,
        config: serde_json::json!({}),
    };

    let err = create_reaction(&registry, config)
        .await
        .err()
        .expect("Expected error");
    let msg = err.to_string();
    assert!(msg.contains("Unknown reaction kind"), "Error: {msg}");
    assert!(msg.contains("email-blast"), "Error: {msg}");
}

#[tokio::test]
async fn test_unknown_bootstrap_kind_returns_helpful_error() {
    use drasi_server::factories::create_bootstrap_provider;

    let registry = core_registry();
    let bootstrap_config = BootstrapProviderConfig {
        kind: "imaginary-bootstrap".to_string(),
        config: serde_json::json!({}),
    };
    let source_config_json = serde_json::json!({});

    let err = create_bootstrap_provider(&registry, &bootstrap_config, &source_config_json)
        .await
        .err()
        .expect("Expected error");
    let msg = err.to_string();
    assert!(msg.contains("Unknown bootstrap kind"), "Error: {msg}");
    assert!(msg.contains("imaginary-bootstrap"), "Error: {msg}");
}

// ==========================================================================
// Empty registry errors
// ==========================================================================

#[tokio::test]
async fn test_empty_registry_rejects_all_sources() {
    let registry = PluginRegistry::new();
    let config = SourceConfig {
        kind: "mock".to_string(),
        id: "test".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: serde_json::json!({}),
    };

    let err = create_source(&registry, config)
        .await
        .err()
        .expect("Expected error");
    assert!(err.to_string().contains("Unknown source kind"));
}

#[tokio::test]
async fn test_empty_registry_rejects_all_reactions() {
    let registry = PluginRegistry::new();
    let config = ReactionConfig {
        kind: "log".to_string(),
        id: "test".to_string(),
        queries: vec![],
        auto_start: true,
        config: serde_json::json!({}),
    };

    let err = create_reaction(&registry, config)
        .await
        .err()
        .expect("Expected error");
    assert!(err.to_string().contains("Unknown reaction kind"));
}

// ==========================================================================
// Dynamic loading edge cases
// ==========================================================================

#[test]
fn test_dynamic_loading_nonexistent_dir() {
    let mut registry = PluginRegistry::new();
    let stats = drasi_server::dynamic_loading::load_plugins(
        std::path::Path::new("/nonexistent/path/to/plugins"),
        &mut registry,
        None,
    )
    .unwrap();

    assert_eq!(stats.plugins_loaded, 0);
}

#[test]
fn test_dynamic_loading_empty_dir() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut registry = PluginRegistry::new();
    let stats =
        drasi_server::dynamic_loading::load_plugins(temp_dir.path(), &mut registry, None).unwrap();

    assert_eq!(stats.plugins_loaded, 0);
}

#[test]
fn test_dynamic_loading_skips_non_library_files() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    std::fs::write(temp_dir.path().join("README.md"), "# Not a plugin").unwrap();
    std::fs::write(temp_dir.path().join("config.yaml"), "key: value").unwrap();
    std::fs::write(temp_dir.path().join("data.json"), "{}").unwrap();

    let mut registry = PluginRegistry::new();
    let stats =
        drasi_server::dynamic_loading::load_plugins(temp_dir.path(), &mut registry, None).unwrap();

    assert_eq!(
        stats.plugins_loaded, 0,
        "Non-library files should be skipped"
    );
}

// ==========================================================================
// Config deserialization edge cases
// ==========================================================================

#[test]
fn test_source_config_rejects_snake_case_auto_start() {
    let yaml = r#"
        kind: mock
        id: test
        auto_start: true
    "#;

    let result: Result<SourceConfig, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "snake_case auto_start should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("autoStart"),
        "Error should suggest camelCase: {err}"
    );
}

#[test]
fn test_reaction_config_rejects_snake_case_auto_start() {
    let yaml = r#"
        kind: log
        id: test
        queries: ["q1"]
        auto_start: true
    "#;

    let result: Result<ReactionConfig, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "snake_case auto_start should be rejected");
}

#[test]
fn test_source_config_rejects_snake_case_bootstrap_provider() {
    let yaml = r#"
        kind: mock
        id: test
        bootstrap_provider:
          kind: noop
    "#;

    let result: Result<SourceConfig, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "snake_case bootstrap_provider should be rejected"
    );
}
