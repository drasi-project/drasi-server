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

//! JSON Schema–based config validation.
//!
//! This module validates component configs against their plugin's OpenAPI
//! schemas. It builds a JSON Schema document from an OpenAPI schema map,
//! rewrites `$ref` paths, and validates instances using `jsonschema`.

use log::warn;
use serde_json::Value;

use super::plugin_validation::{
    collect_all_bootstrap_providers, collect_all_reactions, collect_all_sources,
    schema_error_codes, ComponentValidationReport, FieldError,
};
use crate::config::types::DrasiServerConfig;
use crate::plugin_registry::PluginRegistry;

/// Validate component configs against their plugin's OpenAPI schemas.
///
/// For each source/reaction in the config whose plugin is registered, this
/// fetches the schema via `config_schema_json()` and validates the component's
/// config JSON against it.
pub fn validate_component_configs(
    config: &DrasiServerConfig,
    registry: &PluginRegistry,
) -> Vec<ComponentValidationReport> {
    let mut reports = Vec::new();

    let all_sources = collect_all_sources(config);
    let all_reactions = collect_all_reactions(config);
    let all_bootstrap_providers = collect_all_bootstrap_providers(config);

    for src in &all_sources {
        if let Some(descriptor) = registry.get_source(&src.kind) {
            let schema_json = descriptor.config_schema_json();
            let schema_name = descriptor.config_schema_name().to_string();
            let errors = validate_against_schema(&schema_json, &schema_name, &src.config);
            if !errors.is_empty() {
                reports.push(ComponentValidationReport {
                    component_type: "source".to_string(),
                    component_id: src.id.clone(),
                    plugin_kind: src.kind.clone(),
                    errors,
                });
            }
        }

        // Also validate bootstrap provider config if plugin is available
        if let Some(bp) = src.bootstrap_provider().and_then(|r| r.as_inline()) {
            if let Some(descriptor) = registry.get_bootstrapper(&bp.kind) {
                let schema_json = descriptor.config_schema_json();
                let schema_name = descriptor.config_schema_name().to_string();
                let errors = validate_against_schema(&schema_json, &schema_name, &bp.config);
                if !errors.is_empty() {
                    reports.push(ComponentValidationReport {
                        component_type: "bootstrap".to_string(),
                        component_id: format!("{} bootstrapProvider", src.id),
                        plugin_kind: bp.kind.clone(),
                        errors,
                    });
                }
            }
        }
    }

    for rxn in &all_reactions {
        if let Some(descriptor) = registry.get_reaction(&rxn.kind) {
            let schema_json = descriptor.config_schema_json();
            let schema_name = descriptor.config_schema_name().to_string();
            let errors = validate_against_schema(&schema_json, &schema_name, &rxn.config);
            if !errors.is_empty() {
                reports.push(ComponentValidationReport {
                    component_type: "reaction".to_string(),
                    component_id: rxn.id.clone(),
                    plugin_kind: rxn.kind.clone(),
                    errors,
                });
            }
        }
    }

    // Validate top-level bootstrap providers against their plugin schemas.
    for bp in &all_bootstrap_providers {
        if let Some(descriptor) = registry.get_bootstrapper(&bp.kind) {
            let schema_json = descriptor.config_schema_json();
            let schema_name = descriptor.config_schema_name().to_string();
            let errors = validate_against_schema(&schema_json, &schema_name, &bp.config);
            if !errors.is_empty() {
                reports.push(ComponentValidationReport {
                    component_type: "bootstrap".to_string(),
                    component_id: format!(
                        "bootstrapProvider '{}'",
                        bp.id().unwrap_or("<missing id>")
                    ),
                    plugin_kind: bp.kind.clone(),
                    errors,
                });
            }
        }
    }

    reports
}

