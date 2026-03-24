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

//! Plugin-aware configuration validation.
//!
//! This module provides validation that goes beyond YAML structure checking:
//! - Extracting plugin requirements from config
//! - Checking plugin availability in the registry
//! - Walking env var references and reporting missing ones
//! - Validating component configs against plugin OpenAPI schemas
//!
//! The main entry point is [`validate_with_plugins`], which runs all validation
//! steps and returns a comprehensive [`FullValidationResult`].

use log::warn;
use serde_json::Value;
use std::path::Path;

use crate::config::types::DrasiServerConfig;
use crate::plugin_registry::PluginRegistry;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A required plugin extracted from the configuration.
#[derive(Debug, Clone)]
pub struct PluginRequirement {
    /// Plugin category: "source", "reaction", or "bootstrap".
    pub category: String,
    /// Plugin kind identifier (e.g. "postgres", "http").
    pub kind: String,
    /// Human-readable back-reference (e.g. "source 'postgres-stocks'").
    pub referenced_by: String,
}

/// A plugin that the configuration requires but is not available.
#[derive(Debug, Clone)]
pub struct MissingPlugin {
    pub requirement: PluginRequirement,
    /// Kinds that *are* registered for this category.
    pub available_kinds: Vec<String>,
}

/// A warning about an unresolvable environment variable reference.
#[derive(Debug, Clone)]
pub struct ReferenceWarning {
    /// Config path (e.g. "sources['postgres-stocks'].password").
    pub path: String,
    /// The environment variable name that was referenced.
    pub var_name: String,
    /// Human-readable message.
    pub message: String,
}

/// A single field-level config validation error.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// Report for a single component's config validation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentValidationReport {
    /// "source", "reaction", or "bootstrap".
    pub component_type: String,
    pub component_id: String,
    pub plugin_kind: String,
    pub errors: Vec<FieldError>,
}

/// Full validation result from [`validate_with_plugins`].
#[derive(Debug)]
pub struct FullValidationResult {
    pub env_warnings: Vec<ReferenceWarning>,
    pub missing_plugins: Vec<MissingPlugin>,
    pub config_errors: Vec<ComponentValidationReport>,
    pub plugins_loaded: usize,
    /// `true` when no plugins directory was found or it was empty.
    pub plugins_not_loaded: bool,
}

impl FullValidationResult {
    /// Returns `true` when there are hard errors (config errors or missing
    /// env vars without defaults).
    pub fn has_errors(&self) -> bool {
        !self.env_warnings.is_empty() || !self.config_errors.is_empty()
    }
}

// ---------------------------------------------------------------------------
// R2: Extract plugin requirements
// ---------------------------------------------------------------------------

/// Walk all sources, reactions, and bootstrap providers in a config and return
/// the set of plugin kinds they depend on.
pub fn extract_plugin_requirements(config: &DrasiServerConfig) -> Vec<PluginRequirement> {
    let mut requirements = Vec::new();

    let all_sources = collect_all_sources(config);
    let all_reactions = collect_all_reactions(config);

    for src in &all_sources {
        requirements.push(PluginRequirement {
            category: "source".to_string(),
            kind: src.kind.clone(),
            referenced_by: format!("source '{}'", src.id),
        });

        if let Some(bp) = &src.bootstrap_provider {
            requirements.push(PluginRequirement {
                category: "bootstrap".to_string(),
                kind: bp.kind.clone(),
                referenced_by: format!("source '{}' bootstrapProvider", src.id),
            });
        }
    }

    for rxn in &all_reactions {
        requirements.push(PluginRequirement {
            category: "reaction".to_string(),
            kind: rxn.kind.clone(),
            referenced_by: format!("reaction '{}'", rxn.id),
        });
    }

    requirements
}

// ---------------------------------------------------------------------------
// R2: Check plugin availability
// ---------------------------------------------------------------------------

