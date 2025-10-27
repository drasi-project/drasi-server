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

/// Integration tests for configuration persistence
/// Tests that API mutations are saved to config file
use drasi_server::{DrasiServerConfig, QueryConfig, ReactionConfig, SourceConfig};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_persistence_creates_config_file_on_save() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create initial config
    let config = DrasiServerConfig {
        api: drasi_server::ApiSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
        },
        server: drasi_server::ServerSettings {
            log_level: "info".to_string(),
            disable_persistence: false,
        },
        sources: vec![],
        queries: vec![],
        reactions: vec![],
    };

    // Save config
    config.save_to_file(&config_path).expect("Failed to save config");

    // Verify file exists
    assert!(config_path.exists());

    // Verify content
    let loaded_config = DrasiServerConfig::load_from_file(&config_path)
        .expect("Failed to load config");
    assert_eq!(loaded_config.api.host, "127.0.0.1");
    assert_eq!(loaded_config.api.port, 8080);
    assert!(!loaded_config.server.disable_persistence);
}

#[tokio::test]
async fn test_persistence_disabled_by_flag() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create config with persistence disabled
    let config = DrasiServerConfig {
        api: drasi_server::ApiSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
        },
        server: drasi_server::ServerSettings {
            log_level: "info".to_string(),
            disable_persistence: true, // Disabled
        },
        sources: vec![],
        queries: vec![],
        reactions: vec![],
    };

    // Save config
    config.save_to_file(&config_path).expect("Failed to save config");

    // Load and verify
    let loaded_config = DrasiServerConfig::load_from_file(&config_path)
        .expect("Failed to load config");
    assert!(loaded_config.server.disable_persistence);
}

#[tokio::test]
async fn test_persistence_saves_complete_configuration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create config with all components
    let config = DrasiServerConfig {
        api: drasi_server::ApiSettings {
            host: "0.0.0.0".to_string(),
            port: 9090,
        },
        server: drasi_server::ServerSettings {
            log_level: "debug".to_string(),
            disable_persistence: false,
        },
        sources: vec![
            SourceConfig {
                id: "test-source-1".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: {
                    let mut props = HashMap::new();
                    props.insert("key".to_string(), serde_json::json!("value"));
                    props
                },
                bootstrap_provider: None,
                dispatch_buffer_capacity: None,
            dispatch_mode: None,
            },
            SourceConfig {
                id: "test-source-2".to_string(),
                source_type: "http".to_string(),
                auto_start: false,
                properties: HashMap::new(),
                bootstrap_provider: None,
                dispatch_buffer_capacity: None,
            dispatch_mode: None,
            },
        ],
        queries: vec![
            QueryConfig {
                id: "test-query-1".to_string(),
                query: "MATCH (n) RETURN n".to_string(),
                query_language: drasi_server_core::config::QueryLanguage::default(),
                sources: vec!["test-source-1".to_string()],
                auto_start: true,
                properties: HashMap::new(),
                joins: None,
            },
        ],
        reactions: vec![
            ReactionConfig {
                id: "test-reaction-1".to_string(),
                reaction_type: "log".to_string(),
                queries: vec!["test-query-1".to_string()],
                auto_start: true,
                properties: HashMap::new(),
            },
        ],
    };

    // Save config
    config.save_to_file(&config_path).expect("Failed to save config");

    // Load and verify all components
    let loaded_config = DrasiServerConfig::load_from_file(&config_path)
        .expect("Failed to load config");

    // Verify API settings
    assert_eq!(loaded_config.api.host, "0.0.0.0");
    assert_eq!(loaded_config.api.port, 9090);

    // Verify server settings
    assert_eq!(loaded_config.server.log_level, "debug");
    assert!(!loaded_config.server.disable_persistence);

    // Verify sources
    assert_eq!(loaded_config.sources.len(), 2);
    assert_eq!(loaded_config.sources[0].id, "test-source-1");
    assert_eq!(loaded_config.sources[0].source_type, "mock");
    assert!(loaded_config.sources[0].auto_start);
    assert_eq!(loaded_config.sources[1].id, "test-source-2");
    assert_eq!(loaded_config.sources[1].source_type, "http");
    assert!(!loaded_config.sources[1].auto_start);

    // Verify queries
    assert_eq!(loaded_config.queries.len(), 1);
    assert_eq!(loaded_config.queries[0].id, "test-query-1");
    assert_eq!(loaded_config.queries[0].query, "MATCH (n) RETURN n");
    assert_eq!(loaded_config.queries[0].sources.len(), 1);
    assert_eq!(loaded_config.queries[0].sources[0], "test-source-1");

    // Verify reactions
    assert_eq!(loaded_config.reactions.len(), 1);
    assert_eq!(loaded_config.reactions[0].id, "test-reaction-1");
    assert_eq!(loaded_config.reactions[0].reaction_type, "log");
    assert_eq!(loaded_config.reactions[0].queries.len(), 1);
    assert_eq!(loaded_config.reactions[0].queries[0], "test-query-1");
}

