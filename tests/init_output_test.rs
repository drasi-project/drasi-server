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

//! Integration tests for verifying that the init command generates valid configurations.
//!
//! These tests build configurations programmatically (simulating what the init command does)
//! and verify that:
//! - Generated YAML is valid and can be parsed back
//! - All source types produce valid configurations
//! - All reaction types produce valid configurations
//! - All bootstrap provider types produce valid configurations
//! - Generated configs use camelCase field names

use drasi_lib::config::QueryLanguage;
use drasi_server::api::models::*;
use drasi_server::DrasiServerConfig;
use serde_json::json;

/// Helper to strip YAML comments and parse config
fn parse_yaml_config(yaml: &str) -> Result<DrasiServerConfig, serde_yaml::Error> {
    let yaml_content: String = yaml
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    serde_yaml::from_str(&yaml_content)
}

/// Helper to verify YAML contains camelCase and not snake_case for specific fields
fn assert_camel_case_fields(yaml: &str) {
    // Fields that must be camelCase
    let camel_case_fields = [
        "logLevel",
        "persistConfig",
        "persistIndex",
        "stateStore",
        "autoStart",
        "bootstrapProvider",
        "dataType",
        "intervalMs",
        "timeoutMs",
        "slotName",
        "publicationName",
        "tableKeys",
        "keyColumns",
        "sslMode",
        "redisUrl",
        "streamKey",
        "consumerGroup",
        "batchSize",
        "blockMs",
        "filePaths",
        "queryApiUrl",
        "timeoutSeconds",
        "queryLanguage",
        "enableBootstrap",
        "bootstrapBufferSize",
        "sourceId",
        "baseUrl",
        "ssePath",
        "heartbeatIntervalMs",
    ];

    // Corresponding snake_case versions that must NOT appear
    let snake_case_fields = [
        "log_level",
        "persist_config",
        "persist_index",
        "state_store",
        "auto_start",
        "bootstrap_provider",
        "data_type",
        "interval_ms",
        "timeout_ms",
        "slot_name",
        "publication_name",
        "table_keys",
        "key_columns",
        "ssl_mode",
        "redis_url",
        "stream_key",
        "consumer_group",
        "batch_size",
        "block_ms",
        "file_paths",
        "query_api_url",
        "timeout_seconds",
        "query_language",
        "enable_bootstrap",
        "bootstrap_buffer_size",
        "source_id",
        "base_url",
        "sse_path",
        "heartbeat_interval_ms",
    ];

    for snake_field in &snake_case_fields {
        // Check for field: pattern (YAML key)
        let pattern = format!("{snake_field}:");
        assert!(
            !yaml.contains(&pattern),
            "YAML should not contain snake_case field '{snake_field}'. Found in:\n{yaml}"
        );
    }

    // Just log which camelCase fields are present (for debugging)
    for camel_field in &camel_case_fields {
        if yaml.contains(&format!("{camel_field}:")) {
            // Field is present and correctly named - good
        }
    }
}

// =============================================================================
// Basic Config Generation Tests
// =============================================================================

#[test]
fn test_empty_config_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.host, config.host);
    assert_eq!(parsed.port, config.port);
}

#[test]
fn test_config_with_state_store_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: true,
        state_store: Some(StateStoreConfig::Redb {
            path: ConfigValue::Static("./data/state.redb".to_string()),
        }),
        default_priority_queue_capacity: Some(ConfigValue::Static(5000)),
        default_dispatch_buffer_capacity: Some(ConfigValue::Static(500)),
        sources: vec![],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    // Verify state store fields are present
    assert!(yaml.contains("stateStore:"), "Should contain stateStore");
    assert!(yaml.contains("kind: redb"), "Should contain kind: redb");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert!(parsed.state_store.is_some());
}

// =============================================================================
// Source Configuration Tests
// =============================================================================

#[test]
fn test_mock_source_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "mock".to_string(),
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: json!({"dataType": {"type": "sensorReading", "sensorCount": 5}, "intervalMs": 5000}),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: mock"), "Should contain kind: mock");
    assert!(yaml.contains("dataType:"), "Should contain dataType");
    assert!(yaml.contains("intervalMs:"), "Should contain intervalMs");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

#[test]
fn test_http_source_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "http".to_string(),
            id: "http-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: json!({"host": "0.0.0.0", "port": 9000, "timeoutMs": 10000}),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: http"), "Should contain kind: http");
    assert!(yaml.contains("timeoutMs:"), "Should contain timeoutMs");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

#[test]
fn test_grpc_source_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "grpc".to_string(),
            id: "grpc-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: json!({"host": "0.0.0.0", "port": 50051, "timeoutMs": 5000}),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: grpc"), "Should contain kind: grpc");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

