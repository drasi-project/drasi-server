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

//! Integration tests for config parsing failures.
//!
//! These tests verify that the config loader correctly rejects invalid configurations
//! with appropriate error messages. This ensures that typos and snake_case fields
//! are caught before they can cause silent failures.

use drasi_server::config::load_config_file;
use std::fs;
use tempfile::TempDir;

/// Helper to write YAML to a temp file and attempt to load it
fn try_load_config(yaml: &str) -> Result<(), String> {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("test-config.yaml");
    fs::write(&config_path, yaml).expect("Failed to write config file");

    match load_config_file(&config_path) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Helper to assert that loading fails with a specific field mentioned in error
fn assert_fails_with_field(yaml: &str, expected_field: &str) {
    let result = try_load_config(yaml);
    assert!(
        result.is_err(),
        "Config should fail to load, but it succeeded"
    );
    let err = result.expect_err("Expected error");
    assert!(
        err.contains(expected_field),
        "Error should mention '{expected_field}' but got: {err}"
    );
}

// ==================== Server-level snake_case rejection ====================

#[test]
fn test_load_fails_with_snake_case_log_level() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
log_level: info
sources: []
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "log_level");
}

#[test]
fn test_load_fails_with_snake_case_persist_config() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
persist_config: true
sources: []
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "persist_config");
}

#[test]
fn test_load_fails_with_snake_case_persist_index() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
persist_index: true
sources: []
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "persist_index");
}

#[test]
fn test_load_fails_with_snake_case_state_store() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
state_store:
  kind: redb
  path: ./data/state.redb
sources: []
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "state_store");
}

// ==================== Source snake_case rejection ====================

#[test]
fn test_load_fails_with_source_snake_case_auto_start() {
    let yaml = r#"
id: test-server
sources:
  - kind: mock
    id: test-source
    auto_start: true
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "auto_start");
}

#[test]
fn test_load_fails_with_source_snake_case_bootstrap_provider() {
    let yaml = r#"
id: test-server
sources:
  - kind: postgres
    id: pg-source
    database: testdb
    user: testuser
    bootstrap_provider:
      kind: postgres
      database: testdb
      user: testuser
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "bootstrap_provider");
}

#[test]
fn test_load_fails_with_mock_snake_case_data_type() {
    let yaml = r#"
id: test-server
sources:
  - kind: mock
    id: test-source
    autoStart: true
    data_type: sensor
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "data_type");
}

#[test]
fn test_load_fails_with_mock_snake_case_interval_ms() {
    let yaml = r#"
id: test-server
sources:
  - kind: mock
    id: test-source
    autoStart: true
    interval_ms: 1000
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "interval_ms");
}

#[test]
fn test_load_fails_with_postgres_snake_case_slot_name() {
    let yaml = r#"
id: test-server
sources:
  - kind: postgres
    id: pg-source
    database: testdb
    user: testuser
    slot_name: drasi_slot
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "slot_name");
}

#[test]
fn test_load_fails_with_postgres_snake_case_publication_name() {
    let yaml = r#"
id: test-server
sources:
  - kind: postgres
    id: pg-source
    database: testdb
    user: testuser
    publication_name: drasi_pub
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "publication_name");
}

#[test]
fn test_load_fails_with_postgres_snake_case_ssl_mode() {
    let yaml = r#"
id: test-server
sources:
  - kind: postgres
    id: pg-source
    database: testdb
    user: testuser
    ssl_mode: prefer
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "ssl_mode");
}

#[test]
fn test_load_fails_with_postgres_snake_case_table_keys() {
    let yaml = r#"
id: test-server
sources:
  - kind: postgres
    id: pg-source
    database: testdb
    user: testuser
    table_keys:
      - table: users
        keyColumns: [id]
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "table_keys");
}

#[test]
fn test_load_fails_with_table_key_snake_case_key_columns() {
    let yaml = r#"
id: test-server
sources:
  - kind: postgres
    id: pg-source
    database: testdb
    user: testuser
    tableKeys:
      - table: users
        key_columns: [id]
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "key_columns");
}

