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

//! Configuration field validation.
//!
//! This module provides validation of configuration fields to catch typos and
//! unknown fields that would otherwise be silently ignored due to serde defaults.
//!
//! The validation uses a two-pass approach:
//! 1. Parse YAML to `serde_yaml::Value` to get raw field names
//! 2. Validate field names against known schemas per `kind`
//! 3. Report unknown fields before typed deserialization

use std::collections::HashSet;

/// Validation error for unknown configuration fields.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Unknown field '{field}' in {context}. Valid fields are: {valid_fields}")]
    UnknownField {
        field: String,
        context: String,
        valid_fields: String,
    },

    #[error("Multiple validation errors:\n{}", .0.join("\n"))]
    Multiple(Vec<String>),
}

/// Known fields for server-level configuration.
const SERVER_FIELDS: &[&str] = &[
    "id",
    "host",
    "port",
    "logLevel",
    "persistConfig",
    "persistIndex",
    "stateStore",
    "defaultPriorityQueueCapacity",
    "defaultDispatchBufferCapacity",
    "sources",
    "queries",
    "reactions",
    "instances",
];

/// Known fields for instance configuration.
const INSTANCE_FIELDS: &[&str] = &[
    "id",
    "persistIndex",
    "stateStore",
    "defaultPriorityQueueCapacity",
    "defaultDispatchBufferCapacity",
    "sources",
    "queries",
    "reactions",
];

/// Known fields for query configuration.
const QUERY_FIELDS: &[&str] = &[
    "id",
    "query",
    "queryLanguage",
    "sources",
    "autoStart",
    "joins",
    "enableBootstrap",
    "bootstrapBufferSize",
    "priorityQueueCapacity",
    "dispatchBufferCapacity",
    "middleware",
    "dispatchMode",
    "storageBackend",
];

/// Common fields for all source kinds.
const SOURCE_COMMON_FIELDS: &[&str] = &["kind", "id", "autoStart", "bootstrapProvider"];

/// Common fields for all reaction kinds.
const REACTION_COMMON_FIELDS: &[&str] = &["kind", "id", "queries", "autoStart"];

/// Source-specific fields by kind.
fn source_fields(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "mock" => Some(&["dataType", "intervalMs"]),
        "http" => Some(&[
            "host",
            "port",
            "endpoint",
            "timeoutMs",
            "adaptiveMaxBatchSize",
            "adaptiveMinBatchSize",
            "adaptiveMaxWaitMs",
            "adaptiveMinWaitMs",
            "adaptiveWindowSecs",
            "adaptiveEnabled",
        ]),
        "grpc" => Some(&["host", "port", "endpoint", "timeoutMs"]),
        "postgres" => Some(&[
            "host",
            "port",
            "database",
            "user",
            "password",
            "tables",
            "slotName",
            "publicationName",
            "sslMode",
            "tableKeys",
        ]),
        "platform" => Some(&[
            "redisUrl",
            "streamKey",
            "consumerGroup",
            "consumerName",
            "batchSize",
            "blockMs",
        ]),
        _ => None,
    }
}

/// Reaction-specific fields by kind.
fn reaction_fields(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "log" => Some(&["routes", "defaultTemplate"]),
        "http" => Some(&["baseUrl", "token", "timeoutMs", "routes"]),
        "http-adaptive" => Some(&[
            "baseUrl",
            "token",
            "timeoutMs",
            "routes",
            "adaptiveMinBatchSize",
            "adaptiveMaxBatchSize",
            "adaptiveWindowSize",
            "adaptiveBatchTimeoutMs",
        ]),
        "grpc" => Some(&[
            "endpoint",
            "timeoutMs",
            "batchSize",
            "batchFlushTimeoutMs",
            "maxRetries",
            "connectionRetryAttempts",
            "initialConnectionTimeoutMs",
            "metadata",
        ]),
        "grpc-adaptive" => Some(&[
            "endpoint",
            "timeoutMs",
            "maxRetries",
            "connectionRetryAttempts",
            "initialConnectionTimeoutMs",
            "metadata",
            "adaptiveMinBatchSize",
            "adaptiveMaxBatchSize",
            "adaptiveWindowSize",
            "adaptiveBatchTimeoutMs",
        ]),
        "sse" => Some(&[
            "host",
            "port",
            "ssePath",
            "heartbeatIntervalMs",
            "routes",
            "defaultTemplate",
        ]),
        "platform" => Some(&["redisUrl", "streamKeyPrefix"]),
        "profiler" => Some(&["windowSize", "reportIntervalSecs"]),
        _ => None,
    }
}