#[test]
fn test_postgres_source_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "postgres".to_string(),
            id: "postgres-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig {
                kind: "postgres".to_string(),
                config: serde_json::json!({
                    "host": "localhost",
                    "port": 5432,
                    "database": "testdb",
                    "user": "testuser",
                    "password": "testpass",
                    "tables": ["users", "orders"],
                    "slotName": "drasi_slot",
                    "publicationName": "drasi_pub",
                    "sslMode": "prefer",
                    "tableKeys": [{"table": "users", "keyColumns": ["id"]}]
                }),
            }),
            config: json!({
                "host": "localhost",
                "port": 5432,
                "database": "testdb",
                "user": "testuser",
                "password": "testpass",
                "tables": ["users", "orders"],
                "slotName": "drasi_slot",
                "publicationName": "drasi_pub",
                "sslMode": "prefer",
                "tableKeys": [{"table": "users", "keyColumns": ["id"]}]
            }),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("kind: postgres"),
        "Should contain kind: postgres"
    );
    assert!(
        yaml.contains("bootstrapProvider:"),
        "Should contain bootstrapProvider"
    );
    assert!(yaml.contains("database:"), "Should contain database");
    assert!(yaml.contains("user:"), "Should contain user");
    assert!(yaml.contains("slotName:"), "Should contain slotName");
    assert!(
        yaml.contains("publicationName:"),
        "Should contain publicationName"
    );
    assert!(yaml.contains("tableKeys:"), "Should contain tableKeys");
    assert!(yaml.contains("keyColumns:"), "Should contain keyColumns");
    assert!(yaml.contains("sslMode:"), "Should contain sslMode");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

// =============================================================================
// Bootstrap Provider Tests
// =============================================================================

#[test]
fn test_postgres_bootstrap_provider_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "mock".to_string(),
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig {
                kind: "postgres".to_string(),
                config: serde_json::json!({
                    "host": "localhost",
                    "port": 5432,
                    "database": "testdb",
                    "user": "testuser",
                    "password": "testpass",
                    "tables": ["users", "orders"],
                    "slotName": "drasi_slot",
                    "publicationName": "drasi_pub",
                    "sslMode": "prefer",
                    "tableKeys": [{"table": "users", "keyColumns": ["id"]}]
                }),
            }),
            config: json!({"dataType": {"type": "generic"}, "intervalMs": 5000}),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("bootstrapProvider:"),
        "Should contain bootstrapProvider"
    );
    assert!(
        yaml.contains("kind: postgres"),
        "Bootstrap provider should use kind: postgres"
    );
    assert!(yaml.contains("database:"), "Should contain database");
    assert!(yaml.contains("user:"), "Should contain user");
    // Should NOT contain "type: postgres"
    assert!(
        !yaml.contains("type: postgres"),
        "Should NOT contain 'type: postgres'"
    );

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

#[test]
fn test_scriptfile_bootstrap_provider_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "mock".to_string(),
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig {
                kind: "scriptfile".to_string(),
                config: serde_json::json!({
                    "filePaths": ["/data/init.jsonl"]
                }),
            }),
            config: json!({"dataType": {"type": "generic"}, "intervalMs": 5000}),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("kind: scriptfile"),
        "Bootstrap provider should use kind: scriptfile"
    );
    assert!(yaml.contains("filePaths:"), "Should contain filePaths");
    assert!(
        !yaml.contains("file_paths:"),
        "Should NOT contain file_paths"
    );

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

#[test]
fn test_noop_bootstrap_provider_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![SourceConfig {
            kind: "mock".to_string(),
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig {
                kind: "noop".to_string(),
                config: serde_json::json!({}),
            }),
            config: json!({"dataType": {"type": "generic"}, "intervalMs": 5000}),
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("kind: noop"),
        "Bootstrap provider should use kind: noop"
    );

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.sources.len(), 1);
}

// =============================================================================
// Reaction Configuration Tests
// =============================================================================

#[test]
fn test_log_reaction_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![],
        queries: vec![],
        reactions: vec![ReactionConfig {
            kind: "log".to_string(),
            id: "log-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: json!({"routes": {}}),
        }],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: log"), "Should contain kind: log");
    assert!(yaml.contains("autoStart:"), "Should contain autoStart");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.reactions.len(), 1);
}

#[test]
fn test_http_reaction_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![],
        queries: vec![],
        reactions: vec![ReactionConfig {
            kind: "http".to_string(),
            id: "http-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: json!({"baseUrl": "https://api.example.com", "token": "secret-token", "timeoutMs": 5000, "routes": {}}),
        }],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: http"), "Should contain kind: http");
    assert!(yaml.contains("baseUrl:"), "Should contain baseUrl");
    assert!(yaml.contains("timeoutMs:"), "Should contain timeoutMs");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.reactions.len(), 1);
}