#[test]
fn test_load_fails_with_http_source_snake_case_timeout_ms() {
    let yaml = r#"
id: test-server
sources:
  - kind: http
    id: http-source
    host: localhost
    port: 8080
    timeout_ms: 5000
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "timeout_ms");
}

// ==================== Query snake_case rejection ====================

#[test]
fn test_load_fails_with_query_snake_case_auto_start() {
    let yaml = r#"
id: test-server
sources: []
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    auto_start: true
    sources:
      - sourceId: test-source
reactions: []
"#;
    assert_fails_with_field(yaml, "auto_start");
}

#[test]
fn test_load_fails_with_query_snake_case_query_language() {
    let yaml = r#"
id: test-server
sources: []
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    query_language: Cypher
    sources:
      - sourceId: test-source
reactions: []
"#;
    assert_fails_with_field(yaml, "query_language");
}

#[test]
fn test_load_fails_with_query_snake_case_enable_bootstrap() {
    let yaml = r#"
id: test-server
sources: []
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    enable_bootstrap: true
    sources:
      - sourceId: test-source
reactions: []
"#;
    assert_fails_with_field(yaml, "enable_bootstrap");
}

#[test]
fn test_load_fails_with_query_snake_case_bootstrap_buffer_size() {
    let yaml = r#"
id: test-server
sources: []
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    bootstrap_buffer_size: 10000
    sources:
      - sourceId: test-source
reactions: []
"#;
    assert_fails_with_field(yaml, "bootstrap_buffer_size");
}

// ==================== Reaction snake_case rejection ====================

#[test]
fn test_load_fails_with_reaction_snake_case_auto_start() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: log
    id: test-log
    queries: [q1]
    auto_start: true
"#;
    assert_fails_with_field(yaml, "auto_start");
}

#[test]
fn test_load_fails_with_log_reaction_snake_case_default_template() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: log
    id: test-log
    queries: [q1]
    autoStart: true
    default_template:
      added:
        template: "{{after}}"
"#;
    assert_fails_with_field(yaml, "default_template");
}

#[test]
fn test_load_fails_with_http_reaction_snake_case_base_url() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: http
    id: test-http
    queries: [q1]
    autoStart: true
    base_url: "http://localhost"
"#;
    assert_fails_with_field(yaml, "base_url");
}

#[test]
fn test_load_fails_with_http_reaction_snake_case_timeout_ms() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: http
    id: test-http
    queries: [q1]
    autoStart: true
    baseUrl: "http://localhost"
    timeout_ms: 5000
"#;
    assert_fails_with_field(yaml, "timeout_ms");
}

#[test]
fn test_load_fails_with_sse_reaction_snake_case_sse_path() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: sse
    id: test-sse
    queries: [q1]
    autoStart: true
    sse_path: "/events"
"#;
    assert_fails_with_field(yaml, "sse_path");
}

#[test]
fn test_load_fails_with_sse_reaction_snake_case_heartbeat_interval_ms() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: sse
    id: test-sse
    queries: [q1]
    autoStart: true
    heartbeat_interval_ms: 30000
"#;
    assert_fails_with_field(yaml, "heartbeat_interval_ms");
}

#[test]
fn test_load_fails_with_grpc_reaction_snake_case_batch_size() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: grpc
    id: test-grpc
    queries: [q1]
    autoStart: true
    endpoint: "grpc://localhost:50051"
    batch_size: 100
"#;
    assert_fails_with_field(yaml, "batch_size");
}

#[test]
fn test_load_fails_with_profiler_reaction_snake_case_window_size() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: profiler
    id: test-profiler
    queries: [q1]
    autoStart: true
    window_size: 100
"#;
    assert_fails_with_field(yaml, "window_size");
}

// ==================== Unknown field rejection ====================

#[test]
fn test_load_fails_with_unknown_server_field() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
unknownServerField: value
sources: []
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "unknownServerField");
}

