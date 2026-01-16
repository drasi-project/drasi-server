// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Integration tests for configuration persistence
//! Tests that API mutations are saved to config file

mod test_support;

use drasi_server::models::{
    ConfigValue, LogReactionConfigDto, MockSourceConfigDto, QueryConfigDto,
    SourceSubscriptionConfigDto,
};
use drasi_server::{load_config_file, DrasiServerConfig, ReactionConfig, SourceConfig};
use std::fs;
use tempfile::TempDir;

fn default_mock_config() -> MockSourceConfigDto {
    MockSourceConfigDto {
        data_type: ConfigValue::Static("generic".to_string()),
        interval_ms: ConfigValue::Static(5000),
    }
}

#[tokio::test]
async fn test_persistence_creates_config_file_on_save() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create initial config
    let config = DrasiServerConfig {
        host: ConfigValue::Static("127.0.0.1".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        ..DrasiServerConfig::default()
    };

    // Save config
    config
        .save_to_file(&config_path)
        .expect("Failed to save config");

    // Verify file exists
    assert!(config_path.exists());

    // Verify content
    let loaded_config = load_config_file(&config_path).expect("Failed to load config");
    assert_eq!(
        loaded_config.host,
        ConfigValue::Static("127.0.0.1".to_string())
    );
    assert_eq!(loaded_config.port, ConfigValue::Static(8080));
    assert!(loaded_config.persist_config);
}

#[tokio::test]
async fn test_persistence_disabled_by_flag() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create config with persistence disabled
    let config = DrasiServerConfig {
        host: ConfigValue::Static("127.0.0.1".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: false, // Disabled
        ..DrasiServerConfig::default()
    };

    // Save config
    config
        .save_to_file(&config_path)
        .expect("Failed to save config");

    // Load and verify
    let loaded_config = load_config_file(&config_path).expect("Failed to load config");
    assert!(!loaded_config.persist_config);
}

#[tokio::test]
async fn test_persistence_saves_complete_configuration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create sources using enum variants
    let source1 = SourceConfig::Mock {
        id: "test-source-1".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: default_mock_config(),
    };
    let source2 = SourceConfig::Mock {
        id: "test-source-2".to_string(),
        auto_start: false,
        bootstrap_provider: None,
        config: default_mock_config(),
    };

    // Create query using QueryConfigDto
    let query = QueryConfigDto {
        id: "test-query-1".to_string(),
        auto_start: true,
        query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
        query_language: ConfigValue::Static("Cypher".to_string()),
        middleware: vec![],
        sources: vec![SourceSubscriptionConfigDto {
            source_id: ConfigValue::Static("test-source-1".to_string()),
            nodes: vec![],
            relations: vec![],
            pipeline: vec![],
        }],
        enable_bootstrap: true,
        bootstrap_buffer_size: 10000,
        joins: None,
        priority_queue_capacity: None,
        dispatch_buffer_capacity: None,
        dispatch_mode: None,
        storage_backend: None,
    };

    // Create reaction using enum variant
    let reaction = ReactionConfig::Log {
        id: "test-reaction-1".to_string(),
        queries: vec!["test-query-1".to_string()],
        auto_start: true,
        config: LogReactionConfigDto::default(),
    };

    // Create config with all components
    let config = DrasiServerConfig {
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(9090),
        log_level: ConfigValue::Static("debug".to_string()),
        persist_config: true,
        sources: vec![source1, source2],
        queries: vec![query],
        reactions: vec![reaction],
        ..DrasiServerConfig::default()
    };

    // Save config
    config
        .save_to_file(&config_path)
        .expect("Failed to save config");

    // Load and verify all components
    let loaded_config = load_config_file(&config_path).expect("Failed to load config");

    // Verify server settings
    assert_eq!(
        loaded_config.host,
        ConfigValue::Static("0.0.0.0".to_string())
    );
    assert_eq!(loaded_config.port, ConfigValue::Static(9090));
    assert_eq!(
        loaded_config.log_level,
        ConfigValue::Static("debug".to_string())
    );
    assert!(loaded_config.persist_config);

    // Verify sources
    assert_eq!(loaded_config.sources.len(), 2);
    assert_eq!(loaded_config.sources[0].id(), "test-source-1");
    assert!(loaded_config.sources[0].auto_start());
    assert_eq!(loaded_config.sources[1].id(), "test-source-2");
    assert!(!loaded_config.sources[1].auto_start());

    // Verify queries
    assert_eq!(loaded_config.queries.len(), 1);
    assert_eq!(loaded_config.queries[0].id, "test-query-1");
    assert_eq!(
        loaded_config.queries[0].query,
        ConfigValue::Static("MATCH (n) RETURN n".to_string())
    );
    assert_eq!(loaded_config.queries[0].sources.len(), 1);
    assert_eq!(
        loaded_config.queries[0].sources[0].source_id,
        ConfigValue::Static("test-source-1".to_string())
    );

    // Verify reactions
    assert_eq!(loaded_config.reactions.len(), 1);
    assert_eq!(loaded_config.reactions[0].id(), "test-reaction-1");
    assert_eq!(loaded_config.reactions[0].queries().len(), 1);
    assert_eq!(loaded_config.reactions[0].queries()[0], "test-query-1");
}