#[test]
fn test_sse_reaction_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![],
        queries: vec![],
        reactions: vec![ReactionConfig {
            kind: "sse".to_string(),
            id: "sse-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: json!({"host": "0.0.0.0", "port": 8081, "ssePath": "/events", "heartbeatIntervalMs": 30000, "routes": {}}),
        }],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: sse"), "Should contain kind: sse");
    assert!(yaml.contains("ssePath:"), "Should contain ssePath");
    assert!(
        yaml.contains("heartbeatIntervalMs:"),
        "Should contain heartbeatIntervalMs"
    );

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.reactions.len(), 1);
}

#[test]
fn test_grpc_reaction_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![],
        queries: vec![],
        reactions: vec![ReactionConfig {
            kind: "grpc".to_string(),
            id: "grpc-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: json!({"endpoint": "grpc://localhost:50052", "timeoutMs": 5000, "batchSize": 100, "batchFlushTimeoutMs": 1000, "maxRetries": 3, "connectionRetryAttempts": 5, "initialConnectionTimeoutMs": 10000, "metadata": {}}),
        }],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(yaml.contains("kind: grpc"), "Should contain kind: grpc");
    assert!(yaml.contains("batchSize:"), "Should contain batchSize");
    assert!(
        yaml.contains("batchFlushTimeoutMs:"),
        "Should contain batchFlushTimeoutMs"
    );
    assert!(yaml.contains("maxRetries:"), "Should contain maxRetries");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.reactions.len(), 1);
}

// =============================================================================
// Query Configuration Tests
// =============================================================================

#[test]
fn test_query_generates_valid_yaml() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: false,
        state_store: None,
        default_priority_queue_capacity: None,
        default_dispatch_buffer_capacity: None,
        sources: vec![],
        queries: vec![QueryConfigDto {
            id: "my-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            query_language: QueryLanguage::Cypher,
            auto_start: true,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            middleware: vec![],
            sources: vec![SourceSubscriptionConfigDto {
                source_id: "test-source".to_string(),
                nodes: vec!["Node".to_string()],
                relations: vec!["REL".to_string()],
                pipeline: vec![],
            }],
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        }],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("queryLanguage:"),
        "Should contain queryLanguage"
    );
    assert!(
        yaml.contains("enableBootstrap:"),
        "Should contain enableBootstrap"
    );
    assert!(
        yaml.contains("bootstrapBufferSize:"),
        "Should contain bootstrapBufferSize"
    );
    assert!(yaml.contains("sourceId:"), "Should contain sourceId");

    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");
    assert_eq!(parsed.queries.len(), 1);
}

// =============================================================================
// Full Config Roundtrip Tests
// =============================================================================

#[test]
fn test_full_config_roundtrip() {
    let config = DrasiServerConfig {
        api_version: None,
        id: ConfigValue::Static("full-test-server".to_string()),
        host: ConfigValue::Static("0.0.0.0".to_string()),
        port: ConfigValue::Static(8080),
        log_level: ConfigValue::Static("info".to_string()),
        persist_config: true,
        persist_index: true,
        state_store: Some(StateStoreConfig::Redb {
            path: ConfigValue::Static("./data/state.redb".to_string()),
        }),
        default_priority_queue_capacity: Some(ConfigValue::Static(5000)),
        default_dispatch_buffer_capacity: Some(ConfigValue::Static(500)),
        sources: vec![SourceConfig {
            kind: "mock".to_string(),
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig {
                kind: "scriptfile".to_string(),
                config: serde_json::json!({
                    "filePaths": ["/data/init.jsonl"]
                }),
            }),
            config: json!({"dataType": {"type": "sensorReading", "sensorCount": 5}, "intervalMs": 5000}),
        }],
        queries: vec![QueryConfigDto {
            id: "sensor-query".to_string(),
            query: "MATCH (s:Sensor) WHERE s.temp > 100 RETURN s".to_string(),
            query_language: QueryLanguage::Cypher,
            auto_start: true,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            middleware: vec![],
            sources: vec![SourceSubscriptionConfigDto {
                source_id: "mock-source".to_string(),
                nodes: vec![],
                relations: vec![],
                pipeline: vec![],
            }],
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        }],
        reactions: vec![ReactionConfig {
            kind: "log".to_string(),
            id: "log-reaction".to_string(),
            queries: vec!["sensor-query".to_string()],
            auto_start: true,
            config: json!({"routes": {}}),
        }],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    // Parse back
    let parsed = parse_yaml_config(&yaml).expect("Should parse back to config");

    // Verify key fields match
    assert_eq!(parsed.id, config.id);
    assert_eq!(parsed.host, config.host);
    assert_eq!(parsed.port, config.port);
    assert_eq!(parsed.log_level, config.log_level);
    assert_eq!(parsed.persist_config, config.persist_config);
    assert_eq!(parsed.persist_index, config.persist_index);
    assert!(parsed.state_store.is_some());
    assert_eq!(parsed.sources.len(), 1);
    assert_eq!(parsed.queries.len(), 1);
    assert_eq!(parsed.reactions.len(), 1);
}