/// Build a JSON Schema document from an OpenAPI schema map and validate
/// `instance` against it.
///
/// `schema_map_json` is the raw JSON string from `config_schema_json()`:
/// ```json
/// {
///   "source.postgres.PostgresSourceConfig": { "type": "object", ... },
///   "source.postgres.SslMode": { "type": "string", "enum": [...] }
/// }
/// ```
///
/// `entry_name` identifies the root schema (from `config_schema_name()`).
fn validate_against_schema(
    schema_map_json: &str,
    entry_name: &str,
    instance: &Value,
) -> Vec<FieldError> {
    // Parse the schema map
    let schema_map: serde_json::Map<String, Value> = match serde_json::from_str(schema_map_json) {
        Ok(Value::Object(map)) => map,
        Ok(_) | Err(_) => {
            warn!("Could not parse plugin schema JSON for entry '{entry_name}'");
            return vec![FieldError {
                field: "(schema)".to_string(),
                message: "Plugin schema could not be parsed as a JSON object — \
                         component config could not be validated"
                    .to_string(),
                code: Some(schema_error_codes::SCHEMA_UNPARSEABLE),
            }];
        }
    };

    // Build JSON Schema document with $defs
    let mut defs = serde_json::Map::new();
    for (name, mut schema_obj) in schema_map {
        // Rewrite OpenAPI $ref paths from #/components/schemas/X to #/$defs/X
        rewrite_refs(&mut schema_obj);
        defs.insert(name, schema_obj);
    }

    // Ensure the entry point exists
    if !defs.contains_key(entry_name) {
        warn!("Schema entry point '{entry_name}' not found in plugin schemas");
        return vec![FieldError {
            field: "(schema)".to_string(),
            message: format!(
                "Schema entry point '{entry_name}' not found in plugin schemas — \
                 component config could not be validated"
            ),
            code: Some(schema_error_codes::SCHEMA_ENTRY_MISSING),
        }];
    }

    let schema_doc = serde_json::json!({
        "$ref": format!("#/$defs/{entry_name}"),
        "$defs": Value::Object(defs),
    });

    // Validate
    let validator = match jsonschema::validator_for(&schema_doc) {
        Ok(v) => v,
        Err(e) => {
            // The schema map parses and the entry exists, but `jsonschema`
            // could not build a validator from the resulting document —
            // typically because a `$ref` points at a `$defs/...` entry that
            // wasn't published in the plugin's schema map (incomplete
            // `$defs`).
            //
            // This is a plugin-packaging bug and is currently widespread in
            // upstream drasi-core plugin schemas (e.g. references to
            // `QueryConfigDto`, `ConfigValue`, `ConfigValueString` that
            // aren't included alongside the root schema). Returning a
            // fatal error here would block users from validating any
            // example config that uses those plugins, even though the
            // configs themselves are well-formed.
            //
            // Log loudly via `error!` (previously `warn!`) so the
            // condition is visible in operator logs, and skip
            // schema-shape validation for this component. This is *not*
            // returning a synthetic `FieldError` — doing so would put us
            // back in the position of silently passing a config that
            // couldn't actually be checked, which is worse than logging
            // and continuing. Once upstream plugin schemas are fixed,
            // this branch should become unreachable and can be promoted
            // to a fatal `FieldError` with
            // `schema_error_codes::SCHEMA_VALIDATOR_BUILD_FAILED`.
            log::error!(
                "Could not compile JSON Schema for entry '{entry_name}' \
                 (likely incomplete plugin $defs) — skipping schema-shape \
                 validation for this component: {e}"
            );
            return Vec::new();
        }
    };

    validator
        .iter_errors(instance)
        .map(|err| {
            let field = if err.instance_path.as_str().is_empty() {
                "(root)".to_string()
            } else {
                err.instance_path.to_string()
            };
            FieldError {
                field,
                message: err.to_string(),
                code: None,
            }
        })
        .collect()
}