/// For each requirement, check whether the plugin is registered.
///
/// Returns `(found, missing)`.
pub fn check_plugin_availability(
    requirements: &[PluginRequirement],
    registry: &PluginRegistry,
) -> (Vec<PluginRequirement>, Vec<MissingPlugin>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();

    for req in requirements {
        let available = match req.category.as_str() {
            "source" => registry.get_source(&req.kind).is_some(),
            "reaction" => registry.get_reaction(&req.kind).is_some(),
            "bootstrap" => registry.get_bootstrapper(&req.kind).is_some(),
            _ => false,
        };

        if available {
            found.push(req.clone());
        } else {
            let available_kinds = match req.category.as_str() {
                "source" => registry
                    .source_kinds()
                    .into_iter()
                    .map(String::from)
                    .collect(),
                "reaction" => registry
                    .reaction_kinds()
                    .into_iter()
                    .map(String::from)
                    .collect(),
                "bootstrap" => registry
                    .bootstrapper_kinds()
                    .into_iter()
                    .map(String::from)
                    .collect(),
                _ => Vec::new(),
            };
            missing.push(MissingPlugin {
                requirement: req.clone(),
                available_kinds,
            });
        }
    }

    (found, missing)
}

// ---------------------------------------------------------------------------
// R4: Env var reference walking
// ---------------------------------------------------------------------------

/// Walk config JSON values looking for env var references that cannot be
/// resolved. Returns warnings for each missing env var that has no default.
pub fn check_config_references(config: &DrasiServerConfig) -> Vec<ReferenceWarning> {
    let mut warnings = Vec::new();

    let all_sources = collect_all_sources(config);
    let all_reactions = collect_all_reactions(config);

    for src in &all_sources {
        let prefix = format!("sources['{}']", src.id);
        walk_json_env_refs(&src.config, &prefix, &mut warnings);
        if let Some(bp) = &src.bootstrap_provider {
            let bp_prefix = format!("{prefix}.bootstrapProvider");
            walk_json_env_refs(&bp.config, &bp_prefix, &mut warnings);
        }
    }

    for rxn in &all_reactions {
        let prefix = format!("reactions['{}']", rxn.id);
        walk_json_env_refs(&rxn.config, &prefix, &mut warnings);
    }

    warnings
}

/// Recursively walk a JSON value and check for env var patterns.
///
/// Recognises two patterns:
/// 1. Object with `{"kind": "EnvironmentVariable", "name": "VAR", ...}`
/// 2. String values containing `${VAR}` or `${VAR:-default}`
fn walk_json_env_refs(value: &Value, path: &str, warnings: &mut Vec<ReferenceWarning>) {
    match value {
        Value::Object(map) => {
            // Check if this object IS a ConfigValue::EnvironmentVariable
            if matches!(map.get("kind"), Some(Value::String(k)) if k == "EnvironmentVariable") {
                if let Some(Value::String(var_name)) = map.get("name") {
                    let has_default = map.get("default").is_some_and(|d| !d.is_null());
                    if !has_default && std::env::var(var_name).is_err() {
                        warnings.push(ReferenceWarning {
                            path: path.to_string(),
                            var_name: var_name.clone(),
                            message: format!(
                                "env var '{var_name}' not found and no default provided"
                            ),
                        });
                    }
                }
                return; // Don't recurse into the ConfigValue fields
            }
            // Otherwise recurse into each field
            for (key, val) in map {
                let child = format!("{path}.{key}");
                walk_json_env_refs(val, &child, warnings);
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let child = format!("{path}[{i}]");
                walk_json_env_refs(val, &child, warnings);
            }
        }
        Value::String(s) => {
            check_handlebars_env_refs(s, path, warnings);
        }
        _ => {}
    }
}