#[tokio::test]
async fn test_persistence_atomic_write() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create initial config
    let initial_config = DrasiServerConfig {
        host: ConfigValue::Static("127.0.0.1".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        ..DrasiServerConfig::default()
    };

    // Save initial config
    initial_config
        .save_to_file(&config_path)
        .expect("Failed to save initial config");

    // Create updated config with a new source
    let new_source = SourceConfig::Mock {
        id: "new-source".to_string(),
        auto_start: true,
        bootstrap_provider: None,
        config: default_mock_config(),
    };

    let updated_config = DrasiServerConfig {
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(9090),
        log_level: ConfigValue::Static("debug".to_string()),
        persist_config: true,
        sources: vec![new_source],
        ..DrasiServerConfig::default()
    };

    // Save updated config
    updated_config
        .save_to_file(&config_path)
        .expect("Failed to save updated config");

    // Verify temp file doesn't exist (atomic write completed)
    let temp_path = config_path.with_extension("tmp");
    assert!(
        !temp_path.exists(),
        "Temp file should not exist after atomic write"
    );

    // Load and verify updated config
    let loaded_config = load_config_file(&config_path).expect("Failed to load updated config");
    assert_eq!(loaded_config.port, ConfigValue::Static(9090));
    assert_eq!(
        loaded_config.log_level,
        ConfigValue::Static("debug".to_string())
    );
    assert_eq!(loaded_config.sources.len(), 1);
    assert_eq!(loaded_config.sources[0].id(), "new-source");
}

#[tokio::test]
async fn test_persistence_validation_before_save() {
    // Create invalid config (port = 0)
    let invalid_config = DrasiServerConfig {
        host: ConfigValue::Static("127.0.0.1".to_string()),
        port: ConfigValue::Static(0), // Invalid port
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        ..DrasiServerConfig::default()
    };

    // Validation should fail
    let result = invalid_config.validate();
    assert!(result.is_err(), "Validation should fail for port 0");
    assert!(result.unwrap_err().to_string().contains("port"));
}

#[test]
fn test_config_load_yaml_format() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Write YAML directly (flat structure)
    let yaml_content = r#"
host: 127.0.0.1
port: 8080
logLevel: info
persistConfig: true
sources:
  - kind: mock
    id: test-source
    autoStart: true
queries: []
reactions: []
"#;
    fs::write(&config_path, yaml_content).expect("Failed to write YAML");

    // Load and verify
    let config = load_config_file(&config_path).expect("Failed to load YAML config");
    assert_eq!(config.host, ConfigValue::Static("127.0.0.1".to_string()));
    assert_eq!(config.sources.len(), 1);
    assert_eq!(config.sources[0].id(), "test-source");
}

#[test]
fn test_config_default_values() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Write minimal YAML (using defaults)
    let yaml_content = r#"
sources: []
queries: []
reactions: []
"#;
    fs::write(&config_path, yaml_content).expect("Failed to write YAML");

    // Load and verify defaults are applied
    let config = load_config_file(&config_path).expect("Failed to load config");
    assert_eq!(config.host, ConfigValue::Static("0.0.0.0".to_string())); // Default
    assert_eq!(config.port, ConfigValue::Static(8080)); // Default
    assert_eq!(config.log_level, ConfigValue::Static("info".to_string())); // Default
    assert!(config.persist_config); // Default true
}