/// Known fields for HTTP route (per-query) configuration.
const HTTP_ROUTE_FIELDS: &[&str] = &["added", "updated", "deleted"];

/// Known fields for HTTP call spec configuration.
const HTTP_CALL_SPEC_FIELDS: &[&str] = &["url", "method", "body", "headers"];

/// Known fields for log/SSE template spec.
const TEMPLATE_SPEC_FIELDS: &[&str] = &["template", "path"];

/// Known fields for log/SSE query config.
const TEMPLATE_QUERY_FIELDS: &[&str] = &["added", "updated", "deleted"];

/// Known fields for table key configuration.
const TABLE_KEY_FIELDS: &[&str] = &["table", "keyColumns"];

/// Known fields for state store configuration.
const STATE_STORE_REDB_FIELDS: &[&str] = &["kind", "path"];

/// Known fields for bootstrap provider configuration.
/// Note: `type` is supported as an alias for `kind` in bootstrap provider configs.
const BOOTSTRAP_PROVIDER_FIELDS: &[&str] = &["kind", "type", "path", "sourceId"];

/// Validate a configuration value and return all unknown field errors.
pub fn validate_config(value: &serde_yaml::Value) -> Result<(), ValidationError> {
    let mut errors = Vec::new();

    if let Some(map) = value.as_mapping() {
        // Validate server-level fields
        validate_fields(map, SERVER_FIELDS, "server configuration", &mut errors);

        // Validate sources
        if let Some(sources) = map.get("sources") {
            validate_sources(sources, &mut errors);
        }

        // Validate queries
        if let Some(queries) = map.get("queries") {
            validate_queries(queries, &mut errors);
        }

        // Validate reactions
        if let Some(reactions) = map.get("reactions") {
            validate_reactions(reactions, &mut errors);
        }

        // Validate instances
        if let Some(instances) = map.get("instances") {
            validate_instances(instances, &mut errors);
        }

        // Validate state_store
        if let Some(state_store) = map.get("stateStore") {
            validate_state_store(state_store, "server stateStore", &mut errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else if errors.len() == 1 {
        Err(ValidationError::UnknownField {
            field: errors[0].clone(),
            context: String::new(),
            valid_fields: String::new(),
        })
    } else {
        Err(ValidationError::Multiple(errors))
    }
}

fn validate_fields(
    map: &serde_yaml::Mapping,
    valid_fields: &[&str],
    context: &str,
    errors: &mut Vec<String>,
) {
    let valid_set: HashSet<&str> = valid_fields.iter().copied().collect();

    for key in map.keys() {
        if let Some(key_str) = key.as_str() {
            if !valid_set.contains(key_str) {
                errors.push(format!(
                    "Unknown field '{key_str}' in {context}. Valid fields: {valid_fields:?}"
                ));
            }
        }
    }
}

fn validate_sources(sources: &serde_yaml::Value, errors: &mut Vec<String>) {
    if let Some(arr) = sources.as_sequence() {
        for (i, source) in arr.iter().enumerate() {
            if let Some(map) = source.as_mapping() {
                let kind = map
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let id = map.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let context = format!("source[{i}] (kind={kind}, id={id})");

                // Build valid fields: common + kind-specific
                let mut valid_fields: Vec<&str> = SOURCE_COMMON_FIELDS.to_vec();
                if let Some(kind_fields) = source_fields(kind) {
                    valid_fields.extend_from_slice(kind_fields);
                }

                validate_fields(map, &valid_fields, &context, errors);

                // Validate bootstrapProvider if present
                if let Some(bp) = map.get("bootstrapProvider") {
                    validate_bootstrap_provider(bp, &format!("{context} bootstrapProvider"), errors);
                }

                // Validate tableKeys for postgres
                if kind == "postgres" {
                    if let Some(table_keys) = map.get("tableKeys") {
                        validate_table_keys(table_keys, &context, errors);
                    }
                }
            }
        }
    }
}

fn validate_queries(queries: &serde_yaml::Value, errors: &mut Vec<String>) {
    if let Some(arr) = queries.as_sequence() {
        for (i, query) in arr.iter().enumerate() {
            if let Some(map) = query.as_mapping() {
                let id = map.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let context = format!("query[{i}] (id={id})");

                validate_fields(map, QUERY_FIELDS, &context, errors);
            }
        }
    }
}

fn validate_reactions(reactions: &serde_yaml::Value, errors: &mut Vec<String>) {
    if let Some(arr) = reactions.as_sequence() {
        for (i, reaction) in arr.iter().enumerate() {
            if let Some(map) = reaction.as_mapping() {
                let kind = map
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let id = map.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let context = format!("reaction[{i}] (kind={kind}, id={id})");

                // Build valid fields: common + kind-specific
                let mut valid_fields: Vec<&str> = REACTION_COMMON_FIELDS.to_vec();
                if let Some(kind_fields) = reaction_fields(kind) {
                    valid_fields.extend_from_slice(kind_fields);
                }

                validate_fields(map, &valid_fields, &context, errors);

                // Validate nested routes for http/http-adaptive/sse/log
                if matches!(kind, "http" | "http-adaptive") {
                    if let Some(routes) = map.get("routes") {
                        validate_http_routes(routes, &context, errors);
                    }
                }

                if matches!(kind, "log" | "sse") {
                    if let Some(routes) = map.get("routes") {
                        validate_template_routes(routes, &context, errors);
                    }
                    if let Some(dt) = map.get("defaultTemplate") {
                        validate_template_query_config(dt, &format!("{context} defaultTemplate"), errors);
                    }
                }
            }
        }
    }
}

fn validate_instances(instances: &serde_yaml::Value, errors: &mut Vec<String>) {
    if let Some(arr) = instances.as_sequence() {
        for (i, instance) in arr.iter().enumerate() {
            if let Some(map) = instance.as_mapping() {
                let id = map.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let context = format!("instance[{i}] (id={id})");

                validate_fields(map, INSTANCE_FIELDS, &context, errors);

                // Validate nested sources/queries/reactions
                if let Some(sources) = map.get("sources") {
                    validate_sources(sources, errors);
                }
                if let Some(queries) = map.get("queries") {
                    validate_queries(queries, errors);
                }
                if let Some(reactions) = map.get("reactions") {
                    validate_reactions(reactions, errors);
                }
                if let Some(state_store) = map.get("stateStore") {
                    validate_state_store(state_store, &format!("{context} stateStore"), errors);
                }
            }
        }
    }
}

fn validate_http_routes(routes: &serde_yaml::Value, parent_context: &str, errors: &mut Vec<String>) {
    if let Some(map) = routes.as_mapping() {
        for (key, route) in map {
            let route_name = key.as_str().unwrap_or("unknown");
            let context = format!("{parent_context} routes.{route_name}");

            if let Some(route_map) = route.as_mapping() {
                validate_fields(route_map, HTTP_ROUTE_FIELDS, &context, errors);

                // Validate each call spec (added/updated/deleted)
                for field in ["added", "updated", "deleted"] {
                    if let Some(call_spec) = route_map.get(field) {
                        if let Some(spec_map) = call_spec.as_mapping() {
                            validate_fields(spec_map, HTTP_CALL_SPEC_FIELDS, &format!("{context}.{field}"), errors);
                        }
                    }
                }
            }
        }
    }
}

fn validate_template_routes(routes: &serde_yaml::Value, parent_context: &str, errors: &mut Vec<String>) {
    if let Some(map) = routes.as_mapping() {
        for (key, route) in map {
            let route_name = key.as_str().unwrap_or("unknown");
            let context = format!("{parent_context} routes.{route_name}");

            if let Some(route_map) = route.as_mapping() {
                validate_template_query_config_inner(route_map, &context, errors);
            }
        }
    }
}

fn validate_template_query_config(value: &serde_yaml::Value, context: &str, errors: &mut Vec<String>) {
    if let Some(map) = value.as_mapping() {
        validate_template_query_config_inner(map, context, errors);
    }
}

fn validate_template_query_config_inner(map: &serde_yaml::Mapping, context: &str, errors: &mut Vec<String>) {
    validate_fields(map, TEMPLATE_QUERY_FIELDS, context, errors);

    // Validate each template spec (added/updated/deleted)
    for field in ["added", "updated", "deleted"] {
        if let Some(spec) = map.get(field) {
            if let Some(spec_map) = spec.as_mapping() {
                validate_fields(spec_map, TEMPLATE_SPEC_FIELDS, &format!("{context}.{field}"), errors);
            }
        }
    }
}

fn validate_table_keys(table_keys: &serde_yaml::Value, parent_context: &str, errors: &mut Vec<String>) {
    if let Some(arr) = table_keys.as_sequence() {
        for (i, tk) in arr.iter().enumerate() {
            if let Some(map) = tk.as_mapping() {
                let context = format!("{parent_context} tableKeys[{i}]");
                validate_fields(map, TABLE_KEY_FIELDS, &context, errors);
            }
        }
    }
}

fn validate_state_store(value: &serde_yaml::Value, context: &str, errors: &mut Vec<String>) {
    if let Some(map) = value.as_mapping() {
        let kind = map.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown");
        match kind {
            "redb" => validate_fields(map, STATE_STORE_REDB_FIELDS, context, errors),
            _ => errors.push(format!("Unknown state store kind '{kind}' in {context}")),
        }
    }
}

fn validate_bootstrap_provider(value: &serde_yaml::Value, context: &str, errors: &mut Vec<String>) {
    if let Some(map) = value.as_mapping() {
        validate_fields(map, BOOTSTRAP_PROVIDER_FIELDS, context, errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config_passes() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            logLevel: info
            sources:
              - kind: mock
                id: test-source
                autoStart: true
                dataType: sensor
                intervalMs: 1000
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                sources:
                  - sourceId: test-source
            reactions:
              - kind: log
                id: test-log
                queries:
                  - test-query
                autoStart: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid config should pass: {result:?}");
    }

    #[test]
    fn test_snake_case_field_detected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            sources:
              - kind: mock
                id: test-source
                auto_start: true
                data_type: sensor
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "snake_case fields should be detected as errors");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("auto_start") || err.contains("data_type"),
            "Error should mention the snake_case field: {err}");
    }

    #[test]
    fn test_typo_detected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            sources:
              - kind: mock
                id: test-source
                autoStart: true
                dataTypo: sensor
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "Typo should be detected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dataTypo"), "Error should mention the typo: {err}");
    }

    #[test]
    fn test_unknown_server_field_detected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            unknownField: value
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "Unknown server field should be detected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknownField"), "Error should mention unknownField: {err}");
    }

    #[test]
    fn test_nested_http_route_validation() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            reactions:
              - kind: http
                id: test-http
                queries: [q1]
                baseUrl: "http://localhost"
                routes:
                  q1:
                    added:
                      url: "/add"
                      method: "POST"
                      unknownSpec: value
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "Unknown nested field should be detected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknownSpec"), "Error should mention unknownSpec: {err}");
    }

    #[test]
    fn test_valid_postgres_config() {
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
                tables:
                  - users
                slotName: drasi_slot
                publicationName: drasi_pub
                sslMode: prefer
                tableKeys:
                  - table: users
                    keyColumns:
                      - id
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid postgres config should pass: {result:?}");
    }

    #[test]
    fn test_invalid_table_key_field() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                tableKeys:
                  - tableName: users
                    columns:
                      - id
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "Invalid tableKey fields should be detected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tableName") || err.contains("columns"),
            "Error should mention invalid field: {err}");
    }

    // ==================== Server-level field validation ====================

    #[test]
    fn test_server_snake_case_log_level_rejected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            log_level: info
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "log_level (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("log_level"), "Error should mention log_level: {err}");
    }

    #[test]
    fn test_server_snake_case_persist_config_rejected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            persist_config: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "persist_config (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("persist_config"), "Error should mention persist_config: {err}");
    }

    #[test]
    fn test_server_snake_case_persist_index_rejected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            persist_index: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "persist_index (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("persist_index"), "Error should mention persist_index: {err}");
    }

    #[test]
    fn test_server_snake_case_state_store_rejected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            state_store:
              kind: redb
              path: ./data/state.redb
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "state_store (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("state_store"), "Error should mention state_store: {err}");
    }

    // ==================== Source field validation ====================

    #[test]
    fn test_source_snake_case_auto_start_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: mock
                id: test-source
                auto_start: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "auto_start (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("auto_start"), "Error should mention auto_start: {err}");
    }

    #[test]
    fn test_source_snake_case_bootstrap_provider_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                bootstrap_provider:
                  type: postgres
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "bootstrap_provider (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bootstrap_provider"), "Error should mention bootstrap_provider: {err}");
    }

    #[test]
    fn test_mock_source_snake_case_data_type_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: mock
                id: test-source
                autoStart: true
                data_type: sensor
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "data_type (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("data_type"), "Error should mention data_type: {err}");
    }

    #[test]
    fn test_mock_source_snake_case_interval_ms_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: mock
                id: test-source
                autoStart: true
                interval_ms: 1000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "interval_ms (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("interval_ms"), "Error should mention interval_ms: {err}");
    }

    #[test]
    fn test_postgres_source_snake_case_slot_name_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                slot_name: drasi_slot
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "slot_name (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("slot_name"), "Error should mention slot_name: {err}");
    }

    #[test]
    fn test_postgres_source_snake_case_publication_name_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                publication_name: drasi_pub
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "publication_name (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("publication_name"), "Error should mention publication_name: {err}");
    }

    #[test]
    fn test_postgres_source_snake_case_ssl_mode_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                ssl_mode: prefer
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "ssl_mode (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("ssl_mode"), "Error should mention ssl_mode: {err}");
    }

    #[test]
    fn test_postgres_source_snake_case_table_keys_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                table_keys:
                  - table: users
                    keyColumns:
                      - id
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "table_keys (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("table_keys"), "Error should mention table_keys: {err}");
    }

    #[test]
    fn test_table_key_snake_case_key_columns_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: postgres
                id: pg-source
                database: testdb
                user: testuser
                tableKeys:
                  - table: users
                    key_columns:
                      - id
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "key_columns (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("key_columns"), "Error should mention key_columns: {err}");
    }

    #[test]
    fn test_http_source_snake_case_timeout_ms_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: http
                id: http-source
                host: localhost
                port: 8080
                timeout_ms: 5000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "timeout_ms (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timeout_ms"), "Error should mention timeout_ms: {err}");
    }

    #[test]
    fn test_grpc_source_snake_case_timeout_ms_rejected() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: grpc
                id: grpc-source
                host: localhost
                port: 50051
                timeout_ms: 5000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "timeout_ms (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timeout_ms"), "Error should mention timeout_ms: {err}");
    }

    // ==================== Query field validation ====================

    #[test]
    fn test_query_snake_case_auto_start_rejected() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                auto_start: true
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "auto_start (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("auto_start"), "Error should mention auto_start: {err}");
    }

    #[test]
    fn test_query_snake_case_query_language_rejected() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                query_language: Cypher
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "query_language (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("query_language"), "Error should mention query_language: {err}");
    }

    #[test]
    fn test_query_snake_case_enable_bootstrap_rejected() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                enable_bootstrap: true
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "enable_bootstrap (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("enable_bootstrap"), "Error should mention enable_bootstrap: {err}");
    }

    #[test]
    fn test_query_snake_case_bootstrap_buffer_size_rejected() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                bootstrap_buffer_size: 10000
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "bootstrap_buffer_size (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bootstrap_buffer_size"), "Error should mention bootstrap_buffer_size: {err}");
    }

    #[test]
    fn test_query_snake_case_priority_queue_capacity_rejected() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                priority_queue_capacity: 5000
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "priority_queue_capacity (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("priority_queue_capacity"), "Error should mention priority_queue_capacity: {err}");
    }

    #[test]
    fn test_query_snake_case_dispatch_buffer_capacity_rejected() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                dispatch_buffer_capacity: 1000
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "dispatch_buffer_capacity (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("dispatch_buffer_capacity"), "Error should mention dispatch_buffer_capacity: {err}");
    }

    // ==================== Reaction field validation ====================

    #[test]
    fn test_reaction_snake_case_auto_start_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                auto_start: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "auto_start (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("auto_start"), "Error should mention auto_start: {err}");
    }

    #[test]
    fn test_log_reaction_snake_case_default_template_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                default_template:
                  added:
                    template: "{{after}}"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "default_template (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("default_template"), "Error should mention default_template: {err}");
    }

    #[test]
    fn test_http_reaction_snake_case_base_url_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: http
                id: test-http
                queries: [q1]
                autoStart: true
                base_url: "http://localhost"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "base_url (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("base_url"), "Error should mention base_url: {err}");
    }

    #[test]
    fn test_http_reaction_snake_case_timeout_ms_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: http
                id: test-http
                queries: [q1]
                autoStart: true
                baseUrl: "http://localhost"
                timeout_ms: 5000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "timeout_ms (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timeout_ms"), "Error should mention timeout_ms: {err}");
    }

    #[test]
    fn test_http_adaptive_reaction_snake_case_fields_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: http-adaptive
                id: test-http-adaptive
                queries: [q1]
                autoStart: true
                baseUrl: "http://localhost"
                adaptive_min_batch_size: 10
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "adaptive_min_batch_size (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("adaptive_min_batch_size"), "Error should mention adaptive_min_batch_size: {err}");
    }

    #[test]
    fn test_sse_reaction_snake_case_sse_path_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: sse
                id: test-sse
                queries: [q1]
                autoStart: true
                sse_path: "/events"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "sse_path (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("sse_path"), "Error should mention sse_path: {err}");
    }

    #[test]
    fn test_sse_reaction_snake_case_heartbeat_interval_ms_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: sse
                id: test-sse
                queries: [q1]
                autoStart: true
                heartbeat_interval_ms: 30000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "heartbeat_interval_ms (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("heartbeat_interval_ms"), "Error should mention heartbeat_interval_ms: {err}");
    }

    #[test]
    fn test_grpc_reaction_snake_case_batch_size_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: grpc
                id: test-grpc
                queries: [q1]
                autoStart: true
                endpoint: "grpc://localhost:50051"
                batch_size: 100
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "batch_size (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("batch_size"), "Error should mention batch_size: {err}");
    }

    #[test]
    fn test_grpc_reaction_snake_case_max_retries_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: grpc
                id: test-grpc
                queries: [q1]
                autoStart: true
                endpoint: "grpc://localhost:50051"
                max_retries: 3
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "max_retries (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("max_retries"), "Error should mention max_retries: {err}");
    }

    #[test]
    fn test_profiler_reaction_snake_case_window_size_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: profiler
                id: test-profiler
                queries: [q1]
                autoStart: true
                window_size: 100
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "window_size (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("window_size"), "Error should mention window_size: {err}");
    }

    #[test]
    fn test_profiler_reaction_snake_case_report_interval_secs_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: profiler
                id: test-profiler
                queries: [q1]
                autoStart: true
                report_interval_secs: 60
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "report_interval_secs (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("report_interval_secs"), "Error should mention report_interval_secs: {err}");
    }

    // ==================== Multiple errors ====================

    #[test]
    fn test_multiple_errors_all_reported() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            log_level: info
            persist_config: true
            sources:
              - kind: mock
                id: test-source
                auto_start: true
                data_type: sensor
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                auto_start: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "Multiple errors should be detected");
        let err = result.unwrap_err().to_string();

        // Should contain multiple error mentions
        assert!(err.contains("log_level"), "Error should mention log_level: {err}");
        assert!(err.contains("persist_config"), "Error should mention persist_config: {err}");
        assert!(err.contains("auto_start"), "Error should mention auto_start: {err}");
        assert!(err.contains("data_type"), "Error should mention data_type: {err}");
    }

    // ==================== Instance config validation ====================

    #[test]
    fn test_instance_snake_case_persist_index_rejected() {
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

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "persist_index (snake_case) in instance should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("persist_index"), "Error should mention persist_index: {err}");
    }

    #[test]
    fn test_instance_snake_case_state_store_rejected() {
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

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "state_store (snake_case) in instance should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("state_store"), "Error should mention state_store: {err}");
    }

    // ==================== Unknown source/reaction kinds ====================

    #[test]
    fn test_unknown_source_kind_allows_common_fields() {
        // Unknown kinds should still validate common fields
        let yaml = r#"
            id: test-server
            sources:
              - kind: custom-source
                id: test-custom
                autoStart: true
                unknownField: value
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        // Should error on unknownField since it's not in common fields
        assert!(result.is_err(), "Unknown field in custom source should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknownField"), "Error should mention unknownField: {err}");
    }

    #[test]
    fn test_unknown_reaction_kind_allows_common_fields() {
        // Unknown kinds should still validate common fields
        let yaml = r#"
            id: test-server
            reactions:
              - kind: custom-reaction
                id: test-custom
                queries: [q1]
                autoStart: true
                unknownField: value
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        // Should error on unknownField since it's not in common fields
        assert!(result.is_err(), "Unknown field in custom reaction should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknownField"), "Error should mention unknownField: {err}");
    }

    // ==================== Valid configurations (positive tests) ====================

    #[test]
    fn test_valid_http_source_config() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: http
                id: http-source
                autoStart: true
                host: "0.0.0.0"
                port: 9000
                endpoint: "/events"
                timeoutMs: 10000
                adaptiveMaxBatchSize: 100
                adaptiveMinBatchSize: 10
                adaptiveMaxWaitMs: 500
                adaptiveMinWaitMs: 10
                adaptiveWindowSecs: 60
                adaptiveEnabled: true
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid HTTP source config should pass: {result:?}");
    }

    #[test]
    fn test_valid_grpc_source_config() {
        let yaml = r#"
            id: test-server
            sources:
              - kind: grpc
                id: grpc-source
                autoStart: true
                host: "0.0.0.0"
                port: 50051
                endpoint: "/stream"
                timeoutMs: 5000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid gRPC source config should pass: {result:?}");
    }

    #[test]
    fn test_valid_http_reaction_with_routes() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: http
                id: test-http
                queries: [q1]
                autoStart: true
                baseUrl: "http://localhost:3000"
                token: "secret-token"
                timeoutMs: 5000
                routes:
                  q1:
                    added:
                      url: "/api/events"
                      method: "POST"
                      body: '{"event": {{after}}}'
                      headers:
                        Content-Type: "application/json"
                    updated:
                      url: "/api/events"
                      method: "PUT"
                      body: '{"before": {{before}}, "after": {{after}}}'
                    deleted:
                      url: "/api/events"
                      method: "DELETE"
                      body: '{"event": {{before}}}'
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid HTTP reaction with routes should pass: {result:?}");
    }

    #[test]
    fn test_valid_sse_reaction_config() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: sse
                id: test-sse
                queries: [q1]
                autoStart: true
                host: "0.0.0.0"
                port: 8081
                ssePath: "/events"
                heartbeatIntervalMs: 30000
                routes:
                  q1:
                    added:
                      path: "/q1/added"
                      template: "{{after}}"
                defaultTemplate:
                  added:
                    template: "{{after}}"
                  updated:
                    template: "{{before}} -> {{after}}"
                  deleted:
                    template: "{{before}}"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid SSE reaction config should pass: {result:?}");
    }

    #[test]
    fn test_valid_grpc_reaction_config() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: grpc
                id: test-grpc
                queries: [q1]
                autoStart: true
                endpoint: "grpc://localhost:50052"
                timeoutMs: 5000
                batchSize: 100
                batchFlushTimeoutMs: 1000
                maxRetries: 3
                connectionRetryAttempts: 5
                initialConnectionTimeoutMs: 10000
                metadata:
                  Authorization: "Bearer token"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid gRPC reaction config should pass: {result:?}");
    }

    #[test]
    fn test_valid_grpc_adaptive_reaction_config() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: grpc-adaptive
                id: test-grpc-adaptive
                queries: [q1]
                autoStart: true
                endpoint: "grpc://localhost:50052"
                timeoutMs: 5000
                maxRetries: 3
                connectionRetryAttempts: 5
                initialConnectionTimeoutMs: 10000
                adaptiveMinBatchSize: 10
                adaptiveMaxBatchSize: 500
                adaptiveWindowSize: 100
                adaptiveBatchTimeoutMs: 2000
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid gRPC adaptive reaction config should pass: {result:?}");
    }

    #[test]
    fn test_valid_profiler_reaction_config() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: profiler
                id: test-profiler
                queries: [q1]
                autoStart: true
                windowSize: 100
                reportIntervalSecs: 60
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid profiler reaction config should pass: {result:?}");
    }

    #[test]
    fn test_valid_query_with_all_fields() {
        let yaml = r#"
            id: test-server
            queries:
              - id: test-query
                query: "MATCH (n) RETURN n"
                queryLanguage: Cypher
                autoStart: true
                enableBootstrap: true
                bootstrapBufferSize: 10000
                priorityQueueCapacity: 5000
                dispatchBufferCapacity: 1000
                middleware: []
                dispatchMode: "immediate"
                sources:
                  - sourceId: test-source
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid query with all fields should pass: {result:?}");
    }

    #[test]
    fn test_valid_multi_instance_config() {
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

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid multi-instance config should pass: {result:?}");
    }
}