/// Check a string for Handlebars-style `${VAR}` or `${VAR:-default}` patterns.
///
/// Skips `${{` which is a Handlebars template expression, not an env var ref.
fn check_handlebars_env_refs(s: &str, path: &str, warnings: &mut Vec<ReferenceWarning>) {
    let mut remaining = s;
    while let Some(start) = remaining.find("${") {
        let after = &remaining[start + 2..];
        // Skip `${{...}}` — that's a Handlebars template expression, not an env var
        if after.starts_with('{') {
            remaining = after;
            continue;
        }
        if let Some(end) = after.find('}') {
            let inner = &after[..end];
            // inner is either "VAR" or "VAR:-default"
            let (var_name, has_default) = if let Some(pos) = inner.find(":-") {
                (&inner[..pos], true)
            } else {
                (inner, false)
            };
            // Only flag as env var ref if the name looks like a valid identifier
            let looks_like_var = !var_name.is_empty()
                && var_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if looks_like_var && !has_default && std::env::var(var_name).is_err() {
                warnings.push(ReferenceWarning {
                    path: path.to_string(),
                    var_name: var_name.to_string(),
                    message: format!("env var '{var_name}' not found and no default provided"),
                });
            }
            remaining = &after[end + 1..];
        } else {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// R1/R3: Schema-based config validation
// ---------------------------------------------------------------------------

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
        if let Some(bp) = &src.bootstrap_provider {
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
            warn!("Could not parse plugin schema JSON");
            return vec![FieldError {
                field: "(schema)".to_string(),
                message: "Could not parse plugin schema — skipping validation".to_string(),
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
        return Vec::new();
    }

    let schema_doc = serde_json::json!({
        "$ref": format!("#/$defs/{entry_name}"),
        "$defs": Value::Object(defs),
    });

    // Validate
    let validator = match jsonschema::validator_for(&schema_doc) {
        Ok(v) => v,
        Err(e) => {
            warn!("Could not compile JSON Schema: {e}");
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

// ---------------------------------------------------------------------------
// R1/R5: Full validation entry point
// ---------------------------------------------------------------------------

/// Run full validation: env vars + plugins + schemas.
///
/// This is the main entry point for the enhanced `validate` command. It:
/// 1. Checks env var references in source/reaction configs
/// 2. Attempts to load plugins from `plugins_dir` (if provided and exists)
/// 3. Checks plugin availability
/// 4. Validates component configs against schemas
/// 5. Returns a comprehensive [`FullValidationResult`]
///
/// Gracefully degrades when plugins aren't available.
pub fn validate_with_plugins(
    config: &DrasiServerConfig,
    plugins_dir: Option<&Path>,
) -> FullValidationResult {
    // 1. Check env var references
    let env_warnings = check_config_references(config);

    // 2. Load plugins
    let mut registry = PluginRegistry::new();
    crate::server::register_core_plugins(&mut registry);

    let mut plugins_loaded: usize = 0;
    let mut plugins_not_loaded = true;

    if let Some(dir) = plugins_dir {
        if dir.exists() {
            match crate::dynamic_loading::load_plugins(dir, &mut registry, None, None) {
                Ok(stats) => {
                    plugins_loaded = stats.plugins_loaded;
                }
                Err(e) => {
                    warn!("Failed to load plugins from {}: {e}", dir.display());
                }
            }
            // Even if no cdylib plugins loaded, core plugins are always there
            plugins_not_loaded = false;
        }
    }

    // 3. Check plugin availability
    let requirements = extract_plugin_requirements(config);
    let (_found, missing_plugins) = check_plugin_availability(&requirements, &registry);

    // 4. Validate component configs against schemas
    let config_errors = validate_component_configs(config, &registry);

    FullValidationResult {
        env_warnings,
        missing_plugins,
        config_errors,
        plugins_loaded,
        plugins_not_loaded,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect all sources from config (top-level + instances).
fn collect_all_sources(config: &DrasiServerConfig) -> Vec<crate::api::models::SourceConfig> {
    let mut sources = config.sources.clone();
    for inst in &config.instances {
        sources.extend(inst.sources.clone());
    }
    sources
}

/// Collect all reactions from config (top-level + instances).
fn collect_all_reactions(config: &DrasiServerConfig) -> Vec<crate::api::models::ReactionConfig> {
    let mut reactions = config.reactions.clone();
    for inst in &config.instances {
        reactions.extend(inst.reactions.clone());
    }
    reactions
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{BootstrapProviderConfig, ReactionConfig, SourceConfig};
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
    // Helper: build a config with sources/reactions
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
            config: serde_json::json!({}),
        }
    }

    fn source_with_bootstrap(kind: &str, id: &str, bp_kind: &str) -> SourceConfig {
        SourceConfig {
            kind: kind.to_string(),
            id: id.to_string(),
            auto_start: true,
            bootstrap_provider: Some(BootstrapProviderConfig {
                kind: bp_kind.to_string(),
                config: serde_json::json!({}),
            }),
            config: serde_json::json!({}),
        }
    }

    fn reaction(kind: &str, id: &str) -> ReactionConfig {
        ReactionConfig {
            kind: kind.to_string(),
            id: id.to_string(),
            queries: vec![],
            auto_start: true,
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
    // extract_plugin_requirements
    // =======================================================================

    #[test]
    fn test_extract_empty_config() {
        let config = DrasiServerConfig::default();
        assert!(extract_plugin_requirements(&config).is_empty());
    }

    #[test]
    fn test_extract_sources_reactions_bootstrap() {
        let config = config_with(
            vec![
                source("postgres", "pg1"),
                source_with_bootstrap("http", "http1", "scriptfile"),
            ],
            vec![reaction("log", "log1")],
        );

        let reqs = extract_plugin_requirements(&config);
        assert_eq!(reqs.len(), 4); // pg source, http source, scriptfile bootstrap, log reaction

        let cats: Vec<(&str, &str)> = reqs
            .iter()
            .map(|r| (r.category.as_str(), r.kind.as_str()))
            .collect();
        assert!(cats.contains(&("source", "postgres")));
        assert!(cats.contains(&("source", "http")));
        assert!(cats.contains(&("bootstrap", "scriptfile")));
        assert!(cats.contains(&("reaction", "log")));
    }

    // =======================================================================
    // check_plugin_availability
    // =======================================================================

    #[test]
    fn test_all_found() {
        let registry = mock_registry();
        let reqs = vec![
            PluginRequirement {
                category: "source".to_string(),
                kind: "postgres".to_string(),
                referenced_by: "source 'pg1'".to_string(),
            },
            PluginRequirement {
                category: "reaction".to_string(),
                kind: "log".to_string(),
                referenced_by: "reaction 'log1'".to_string(),
            },
        ];

        let (found, missing) = check_plugin_availability(&reqs, &registry);
        assert_eq!(found.len(), 2);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_some_missing() {
        let registry = mock_registry();
        let reqs = vec![
            PluginRequirement {
                category: "source".to_string(),
                kind: "postgres".to_string(),
                referenced_by: "source 'pg1'".to_string(),
            },
            PluginRequirement {
                category: "source".to_string(),
                kind: "grpc".to_string(),
                referenced_by: "source 'grpc1'".to_string(),
            },
        ];

        let (found, missing) = check_plugin_availability(&reqs, &registry);
        assert_eq!(found.len(), 1);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].requirement.kind, "grpc");
        assert!(missing[0].available_kinds.contains(&"postgres".to_string()));
    }

    #[test]
    fn test_all_missing() {
        let registry = PluginRegistry::new();
        let reqs = vec![PluginRequirement {
            category: "source".to_string(),
            kind: "postgres".to_string(),
            referenced_by: "source 'pg1'".to_string(),
        }];

        let (found, missing) = check_plugin_availability(&reqs, &registry);
        assert!(found.is_empty());
        assert_eq!(missing.len(), 1);
    }

    // =======================================================================
    // check_config_references
    // =======================================================================

    #[test]
    fn test_no_env_vars() {
        let config = config_with(
            vec![SourceConfig {
                kind: "mock".to_string(),
                id: "s1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({"host": "localhost"}),
            }],
            vec![],
        );

        assert!(check_config_references(&config).is_empty());
    }

    #[test]
    fn test_env_var_present() {
        // Set an env var that our test config references
        // SAFETY: set_var in a single-threaded test context is safe.
        unsafe {
            std::env::set_var("DRASI_TEST_PRESENT_VAR", "value");
        }

        let config = config_with(
            vec![SourceConfig {
                kind: "mock".to_string(),
                id: "s1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "host": {
                        "kind": "EnvironmentVariable",
                        "name": "DRASI_TEST_PRESENT_VAR"
                    }
                }),
            }],
            vec![],
        );

        let warnings = check_config_references(&config);
        assert!(warnings.is_empty());

        unsafe {
            std::env::remove_var("DRASI_TEST_PRESENT_VAR");
        }
    }

    #[test]
    fn test_env_var_missing() {
        // Ensure this var does NOT exist
        std::env::remove_var("DRASI_TEST_NONEXISTENT_XYZ");

        let config = config_with(
            vec![SourceConfig {
                kind: "mock".to_string(),
                id: "s1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "password": {
                        "kind": "EnvironmentVariable",
                        "name": "DRASI_TEST_NONEXISTENT_XYZ"
                    }
                }),
            }],
            vec![],
        );

        let warnings = check_config_references(&config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].var_name, "DRASI_TEST_NONEXISTENT_XYZ");
        assert!(warnings[0].path.contains("password"));
    }

    #[test]
    fn test_env_var_with_default() {
        std::env::remove_var("DRASI_TEST_DEFAULTED_VAR");

        let config = config_with(
            vec![SourceConfig {
                kind: "mock".to_string(),
                id: "s1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "host": {
                        "kind": "EnvironmentVariable",
                        "name": "DRASI_TEST_DEFAULTED_VAR",
                        "default": "localhost"
                    }
                }),
            }],
            vec![],
        );

        let warnings = check_config_references(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_handlebars_env_ref_missing() {
        std::env::remove_var("DRASI_TEST_HBS_MISSING");

        let config = config_with(
            vec![SourceConfig {
                kind: "mock".to_string(),
                id: "s1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "password": "${DRASI_TEST_HBS_MISSING}"
                }),
            }],
            vec![],
        );

        let warnings = check_config_references(&config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].var_name, "DRASI_TEST_HBS_MISSING");
    }

    #[test]
    fn test_handlebars_env_ref_with_default() {
        std::env::remove_var("DRASI_TEST_HBS_DEFAULT");

        let config = config_with(
            vec![SourceConfig {
                kind: "mock".to_string(),
                id: "s1".to_string(),
                auto_start: true,
                bootstrap_provider: None,
                config: serde_json::json!({
                    "password": "${DRASI_TEST_HBS_DEFAULT:-secret}"
                }),
            }],
            vec![],
        );

        let warnings = check_config_references(&config);
        assert!(warnings.is_empty());
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
                bootstrap_provider: Some(BootstrapProviderConfig {
                    kind: "scriptfile".to_string(),
                    config: serde_json::json!({
                        "filePaths": ["/data/file1.jsonl"]
                    }),
                }),
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
                bootstrap_provider: Some(BootstrapProviderConfig {
                    kind: "scriptfile".to_string(),
                    config: serde_json::json!({}), // missing filePaths
                }),
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
    // validate_with_plugins (integration)
    // =======================================================================

    #[test]
    fn test_validate_with_plugins_no_dir() {
        let config = config_with(vec![source("mock", "s1")], vec![]);

        let result = validate_with_plugins(&config, None);
        assert!(result.plugins_not_loaded);
        assert!(result.env_warnings.is_empty());
    }

    #[test]
    fn test_validate_with_plugins_nonexistent_dir() {
        let config = config_with(vec![source("mock", "s1")], vec![]);

        let result = validate_with_plugins(&config, Some(Path::new("/nonexistent/dir")));
        assert!(result.plugins_not_loaded);
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
    }

    #[test]
    fn test_missing_entry_point() {
        let errors = validate_against_schema(
            r#"{"OtherSchema": {"type": "object"}}"#,
            "NonExistent",
            &serde_json::json!({}),
        );
        assert!(errors.is_empty()); // Graceful: no errors, just skipped
    }
}
