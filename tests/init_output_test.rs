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

use drasi_server::api::models::sources::mock::DataTypeDto;
use drasi_server::api::models::*;
use drasi_server::DrasiServerConfig;
use std::collections::HashMap;

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
        sources: vec![SourceConfig::Mock {
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: MockSourceConfigDto {
                data_type: DataTypeDto::SensorReading { sensor_count: 5 },
                interval_ms: ConfigValue::Static(5000),
            },
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
        sources: vec![SourceConfig::Http {
            id: "http-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: HttpSourceConfigDto {
                host: ConfigValue::Static("0.0.0.0".to_string()),
                port: ConfigValue::Static(9000),
                endpoint: None,
                timeout_ms: ConfigValue::Static(10000),
                adaptive_max_batch_size: None,
                adaptive_min_batch_size: None,
                adaptive_max_wait_ms: None,
                adaptive_min_wait_ms: None,
                adaptive_window_secs: None,
                adaptive_enabled: None,
                webhooks: None,
            },
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
        sources: vec![SourceConfig::Grpc {
            id: "grpc-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: GrpcSourceConfigDto {
                host: ConfigValue::Static("0.0.0.0".to_string()),
                port: ConfigValue::Static(50051),
                endpoint: None,
                timeout_ms: ConfigValue::Static(5000),
            },
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
        sources: vec![SourceConfig::Postgres {
            id: "postgres-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::Postgres(
                PostgresBootstrapConfigDto {
                    host: ConfigValue::Static("localhost".to_string()),
                    port: ConfigValue::Static(5432),
                    database: ConfigValue::Static("testdb".to_string()),
                    user: ConfigValue::Static("testuser".to_string()),
                    password: ConfigValue::Static("testpass".to_string()),
                    tables: vec!["users".to_string(), "orders".to_string()],
                    slot_name: "drasi_slot".to_string(),
                    publication_name: "drasi_pub".to_string(),
                    ssl_mode: ConfigValue::Static(SslModeDto::Prefer),
                    table_keys: vec![TableKeyConfigDto {
                        table: "users".to_string(),
                        key_columns: vec!["id".to_string()],
                    }],
                },
            )),
            config: PostgresSourceConfigDto {
                host: ConfigValue::Static("localhost".to_string()),
                port: ConfigValue::Static(5432),
                database: ConfigValue::Static("testdb".to_string()),
                user: ConfigValue::Static("testuser".to_string()),
                password: ConfigValue::Static("testpass".to_string()),
                tables: vec!["users".to_string(), "orders".to_string()],
                slot_name: "drasi_slot".to_string(),
                publication_name: "drasi_pub".to_string(),
                ssl_mode: ConfigValue::Static(SslModeDto::Prefer),
                table_keys: vec![TableKeyConfigDto {
                    table: "users".to_string(),
                    key_columns: vec!["id".to_string()],
                }],
            },
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

#[test]
fn test_platform_source_generates_valid_yaml() {
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
        sources: vec![SourceConfig::Platform {
            id: "platform-source".to_string(),
            auto_start: true,
            bootstrap_provider: None,
            config: PlatformSourceConfigDto {
                redis_url: ConfigValue::Static("redis://localhost:6379".to_string()),
                stream_key: ConfigValue::Static("my-stream".to_string()),
                consumer_group: ConfigValue::Static("drasi-core".to_string()),
                consumer_name: None,
                batch_size: ConfigValue::Static(100),
                block_ms: ConfigValue::Static(5000),
            },
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("kind: platform"),
        "Should contain kind: platform"
    );
    assert!(yaml.contains("redisUrl:"), "Should contain redisUrl");
    assert!(yaml.contains("streamKey:"), "Should contain streamKey");
    assert!(
        yaml.contains("consumerGroup:"),
        "Should contain consumerGroup"
    );
    assert!(yaml.contains("batchSize:"), "Should contain batchSize");
    assert!(yaml.contains("blockMs:"), "Should contain blockMs");

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
        sources: vec![SourceConfig::Mock {
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::Postgres(
                PostgresBootstrapConfigDto {
                    host: ConfigValue::Static("localhost".to_string()),
                    port: ConfigValue::Static(5432),
                    database: ConfigValue::Static("testdb".to_string()),
                    user: ConfigValue::Static("testuser".to_string()),
                    password: ConfigValue::Static("testpass".to_string()),
                    tables: vec!["users".to_string(), "orders".to_string()],
                    slot_name: "drasi_slot".to_string(),
                    publication_name: "drasi_pub".to_string(),
                    ssl_mode: ConfigValue::Static(SslModeDto::Prefer),
                    table_keys: vec![TableKeyConfigDto {
                        table: "users".to_string(),
                        key_columns: vec!["id".to_string()],
                    }],
                },
            )),
            config: MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(5000),
            },
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
        sources: vec![SourceConfig::Mock {
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::ScriptFile(
                ScriptFileBootstrapConfigDto {
                    file_paths: vec!["/data/init.jsonl".to_string()],
                },
            )),
            config: MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(5000),
            },
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
fn test_platform_bootstrap_provider_generates_valid_yaml() {
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
        sources: vec![SourceConfig::Mock {
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::Platform(
                PlatformBootstrapConfigDto {
                    query_api_url: Some("http://query-api:8080".to_string()),
                    timeout_seconds: 300,
                },
            )),
            config: MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(5000),
            },
        }],
        queries: vec![],
        reactions: vec![],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("kind: platform"),
        "Bootstrap provider should use kind: platform"
    );
    assert!(yaml.contains("queryApiUrl:"), "Should contain queryApiUrl");
    assert!(
        yaml.contains("timeoutSeconds:"),
        "Should contain timeoutSeconds"
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
        sources: vec![SourceConfig::Mock {
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::Noop),
            config: MockSourceConfigDto {
                data_type: DataTypeDto::Generic,
                interval_ms: ConfigValue::Static(5000),
            },
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
        reactions: vec![ReactionConfig::Log {
            id: "log-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: LogReactionConfigDto::default(),
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
        reactions: vec![ReactionConfig::Http {
            id: "http-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: HttpReactionConfigDto {
                base_url: ConfigValue::Static("https://api.example.com".to_string()),
                token: Some(ConfigValue::Static("secret-token".to_string())),
                timeout_ms: ConfigValue::Static(5000),
                routes: HashMap::new(),
            },
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
        reactions: vec![ReactionConfig::Sse {
            id: "sse-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: SseReactionConfigDto {
                host: ConfigValue::Static("0.0.0.0".to_string()),
                port: ConfigValue::Static(8081),
                sse_path: ConfigValue::Static("/events".to_string()),
                heartbeat_interval_ms: ConfigValue::Static(30000),
                routes: HashMap::new(),
                default_template: None,
            },
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
        reactions: vec![ReactionConfig::Grpc {
            id: "grpc-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: GrpcReactionConfigDto {
                endpoint: ConfigValue::Static("grpc://localhost:50052".to_string()),
                timeout_ms: ConfigValue::Static(5000),
                batch_size: ConfigValue::Static(100),
                batch_flush_timeout_ms: ConfigValue::Static(1000),
                max_retries: ConfigValue::Static(3),
                connection_retry_attempts: ConfigValue::Static(5),
                initial_connection_timeout_ms: ConfigValue::Static(10000),
                metadata: HashMap::new(),
            },
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

#[test]
fn test_platform_reaction_generates_valid_yaml() {
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
        reactions: vec![ReactionConfig::Platform {
            id: "platform-reaction".to_string(),
            queries: vec!["my-query".to_string()],
            auto_start: true,
            config: PlatformReactionConfigDto {
                redis_url: ConfigValue::Static("redis://localhost:6379".to_string()),
                pubsub_name: Some(ConfigValue::Static("my-pubsub".to_string())),
                source_name: None,
                max_stream_length: Some(ConfigValue::Static(1000)),
                emit_control_events: ConfigValue::Static(false),
                batch_enabled: ConfigValue::Static(true),
                batch_max_size: ConfigValue::Static(100),
                batch_max_wait_ms: ConfigValue::Static(100),
            },
        }],
        instances: vec![],
    };

    let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
    assert_camel_case_fields(&yaml);

    assert!(
        yaml.contains("kind: platform"),
        "Should contain kind: platform"
    );
    assert!(yaml.contains("redisUrl:"), "Should contain redisUrl");
    assert!(yaml.contains("pubsubName:"), "Should contain pubsubName");
    assert!(
        yaml.contains("batchEnabled:"),
        "Should contain batchEnabled"
    );
    assert!(
        yaml.contains("batchMaxSize:"),
        "Should contain batchMaxSize"
    );
    assert!(
        yaml.contains("batchMaxWaitMs:"),
        "Should contain batchMaxWaitMs"
    );

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
            query: ConfigValue::Static("MATCH (n) RETURN n".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            auto_start: true,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            middleware: vec![],
            sources: vec![SourceSubscriptionConfigDto {
                source_id: ConfigValue::Static("test-source".to_string()),
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
        sources: vec![SourceConfig::Mock {
            id: "mock-source".to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig::ScriptFile(
                ScriptFileBootstrapConfigDto {
                    file_paths: vec!["/data/init.jsonl".to_string()],
                },
            )),
            config: MockSourceConfigDto {
                data_type: DataTypeDto::SensorReading { sensor_count: 5 },
                interval_ms: ConfigValue::Static(5000),
            },
        }],
        queries: vec![QueryConfigDto {
            id: "sensor-query".to_string(),
            query: ConfigValue::Static("MATCH (s:Sensor) WHERE s.temp > 100 RETURN s".to_string()),
            query_language: ConfigValue::Static("Cypher".to_string()),
            auto_start: true,
            enable_bootstrap: true,
            bootstrap_buffer_size: 10000,
            middleware: vec![],
            sources: vec![SourceSubscriptionConfigDto {
                source_id: ConfigValue::Static("mock-source".to_string()),
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
        reactions: vec![ReactionConfig::Log {
            id: "log-reaction".to_string(),
            queries: vec!["sensor-query".to_string()],
            auto_start: true,
            config: LogReactionConfigDto::default(),
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
