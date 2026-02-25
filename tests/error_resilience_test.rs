//! Error resilience tests for the plugin system.
//!
//! Verifies graceful behavior when plugins receive bad config, unknown kinds
//! are requested, or dynamic loading encounters problems.

use drasi_server::builtin_plugins::register_builtin_plugins;
use drasi_server::config::{ReactionConfig, SourceConfig};
use drasi_server::api::models::BootstrapProviderConfig;
use drasi_server::factories::{create_reaction, create_source};
use drasi_server::plugin_registry::PluginRegistry;
use drasi_server::register_core_plugins;

fn populated_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_core_plugins(&mut registry);
    register_builtin_plugins(&mut registry);
    registry
}

// ==========================================================================
// Unknown kind errors
// ==========================================================================

#[tokio::test]
async fn test_unknown_source_kind_returns_helpful_error() {
    let registry = populated_registry();
    let config = SourceConfig {
        kind: "nosql-fantasy".to_string(),
        id: "test".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: serde_json::json!({}),
    };

    let err = create_source(&registry, config).await.err().expect("Expected error");
    let msg = err.to_string();
    assert!(msg.contains("Unknown source kind"), "Error: {msg}");
    assert!(
        msg.contains("nosql-fantasy"),
        "Error should include the requested kind: {msg}"
    );
    assert!(
        msg.contains("mock"),
        "Error should list available kinds: {msg}"
    );
}

#[tokio::test]
async fn test_unknown_reaction_kind_returns_helpful_error() {
    let registry = populated_registry();
    let config = ReactionConfig {
        kind: "email-blast".to_string(),
        id: "test".to_string(),
        queries: vec!["q1".to_string()],
        auto_start: true,
        config: serde_json::json!({}),
    };

    let err = create_reaction(&registry, config).await.err().expect("Expected error");
    let msg = err.to_string();
    assert!(msg.contains("Unknown reaction kind"), "Error: {msg}");
    assert!(msg.contains("email-blast"), "Error: {msg}");
    assert!(msg.contains("log"), "Should list available kinds: {msg}");
}

#[tokio::test]
async fn test_unknown_bootstrap_kind_returns_helpful_error() {
    let registry = populated_registry();
    let config = SourceConfig {
        kind: "mock".to_string(),
        id: "test".to_string(),
        auto_start: true,
        bootstrap_provider: Some(BootstrapProviderConfig {
            kind: "imaginary-bootstrap".to_string(),
            config: serde_json::json!({}),
        }),
        config: serde_json::json!({
            "dataType": { "type": "generic" },
            "intervalMs": 1000
        }),
    };

    let err = create_source(&registry, config).await.err().expect("Expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("Unknown bootstrap kind"),
        "Error: {msg}"
    );
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

    let err = create_source(&registry, config).await.err().expect("Expected error");
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

    let err = create_reaction(&registry, config).await.err().expect("Expected error");
    assert!(err.to_string().contains("Unknown reaction kind"));
}

// ==========================================================================
// Bad config JSON
// ==========================================================================

#[tokio::test]
async fn test_source_with_completely_wrong_config_structure() {
    let registry = populated_registry();
    let config = SourceConfig {
        kind: "mock".to_string(),
        id: "bad-config".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: serde_json::json!("this is a string, not an object"),
    };

    // This may succeed or fail depending on how the mock plugin validates config.
    // The important thing is it doesn't panic.
    let _result = create_source(&registry, config).await;
}

#[tokio::test]
async fn test_reaction_with_null_config() {
    let registry = populated_registry();
    let config = ReactionConfig {
        kind: "log".to_string(),
        id: "null-config".to_string(),
        queries: vec![],
        auto_start: true,
        config: serde_json::json!(null),
    };

    // Log reaction should handle null config gracefully (no required fields)
    let _result = create_reaction(&registry, config).await;
}

#[tokio::test]
async fn test_source_with_extra_unknown_fields_in_config() {
    let registry = populated_registry();
    let config = SourceConfig {
        kind: "mock".to_string(),
        id: "extra-fields".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: serde_json::json!({
            "dataType": { "type": "generic" },
            "intervalMs": 1000,
            "unknownField": "should be ignored",
            "anotherUnknown": 42
        }),
    };

    // Plugins may accept or reject extra fields depending on their deserialization.
    // The key assertion is that this doesn't panic or crash the server.
    let result = create_source(&registry, config).await;
    // If it fails, the error should be descriptive (not a panic)
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(
            !msg.is_empty(),
            "Error message should be descriptive"
        );
    }
}

// ==========================================================================
// Dynamic loading edge cases
// ==========================================================================

#[test]
fn test_dynamic_loading_nonexistent_dir() {
    let mut registry = PluginRegistry::new();
    let (stats, handles) = drasi_server::dynamic_loading::load_plugins_from_directory(
        "/nonexistent/path/to/plugins",
        &mut registry,
    )
    .unwrap();

    assert_eq!(stats.found, 0);
    assert_eq!(stats.loaded, 0);
    assert_eq!(stats.failed, 0);
    assert!(handles.is_empty());
}

#[test]
fn test_dynamic_loading_empty_dir() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut registry = PluginRegistry::new();
    let (stats, handles) = drasi_server::dynamic_loading::load_plugins_from_directory(
        temp_dir.path().to_str().unwrap(),
        &mut registry,
    )
    .unwrap();

    assert_eq!(stats.found, 0);
    assert_eq!(stats.loaded, 0);
    assert!(handles.is_empty());
}

#[test]
fn test_dynamic_loading_invalid_library_file() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Create a fake .so file
    #[cfg(target_os = "linux")]
    let ext = "so";
    #[cfg(target_os = "macos")]
    let ext = "dylib";
    #[cfg(target_os = "windows")]
    let ext = "dll";

    std::fs::write(
        temp_dir.path().join(format!("libdrasi_source_fake.{ext}")),
        b"not a real shared library",
    )
    .unwrap();

    let mut registry = PluginRegistry::new();
    let (stats, _handles) = drasi_server::dynamic_loading::load_plugins_from_directory(
        temp_dir.path().to_str().unwrap(),
        &mut registry,
    )
    .unwrap();

    assert_eq!(stats.found, 1);
    assert_eq!(stats.loaded, 0);
    assert_eq!(stats.failed, 1);
    // The registry should remain empty
    assert!(registry.is_empty());
}

#[test]
fn test_dynamic_loading_skips_non_library_files() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    std::fs::write(temp_dir.path().join("README.md"), "# Not a plugin").unwrap();
    std::fs::write(temp_dir.path().join("config.yaml"), "key: value").unwrap();
    std::fs::write(temp_dir.path().join("data.json"), "{}").unwrap();

    let mut registry = PluginRegistry::new();
    let (stats, _) = drasi_server::dynamic_loading::load_plugins_from_directory(
        temp_dir.path().to_str().unwrap(),
        &mut registry,
    )
    .unwrap();

    assert_eq!(stats.found, 0, "Non-library files should be skipped");
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
    assert!(
        result.is_err(),
        "snake_case auto_start should be rejected"
    );
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
    assert!(
        result.is_err(),
        "snake_case auto_start should be rejected"
    );
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