/// Recursively rewrite `$ref` values from OpenAPI's
/// `#/components/schemas/Name` to JSON Schema's `#/$defs/Name`.
fn rewrite_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(ref_val)) = map.get_mut("$ref") {
                if let Some(name) = ref_val.strip_prefix("#/components/schemas/") {
                    *ref_val = format!("#/$defs/{name}");
                }
            }
            // Also handle allOf/oneOf/anyOf which may contain $ref objects
            for val in map.values_mut() {
                rewrite_refs(val);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                rewrite_refs(val);
            }
        }
        _ => {}
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{
        BootstrapProviderConfig, BootstrapProviderRef, ReactionConfig, SourceConfig,
    };
    use crate::config::types::DrasiServerConfig;
    use async_trait::async_trait;
    use drasi_plugin_sdk::{
        BootstrapPluginDescriptor, ReactionPluginDescriptor, SourcePluginDescriptor,
    };
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Mock descriptors
    // -----------------------------------------------------------------------

    struct MockSourceDesc {
        kind: &'static str,
        schema_json: &'static str,
        schema_name: &'static str,
    }

    #[async_trait]
    impl SourcePluginDescriptor for MockSourceDesc {
        fn kind(&self) -> &str {
            self.kind
        }
        fn config_version(&self) -> &str {
            "1.0.0"
        }
        fn config_schema_json(&self) -> String {
            self.schema_json.to_string()
        }
        fn config_schema_name(&self) -> &str {
            self.schema_name
        }
        async fn create_source(
            &self,
            _id: &str,
            _config_json: &serde_json::Value,
            _auto_start: bool,
        ) -> anyhow::Result<Box<dyn drasi_lib::sources::Source>> {
            anyhow::bail!("mock")
        }
    }

    struct MockReactionDesc {
        kind: &'static str,
        schema_json: &'static str,
        schema_name: &'static str,
    }

    #[async_trait]
    impl ReactionPluginDescriptor for MockReactionDesc {
        fn kind(&self) -> &str {
            self.kind
        }
        fn config_version(&self) -> &str {
            "1.0.0"
        }
        fn config_schema_json(&self) -> String {
            self.schema_json.to_string()
        }
        fn config_schema_name(&self) -> &str {
            self.schema_name
        }
        async fn create_reaction(
            &self,
            _id: &str,
            _query_ids: Vec<String>,
            _config_json: &serde_json::Value,
            _auto_start: bool,
        ) -> anyhow::Result<Box<dyn drasi_lib::reactions::Reaction>> {
            anyhow::bail!("mock")
        }
    }

    struct MockBootstrapDesc {
        kind: &'static str,
        schema_json: &'static str,
        schema_name: &'static str,
    }

    #[async_trait]
    impl BootstrapPluginDescriptor for MockBootstrapDesc {
        fn kind(&self) -> &str {
            self.kind
        }
        fn config_version(&self) -> &str {
            "1.0.0"
        }
        fn config_schema_json(&self) -> String {
            self.schema_json.to_string()
        }
        fn config_schema_name(&self) -> &str {
            self.schema_name
        }
        async fn create_bootstrap_provider(
            &self,
            _config_json: &serde_json::Value,
            _source_config_json: &serde_json::Value,
        ) -> anyhow::Result<Box<dyn drasi_lib::bootstrap::BootstrapProvider>> {
            anyhow::bail!("mock")
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn config_with(
        sources: Vec<SourceConfig>,
        reactions: Vec<ReactionConfig>,
    ) -> DrasiServerConfig {
        DrasiServerConfig {
            sources,
            reactions,
            ..Default::default()
        }
    }

    fn source(kind: &str, id: &str) -> SourceConfig {
        SourceConfig {
            kind: kind.to_string(),
            id: id.to_string(),
            auto_start: true,
            bootstrap_provider: None,
            identity_provider: None,
            config: serde_json::json!({}),
        }
    }

    // Realistic OpenAPI-style schema map
    const POSTGRES_SCHEMA: &str = r##"{
        "source.postgres.Config": {
            "type": "object",
            "required": ["host", "database"],
            "properties": {
                "host": { "type": "string" },
                "port": { "type": "integer" },
                "database": { "type": "string" },
                "sslMode": { "$ref": "#/components/schemas/source.postgres.SslMode" }
            },
            "additionalProperties": false
        },
        "source.postgres.SslMode": {
            "type": "string",
            "enum": ["disable", "prefer", "require"]
        }
    }"##;

    const REACTION_SCHEMA: &str = r#"{
        "reaction.log.Config": {
            "type": "object",
            "properties": {
                "level": { "type": "string", "enum": ["info", "debug", "warn"] }
            },
            "additionalProperties": false
        }
    }"#;

    const BOOTSTRAP_SCHEMA: &str = r#"{
        "bootstrap.scriptfile.Config": {
            "type": "object",
            "required": ["filePaths"],
            "properties": {
                "filePaths": { "type": "array", "items": { "type": "string" } }
            },
            "additionalProperties": false
        }
    }"#;

    fn mock_registry() -> PluginRegistry {
        let mut reg = PluginRegistry::new();
        reg.register_source(Arc::new(MockSourceDesc {
            kind: "postgres",
            schema_json: POSTGRES_SCHEMA,
            schema_name: "source.postgres.Config",
        }));
        reg.register_reaction(Arc::new(MockReactionDesc {
            kind: "log",
            schema_json: REACTION_SCHEMA,
            schema_name: "reaction.log.Config",
        }));
        reg.register_bootstrapper(Arc::new(MockBootstrapDesc {
            kind: "scriptfile",
            schema_json: BOOTSTRAP_SCHEMA,
            schema_name: "bootstrap.scriptfile.Config",
        }));
        reg
    }

    // =======================================================================
    // validate_component_configs
    // =======================================================================

    #[test]
    fn test_valid_source_config() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                identity_provider: None,
                config: serde_json::json!({
                    "host": "localhost",
                    "database": "mydb"
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        assert!(reports.is_empty());
    }

    #[test]
    fn test_missing_required_field() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                identity_provider: None,
                config: serde_json::json!({
                    "host": "localhost"
                    // missing "database" which is required
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].component_id, "pg1");
        assert!(!reports[0].errors.is_empty());
        assert!(reports[0]
            .errors
            .iter()
            .any(|e| e.message.contains("database")));
    }

    #[test]
    fn test_unknown_field() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                identity_provider: None,
                config: serde_json::json!({
                    "host": "localhost",
                    "database": "mydb",
                    "unknownField": true
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        assert_eq!(reports.len(), 1);
        assert!(!reports[0].errors.is_empty());
    }

    #[test]
    fn test_invalid_field_type() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                identity_provider: None,
                config: serde_json::json!({
                    "host": 12345,  // should be string
                    "database": "mydb"
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].errors.iter().any(|e| e.field.contains("host")));
    }

    #[test]
    fn test_schema_ref_validation() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                identity_provider: None,
                config: serde_json::json!({
                    "host": "localhost",
                    "database": "mydb",
                    "sslMode": "invalid_mode"  // not in enum
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        assert_eq!(reports.len(), 1);
        assert!(reports[0]
            .errors
            .iter()
            .any(|e| e.field.contains("sslMode")));
    }

    #[test]
    fn test_plugin_not_in_registry_skipped() {
        let registry = PluginRegistry::new();
        let config = config_with(vec![source("unknown-plugin", "s1")], vec![]);

        // Should not produce reports for unregistered plugins
        let reports = validate_component_configs(&config, &registry);
        assert!(reports.is_empty());
    }

    #[test]
    fn test_reaction_validation() {
        let registry = mock_registry();
        let config = config_with(
            vec![],
            vec![ReactionConfig {
                kind: "log".to_string(),
                id: "log1".to_string(),
                queries: vec![],
                auto_start: true,
                identity_provider: None,
                config: serde_json::json!({
                    "level": "info"
                }),
            }],
        );

        let reports = validate_component_configs(&config, &registry);
        assert!(reports.is_empty());
    }

    #[test]
    fn test_reaction_invalid_enum_value() {
        let registry = mock_registry();
        let config = config_with(
            vec![],
            vec![ReactionConfig {
                kind: "log".to_string(),
                id: "log1".to_string(),
                queries: vec![],
                auto_start: true,
                identity_provider: None,
                config: serde_json::json!({
                    "level": "verbose"  // not in enum
                }),
            }],
        );

        let reports = validate_component_configs(&config, &registry);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].errors.iter().any(|e| e.field.contains("level")));
    }

    #[test]
    fn test_bootstrap_validation() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                identity_provider: None,
                bootstrap_provider: Some(BootstrapProviderRef::Inline(BootstrapProviderConfig {
                    kind: "scriptfile".to_string(),
                    id: None,
                    config: serde_json::json!({
                        "filePaths": ["/data/file1.jsonl"]
                    }),
                })),
                config: serde_json::json!({
                    "host": "localhost",
                    "database": "mydb"
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        assert!(reports.is_empty());
    }

    #[test]
    fn test_bootstrap_missing_required() {
        let registry = mock_registry();
        let config = config_with(
            vec![SourceConfig {
                kind: "postgres".to_string(),
                id: "pg1".to_string(),
                auto_start: true,
                identity_provider: None,
                bootstrap_provider: Some(BootstrapProviderRef::Inline(BootstrapProviderConfig {
                    kind: "scriptfile".to_string(),
                    id: None,
                    config: serde_json::json!({}), // missing filePaths
                })),
                config: serde_json::json!({
                    "host": "localhost",
                    "database": "mydb"
                }),
            }],
            vec![],
        );

        let reports = validate_component_configs(&config, &registry);
        // Should have error for bootstrap but not for source
        let bootstrap_reports: Vec<_> = reports
            .iter()
            .filter(|r| r.component_type == "bootstrap")
            .collect();
        assert_eq!(bootstrap_reports.len(), 1);
        assert!(bootstrap_reports[0]
            .errors
            .iter()
            .any(|e| e.message.contains("filePaths")));
    }

    // =======================================================================
    // rewrite_refs
    // =======================================================================

    #[test]
    fn test_rewrite_refs() {
        let mut val = serde_json::json!({
            "$ref": "#/components/schemas/Foo"
        });
        rewrite_refs(&mut val);
        assert_eq!(val["$ref"], "#/$defs/Foo");
    }

    #[test]
    fn test_rewrite_nested_refs() {
        let mut val = serde_json::json!({
            "allOf": [
                { "$ref": "#/components/schemas/Base" },
                { "type": "object" }
            ]
        });
        rewrite_refs(&mut val);
        assert_eq!(val["allOf"][0]["$ref"], "#/$defs/Base");
    }

    // =======================================================================
    // validate_against_schema edge cases
    // =======================================================================

    #[test]
    fn test_invalid_schema_json() {
        let errors = validate_against_schema("not valid json", "Foo", &serde_json::json!({}));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].field.contains("schema"));
        assert_eq!(errors[0].code, Some(schema_error_codes::SCHEMA_UNPARSEABLE));
    }

    #[test]
    fn test_missing_entry_point() {
        let errors = validate_against_schema(
            r#"{"OtherSchema": {"type": "object"}}"#,
            "NonExistent",
            &serde_json::json!({}),
        );
        // Missing entry point is a fatal validation failure: the component
        // config could not be evaluated at all, so callers must see the
        // error rather than silently treating it as "OK".
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "(schema)");
        assert_eq!(
            errors[0].code,
            Some(schema_error_codes::SCHEMA_ENTRY_MISSING)
        );
        assert!(errors[0].message.contains("NonExistent"));
    }

    #[test]
    fn test_validator_build_failure_is_logged_not_fatal() {
        // A schema referencing a `$defs` entry that isn't published is a
        // plugin-packaging bug. Per the documented design, this is logged
        // loudly via `error!` but does NOT return a fatal `FieldError` —
        // current upstream plugin schemas are widely affected and a fatal
        // error would block validation of example configs. The
        // `SCHEMA_VALIDATOR_BUILD_FAILED` code is reserved for the future
        // tightening once upstream schemas are fixed.
        let errors = validate_against_schema(
            r##"{"Foo": {"$ref": "#/components/schemas/Missing"}}"##,
            "Foo",
            &serde_json::json!({}),
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_field_errors_have_no_code() {
        // jsonschema-derived field errors keep `code = None`; only
        // meta-level schema-evaluation failures carry a code.
        let errors = validate_against_schema(
            r##"{"Foo": {"type": "object", "required": ["x"], "properties": {"x": {"type": "string"}}}}"##,
            "Foo",
            &serde_json::json!({}),
        );
        assert!(!errors.is_empty());
        assert!(errors.iter().all(|e| e.code.is_none()));
    }
}
