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
//! Validation is split between:
//! 1. This module: server-level, instance, query fields, bootstrap providers, and template syntax
//! 2. Custom deserializers in api/models: source and reaction fields (using deny_unknown_fields)

use std::collections::HashSet;

use handlebars::Handlebars;

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

/// Known fields for state store configuration.
const STATE_STORE_REDB_FIELDS: &[&str] = &["kind", "path"];

/// Known fields for bootstrap provider configuration.
/// Note: `type` is supported as an alias for `kind` in bootstrap provider configs.
const BOOTSTRAP_PROVIDER_FIELDS: &[&str] = &["kind", "type", "path", "sourceId"];

/// Validate a configuration value and return all unknown field errors.
///
/// This validates server-level, instance, and query fields, as well as
/// bootstrap providers and template syntax. Source and reaction field
/// validation is handled by custom deserializers with deny_unknown_fields.
pub fn validate_config(value: &serde_yaml::Value) -> Result<(), ValidationError> {
    let mut errors = Vec::new();

    if let Some(map) = value.as_mapping() {
        // Validate server-level fields
        validate_fields(map, SERVER_FIELDS, "server configuration", &mut errors);

        // Validate sources (bootstrap providers and template syntax only)
        if let Some(sources) = map.get("sources") {
            validate_sources(sources, &mut errors);
        }

        // Validate queries
        if let Some(queries) = map.get("queries") {
            validate_queries(queries, &mut errors);
        }

        // Validate reactions (template syntax only)
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

/// Validate source configurations.
/// Note: Field validation for sources is handled by custom deserializers.
/// This function only validates bootstrap providers.
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

                // Validate bootstrapProvider if present
                if let Some(bp) = map.get("bootstrapProvider") {
                    validate_bootstrap_provider(
                        bp,
                        &format!("{context} bootstrapProvider"),
                        errors,
                    );
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

/// Validate reaction configurations.
/// Note: Field validation for reactions is handled by custom deserializers.
/// This function only validates template syntax.
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

                // Validate template syntax for http/http-adaptive
                if matches!(kind, "http" | "http-adaptive") {
                    if let Some(routes) = map.get("routes") {
                        validate_http_routes_templates(routes, &context, errors);
                    }
                }

                // Validate template syntax for log/sse
                if matches!(kind, "log" | "sse") {
                    if let Some(routes) = map.get("routes") {
                        validate_template_routes(routes, &context, errors);
                    }
                    if let Some(dt) = map.get("defaultTemplate") {
                        validate_template_query_config(
                            dt,
                            &format!("{context} defaultTemplate"),
                            errors,
                        );
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

/// Validate HTTP routes for template syntax only.
fn validate_http_routes_templates(
    routes: &serde_yaml::Value,
    parent_context: &str,
    errors: &mut Vec<String>,
) {
    if let Some(map) = routes.as_mapping() {
        for (key, route) in map {
            let route_name = key.as_str().unwrap_or("unknown");
            let context = format!("{parent_context} routes.{route_name}");

            if let Some(route_map) = route.as_mapping() {
                // Validate each call spec (added/updated/deleted) for template syntax
                for field in ["added", "updated", "deleted"] {
                    if let Some(call_spec) = route_map.get(field) {
                        if let Some(spec_map) = call_spec.as_mapping() {
                            let field_context = format!("{context}.{field}");

                            // Validate body field as Handlebars template
                            if let Some(body_val) = spec_map.get("body") {
                                if let Some(body_str) = body_val.as_str() {
                                    validate_template_syntax(
                                        body_str,
                                        &format!("{field_context}.body"),
                                        errors,
                                    );
                                }
                            }

                            // Validate url field as Handlebars template (can contain {{variable}})
                            if let Some(url_val) = spec_map.get("url") {
                                if let Some(url_str) = url_val.as_str() {
                                    validate_template_syntax(
                                        url_str,
                                        &format!("{field_context}.url"),
                                        errors,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn validate_template_routes(
    routes: &serde_yaml::Value,
    parent_context: &str,
    errors: &mut Vec<String>,
) {
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

fn validate_template_query_config(
    value: &serde_yaml::Value,
    context: &str,
    errors: &mut Vec<String>,
) {
    if let Some(map) = value.as_mapping() {
        validate_template_query_config_inner(map, context, errors);
    }
}

fn validate_template_query_config_inner(
    map: &serde_yaml::Mapping,
    context: &str,
    errors: &mut Vec<String>,
) {
    // Validate each template spec (added/updated/deleted) for syntax
    for field in ["added", "updated", "deleted"] {
        if let Some(spec) = map.get(field) {
            if let Some(spec_map) = spec.as_mapping() {
                let field_context = format!("{context}.{field}");

                // Validate template syntax if template field is present
                if let Some(template_val) = spec_map.get("template") {
                    if let Some(template_str) = template_val.as_str() {
                        validate_template_syntax(
                            template_str,
                            &format!("{field_context}.template"),
                            errors,
                        );
                    }
                }
            }
        }
    }
}

fn validate_state_store(value: &serde_yaml::Value, context: &str, errors: &mut Vec<String>) {
    if let Some(map) = value.as_mapping() {
        let kind = map
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
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

/// Validates that a Handlebars template string is syntactically valid.
/// Uses Handlebars' own register_template_string() which parses and compiles
/// the template, returning errors for invalid syntax.
fn validate_template_syntax(template: &str, context: &str, errors: &mut Vec<String>) {
    let mut hb = Handlebars::new();

    // Handlebars::register_template_string() performs full parsing and compilation.
    // It will catch:
    // - Unclosed braces: "{{name"
    // - Empty expressions: "{{}}"
    // - Invalid syntax: "{{#if}}" without closing
    // - Malformed helpers: "{{#each items}}...{{/each}" with mismatched tags
    if let Err(e) = hb.register_template_string("_validation_", template) {
        errors.push(format!("{context}: invalid Handlebars template - {e}"));
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
        assert!(
            err.contains("unknownField"),
            "Error should mention unknownField: {err}"
        );
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
        assert!(
            err.contains("log_level"),
            "Error should mention log_level: {err}"
        );
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
        assert!(
            result.is_err(),
            "persist_config (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("persist_config"),
            "Error should mention persist_config: {err}"
        );
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
        assert!(
            result.is_err(),
            "persist_index (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("persist_index"),
            "Error should mention persist_index: {err}"
        );
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
        assert!(
            result.is_err(),
            "state_store (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("state_store"),
            "Error should mention state_store: {err}"
        );
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
        assert!(
            result.is_err(),
            "auto_start (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("auto_start"),
            "Error should mention auto_start: {err}"
        );
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
        assert!(
            result.is_err(),
            "query_language (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("query_language"),
            "Error should mention query_language: {err}"
        );
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
        assert!(
            result.is_err(),
            "enable_bootstrap (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("enable_bootstrap"),
            "Error should mention enable_bootstrap: {err}"
        );
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
        assert!(
            result.is_err(),
            "bootstrap_buffer_size (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("bootstrap_buffer_size"),
            "Error should mention bootstrap_buffer_size: {err}"
        );
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
        assert!(
            result.is_err(),
            "priority_queue_capacity (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("priority_queue_capacity"),
            "Error should mention priority_queue_capacity: {err}"
        );
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
        assert!(
            result.is_err(),
            "dispatch_buffer_capacity (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("dispatch_buffer_capacity"),
            "Error should mention dispatch_buffer_capacity: {err}"
        );
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
        assert!(
            result.is_err(),
            "persist_index (snake_case) in instance should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("persist_index"),
            "Error should mention persist_index: {err}"
        );
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
        assert!(
            result.is_err(),
            "state_store (snake_case) in instance should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("state_store"),
            "Error should mention state_store: {err}"
        );
    }

    // ==================== Valid configurations (positive tests) ====================

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
        assert!(
            result.is_ok(),
            "Valid query with all fields should pass: {result:?}"
        );
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
        assert!(
            result.is_ok(),
            "Valid multi-instance config should pass: {result:?}"
        );
    }

    // ==================== Template syntax validation ====================

    #[test]
    fn test_valid_template_passes() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                defaultTemplate:
                  added:
                    template: "{{after.Name}} - {{after.Value}}"
                  updated:
                    template: "Changed from {{before.Value}} to {{after.Value}}"
                  deleted:
                    template: "Removed: {{before.Name}}"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_ok(), "Valid templates should pass: {result:?}");
    }

    #[test]
    fn test_template_unclosed_brace_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                defaultTemplate:
                  added:
                    template: "{{after.Name"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_err(),
            "Unclosed brace template should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid Handlebars template"),
            "Error should mention invalid template: {err}"
        );
    }

    #[test]
    fn test_template_empty_expression_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                defaultTemplate:
                  added:
                    template: "Value: {{}}"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_err(),
            "Empty expression template should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid Handlebars template"),
            "Error should mention invalid template: {err}"
        );
    }

    #[test]
    fn test_template_unclosed_block_rejected() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                defaultTemplate:
                  added:
                    template: "{{#if condition}}true branch"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_err(),
            "Unclosed block template should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid Handlebars template"),
            "Error should mention invalid template: {err}"
        );
    }

    #[test]
    fn test_http_body_template_validated() {
        let yaml = r#"
            id: test-server
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
                      body: '{"data": {{after.Name}'
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_err(),
            "Invalid HTTP body template should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("body") && err.contains("invalid Handlebars template"),
            "Error should mention body and invalid template: {err}"
        );
    }

    #[test]
    fn test_valid_http_body_template_passes() {
        let yaml = r#"
            id: test-server
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
                      body: '{"name": "{{after.Name}}", "value": {{after.Value}}}'
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_ok(),
            "Valid HTTP body template should pass: {result:?}"
        );
    }

    #[test]
    fn test_sse_template_validated() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: sse
                id: test-sse
                queries: [q1]
                autoStart: true
                routes:
                  q1:
                    added:
                      template: "{{after.Name"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(result.is_err(), "Invalid SSE template should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid Handlebars template"),
            "Error should mention invalid template: {err}"
        );
    }

    #[test]
    fn test_template_with_helpers_passes() {
        // Handlebars allows unknown helpers (they just evaluate to empty at runtime)
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                defaultTemplate:
                  added:
                    template: "{{#each items}}{{this}}{{/each}}"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_ok(),
            "Template with valid helper syntax should pass: {result:?}"
        );
    }

    #[test]
    fn test_multiple_template_errors_all_reported() {
        let yaml = r#"
            id: test-server
            reactions:
              - kind: log
                id: test-log
                queries: [q1]
                autoStart: true
                defaultTemplate:
                  added:
                    template: "{{after.Name"
                  updated:
                    template: "{{before"
                  deleted:
                    template: "{{}}"
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let result = validate_config(&value);
        assert!(
            result.is_err(),
            "Multiple invalid templates should be rejected"
        );
        let err = result.unwrap_err().to_string();
        // Should contain multiple error messages
        assert!(
            err.contains("added") || err.contains("updated") || err.contains("deleted"),
            "Error should mention which template field has the error: {err}"
        );
    }
}
