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

//! Configuration template syntax validation.
//!
//! This module validates Handlebars template syntax in configuration files.
//! Unknown field validation is handled by serde's `deny_unknown_fields` attribute
//! on the config structs themselves.

use handlebars::Handlebars;

/// Validation error for configuration.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Template syntax error: {0}")]
    TemplateSyntax(String),

    #[error("Multiple validation errors:\n{}", .0.join("\n"))]
    Multiple(Vec<String>),
}

/// Validate template syntax in a configuration value.
///
/// This validates Handlebars template syntax in reactions that use templates.
/// Unknown field validation is handled by serde's `deny_unknown_fields` attribute
/// during deserialization.
pub fn validate_config(value: &serde_yaml::Value) -> Result<(), ValidationError> {
    let mut errors = Vec::new();

    if let Some(map) = value.as_mapping() {
        // Validate reactions (template syntax)
        if let Some(reactions) = map.get("reactions") {
            validate_reactions(reactions, &mut errors);
        }

        // Validate instances (nested reactions)
        if let Some(instances) = map.get("instances") {
            validate_instances(instances, &mut errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else if errors.len() == 1 {
        Err(ValidationError::TemplateSyntax(errors.remove(0)))
    } else {
        Err(ValidationError::Multiple(errors))
    }
}

/// Validate reaction configurations for template syntax.
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
        for instance in arr.iter() {
            if let Some(map) = instance.as_mapping() {
                // Validate nested reactions for template syntax
                if let Some(reactions) = map.get("reactions") {
                    validate_reactions(reactions, errors);
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
    use crate::api::models::QueryConfigDto;
    use crate::config::types::{DrasiLibInstanceConfig, DrasiServerConfig};

    // ==================== Template syntax validation (validate_config) ====================

    #[test]
    fn test_valid_config_passes() {
        let yaml = r#"
            id: test-server
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

    // ==================== Serde deny_unknown_fields validation ====================
    // These tests verify that unknown fields are rejected during deserialization

    #[test]
    fn test_server_unknown_field_rejected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            unknownField: value
        "#;

        let result: Result<DrasiServerConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "Unknown server field should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_server_snake_case_log_level_rejected() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            log_level: info
        "#;

        let result: Result<DrasiServerConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "log_level (snake_case) should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field") && err.contains("log_level"),
            "Error should mention unknown field log_level: {err}"
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

        let result: Result<DrasiServerConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "persist_config (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_instance_unknown_field_rejected() {
        let yaml = r#"
            id: instance-1
            persistIndex: true
            unknownField: value
            sources: []
            queries: []
            reactions: []
        "#;

        let result: Result<DrasiLibInstanceConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "Unknown instance field should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_instance_snake_case_persist_index_rejected() {
        let yaml = r#"
            id: instance-1
            persist_index: true
            sources: []
            queries: []
            reactions: []
        "#;

        let result: Result<DrasiLibInstanceConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "persist_index (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_query_unknown_field_rejected() {
        let yaml = r#"
            id: test-query
            query: "MATCH (n) RETURN n"
            unknownField: value
        "#;

        let result: Result<QueryConfigDto, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "Unknown query field should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_query_snake_case_auto_start_rejected() {
        let yaml = r#"
            id: test-query
            query: "MATCH (n) RETURN n"
            auto_start: true
        "#;

        let result: Result<QueryConfigDto, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "auto_start (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    #[test]
    fn test_query_snake_case_query_language_rejected() {
        let yaml = r#"
            id: test-query
            query: "MATCH (n) RETURN n"
            query_language: Cypher
        "#;

        let result: Result<QueryConfigDto, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "query_language (snake_case) should be rejected"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Error should mention unknown field: {err}"
        );
    }

    // ==================== Valid configurations (positive tests) ====================

    #[test]
    fn test_valid_server_config() {
        let yaml = r#"
            id: test-server
            host: 0.0.0.0
            port: 8080
            logLevel: info
            persistConfig: true
            persistIndex: false
        "#;

        let result: Result<DrasiServerConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_ok(),
            "Valid server config should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_valid_query_with_all_fields() {
        let yaml = r#"
            id: test-query
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

        let result: Result<QueryConfigDto, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_ok(),
            "Valid query config should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_valid_instance_config() {
        let yaml = r#"
            id: instance-1
            persistIndex: true
            sources: []
            queries: []
            reactions: []
        "#;

        let result: Result<DrasiLibInstanceConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_ok(),
            "Valid instance config should parse: {:?}",
            result.err()
        );
    }
}