#[test]
fn test_load_fails_with_unknown_source_field() {
    let yaml = r#"
id: test-server
sources:
  - kind: mock
    id: test-source
    autoStart: true
    unknownSourceField: value
queries: []
reactions: []
"#;
    assert_fails_with_field(yaml, "unknownSourceField");
}

#[test]
fn test_load_fails_with_unknown_query_field() {
    let yaml = r#"
id: test-server
sources: []
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    unknownQueryField: value
    sources:
      - sourceId: test-source
reactions: []
"#;
    assert_fails_with_field(yaml, "unknownQueryField");
}

#[test]
fn test_load_fails_with_unknown_reaction_field() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: log
    id: test-log
    queries: [q1]
    autoStart: true
    unknownReactionField: value
"#;
    assert_fails_with_field(yaml, "unknownReactionField");
}

#[test]
fn test_load_fails_with_unknown_http_route_field() {
    let yaml = r#"
id: test-server
sources: []
queries: []
reactions:
  - kind: http
    id: test-http
    queries: [q1]
    autoStart: true
    baseUrl: "http://localhost"
    routes:
      q1:
        added:
          url: "/api/events"
          method: "POST"
          unknownRouteField: value
"#;
    assert_fails_with_field(yaml, "unknownRouteField");
}

// ==================== Multiple errors ====================

#[test]
fn test_load_fails_with_multiple_server_snake_case_fields() {
    // With deny_unknown_fields, serde fails on the first unknown field.
    // This is different from the previous manual validation which accumulated errors.
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
log_level: info
persist_config: true
sources: []
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(result.is_err(), "Config should fail with unknown field");
    let err = result.unwrap_err();

    // Serde fails-fast on the first unknown field (order depends on YAML parsing)
    // At least one of the snake_case fields should be mentioned
    assert!(
        err.contains("log_level") || err.contains("persist_config"),
        "Error should mention one of the unknown fields: {err}"
    );
}

#[test]
fn test_load_fails_with_source_snake_case_fields() {
    // Source errors are caught by custom deserializers after validation passes
    // Note: deserializers fail-fast, so only first error is reported
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: mock
    id: test-source
    auto_start: true
    data_type: sensor
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(result.is_err(), "Config should fail with source errors");
    let err = result.unwrap_err();

    // Should mention the source context and unknown field
    // auto_start goes to inner config (not recognized as common field)
    // and gets caught by deny_unknown_fields
    assert!(
        err.contains("test-source"),
        "Error should mention source id: {err}"
    );
}

// ==================== Instance config rejection ====================

#[test]
fn test_load_fails_with_instance_snake_case_persist_index() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
instances:
  - id: instance-1
    persist_index: true
    sources: []
    queries: []
    reactions: []
"#;
    assert_fails_with_field(yaml, "persist_index");
}

#[test]
fn test_load_fails_with_instance_snake_case_state_store() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
instances:
  - id: instance-1
    state_store:
      kind: redb
      path: ./data/state.redb
    sources: []
    queries: []
    reactions: []
"#;
    assert_fails_with_field(yaml, "state_store");
}

// ==================== Valid configs (positive tests) ====================

#[test]
fn test_load_succeeds_with_valid_minimal_config() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources: []
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_ok(),
        "Valid minimal config should load: {result:?}"
    );
}

#[test]
fn test_load_succeeds_with_valid_camelcase_config() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
logLevel: info
persistConfig: true
persistIndex: false
sources:
  - kind: mock
    id: test-source
    autoStart: true
    dataType: sensor
    intervalMs: 1000
queries:
  - id: test-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher
    autoStart: true
    enableBootstrap: true
    bootstrapBufferSize: 10000
    sources:
      - sourceId: test-source
reactions:
  - kind: log
    id: test-log
    queries: [test-query]
    autoStart: true
    defaultTemplate:
      added:
        template: "{{after}}"
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_ok(),
        "Valid camelCase config should load: {result:?}"
    );
}