#[tokio::test]
async fn test_persistence_atomic_write() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create initial config
    let initial_config = DrasiServerConfig {
        api: drasi_server::ApiSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
        },
        server: drasi_server::ServerSettings {
            log_level: "info".to_string(),
            disable_persistence: false,
        },
        sources: vec![],
        queries: vec![],
        reactions: vec![],
    };

    // Save initial config
    initial_config.save_to_file(&config_path).expect("Failed to save initial config");

    // Create updated config
    let updated_config = DrasiServerConfig {
        api: drasi_server::ApiSettings {
            host: "0.0.0.0".to_string(),
            port: 9090,
        },
        server: drasi_server::ServerSettings {
            log_level: "debug".to_string(),
            disable_persistence: false,
        },
        sources: vec![
            SourceConfig {
                id: "new-source".to_string(),
                source_type: "mock".to_string(),
                auto_start: true,
                properties: HashMap::new(),
                bootstrap_provider: None,
                dispatch_buffer_capacity: None,
            dispatch_mode: None,
            },
        ],
        queries: vec![],
        reactions: vec![],
    };

    // Save updated config
    updated_config.save_to_file(&config_path).expect("Failed to save updated config");

    // Verify temp file doesn't exist
    let temp_path = config_path.with_extension("tmp");
    assert!(!temp_path.exists(), "Temp file should not exist after atomic write");

    // Load and verify updated config
    let loaded_config = DrasiServerConfig::load_from_file(&config_path)
        .expect("Failed to load updated config");
    assert_eq!(loaded_config.api.port, 9090);
    assert_eq!(loaded_config.server.log_level, "debug");
    assert_eq!(loaded_config.sources.len(), 1);
    assert_eq!(loaded_config.sources[0].id, "new-source");
}

#[tokio::test]
async fn test_persistence_validation_before_save() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");

    // Create invalid config (port = 0)
    let invalid_config = DrasiServerConfig {
        api: drasi_server::ApiSettings {
            host: "127.0.0.1".to_string(),
            port: 0, // Invalid port
        },
        server: drasi_server::ServerSettings {
            log_level: "info".to_string(),
            disable_persistence: false,
        },
        sources: vec![],
        queries: vec![],
        reactions: vec![],
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

    // Write YAML directly
    let yaml_content = r#"
api:
  host: 127.0.0.1
  port: 8080
server:
  log_level: info
  disable_persistence: false
sources:
  - id: test-source
    source_type: mock
    auto_start: true
    properties: {}
queries: []
reactions: []
"#;
    fs::write(&config_path, yaml_content).expect("Failed to write YAML");

    // Load and verify
    let config = DrasiServerConfig::load_from_file(&config_path)
        .expect("Failed to load YAML config");
    assert_eq!(config.api.host, "127.0.0.1");
    assert_eq!(config.sources.len(), 1);
    assert_eq!(config.sources[0].id, "test-source");
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
    let config = DrasiServerConfig::load_from_file(&config_path)
        .expect("Failed to load config");
    assert_eq!(config.api.host, "0.0.0.0"); // Default
    assert_eq!(config.api.port, 8080); // Default
    assert_eq!(config.server.log_level, "info"); // Default
    assert!(!config.server.disable_persistence); // Default false
}