#[test]
fn test_load_succeeds_with_valid_postgres_source() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: postgres
    id: pg-source
    autoStart: true
    host: localhost
    port: 5432
    database: testdb
    user: testuser
    password: secret
    slotName: drasi_slot
    publicationName: drasi_pub
    sslMode: prefer
    tables: [users, orders]
    tableKeys:
      - table: users
        keyColumns: [id]
      - table: orders
        keyColumns: [order_id]
    bootstrapProvider:
      kind: postgres
      host: localhost
      port: 5432
      database: testdb
      user: testuser
      password: secret
      slotName: drasi_slot
      publicationName: drasi_pub
      sslMode: prefer
      tables: [users, orders]
      tableKeys:
        - table: users
          keyColumns: [id]
        - table: orders
          keyColumns: [order_id]
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_ok(),
        "Valid postgres source config should load: {result:?}"
    );
}

#[test]
fn test_load_succeeds_with_valid_http_reaction() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources: []
queries: []
reactions:
  - kind: http
    id: test-http
    queries: [q1]
    autoStart: true
    baseUrl: "http://localhost:3000"
    timeoutMs: 5000
    routes:
      q1:
        added:
          url: "/api/events"
          method: "POST"
          body: '{"event": {{after}}}'
          headers:
            Content-Type: "application/json"
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_ok(),
        "Valid HTTP reaction config should load: {result:?}"
    );
}

#[test]
fn test_load_succeeds_with_valid_multi_instance_config() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
instances:
  - id: instance-1
    persistIndex: true
    stateStore:
      kind: redb
      path: ./data/instance1.redb
    sources:
      - kind: mock
        id: mock-1
        autoStart: true
    queries:
      - id: query-1
        query: "MATCH (n) RETURN n"
        sources:
          - sourceId: mock-1
    reactions:
      - kind: log
        id: log-1
        queries: [query-1]
        autoStart: true
  - id: instance-2
    persistIndex: false
    sources: []
    queries: []
    reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_ok(),
        "Valid multi-instance config should load: {result:?}"
    );
}

// =============================================================================
// Bootstrap Provider Unknown Field Tests
// =============================================================================

#[test]
fn test_load_fails_with_unknown_field_in_bootstrap_provider_scriptfile() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: mock
    id: test-mock
    bootstrapProvider:
      kind: scriptfile
      filePaths: ["/test.jsonl"]
      unknownField: "should fail"
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_err(),
        "Unknown field in bootstrapProvider should fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unknown field"),
        "Error should mention unknown field: {err}"
    );
}

#[test]
fn test_load_fails_with_unknown_field_in_bootstrap_provider_platform() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: mock
    id: test-mock
    bootstrapProvider:
      kind: platform
      queryApiUrl: "http://localhost:8080"
      typoField: 123
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_err(),
        "Unknown field in bootstrapProvider should fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unknown field"),
        "Error should mention unknown field: {err}"
    );
}

#[test]
fn test_load_fails_with_unknown_field_in_bootstrap_provider_postgres() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: mock
    id: test-mock
    bootstrapProvider:
      kind: postgres
      database: testdb
      user: testuser
      extraField: "should fail"
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_err(),
        "Unknown field in bootstrapProvider should fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unknown field"),
        "Error should mention unknown field: {err}"
    );
}

#[test]
fn test_load_fails_with_unknown_bootstrap_provider_type() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: mock
    id: test-mock
    bootstrapProvider:
      kind: unknown
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_err(),
        "Unknown bootstrap provider kind should fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unknown bootstrap provider kind"),
        "Error should mention unknown kind: {err}"
    );
}

#[test]
fn test_load_succeeds_with_valid_scriptfile_bootstrap() {
    let yaml = r#"
id: test-server
host: 0.0.0.0
port: 8080
sources:
  - kind: mock
    id: test-mock
    bootstrapProvider:
      kind: scriptfile
      filePaths: ["/data/test.jsonl"]
queries: []
reactions: []
"#;
    let result = try_load_config(yaml);
    assert!(
        result.is_ok(),
        "Valid scriptfile bootstrap should load: {result:?}"
    );
}
