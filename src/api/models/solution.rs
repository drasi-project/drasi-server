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

//! Solution template types for deploying collections of Drasi components.
//!
//! Solution templates are YAML files containing sources, queries, and reactions
//! that can be deployed together with user-provided variable values.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Metadata for a solution template.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SolutionTemplateMetadata {
    /// Human-readable name of the solution
    pub name: String,

    /// Description of what this solution does
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Version of the solution template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Author of the solution template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// License for the solution template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Default instance ID to use when deploying
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_instance_id: Option<String>,
}

/// A variable extracted from a solution template.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SolutionVariable {
    /// The variable name (without ${ })
    pub name: String,

    /// The default value, if specified with :- syntax
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    /// Whether this variable is required (no default provided)
    pub required: bool,

    /// Description extracted from YAML comment on the same line
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// List of component IDs that use this variable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub used_by: Vec<String>,
}

/// Summary of a solution template for list views.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SolutionTemplateSummary {
    /// Template ID (derived from filename)
    pub id: String,

    /// Template metadata
    #[serde(flatten)]
    pub metadata: SolutionTemplateMetadata,

    /// Number of sources in the template
    pub source_count: usize,

    /// Number of queries in the template
    pub query_count: usize,

    /// Number of reactions in the template
    pub reaction_count: usize,
}

/// Detailed view of a solution template.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SolutionTemplateDetail {
    /// Template ID (derived from filename)
    pub id: String,

    /// Template metadata
    #[serde(flatten)]
    pub metadata: SolutionTemplateMetadata,

    /// Variables that can be configured when deploying
    pub variables: Vec<SolutionVariable>,

    /// List of source IDs in the template
    pub source_ids: Vec<String>,

    /// List of query IDs in the template
    pub query_ids: Vec<String>,

    /// List of reaction IDs in the template
    pub reaction_ids: Vec<String>,
}

/// Request to deploy a solution template.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SolutionDeployRequest {
    /// Template ID to deploy (mutually exclusive with yaml)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,

    /// Raw YAML content to deploy (mutually exclusive with template_id)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaml: Option<String>,

    /// Variable values to substitute in the template
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

impl SolutionDeployRequest {
    /// Validates the request, ensuring exactly one of template_id or yaml is provided.
    pub fn validate(&self) -> Result<(), &'static str> {
        match (&self.template_id, &self.yaml) {
            (Some(_), Some(_)) => Err("Cannot specify both templateId and yaml"),
            (None, None) => Err("Must specify either templateId or yaml"),
            _ => Ok(()),
        }
    }
}

/// Request to create a new solution template from existing components.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSolutionTemplateRequest {
    /// Unique ID for the template (used as filename)
    pub id: String,

    /// Human-readable name of the solution
    pub name: String,

    /// Description of what this solution does
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Version of the solution template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Author of the solution template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// License for the solution template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// IDs of sources to include in the template
    #[serde(default)]
    pub source_ids: Vec<String>,

    /// IDs of queries to include in the template
    #[serde(default)]
    pub query_ids: Vec<String>,

    /// IDs of reactions to include in the template
    #[serde(default)]
    pub reaction_ids: Vec<String>,
}

impl CreateSolutionTemplateRequest {
    /// Validates the request.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty() {
            return Err("Template ID is required");
        }
        if self.name.is_empty() {
            return Err("Template name is required");
        }
        if self.source_ids.is_empty() && self.query_ids.is_empty() && self.reaction_ids.is_empty() {
            return Err("At least one component must be selected");
        }
        Ok(())
    }
}

/// Response from creating a solution template.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSolutionTemplateResponse {
    /// Whether the creation was successful
    pub success: bool,

    /// The ID of the created template
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,

    /// Error message if creation failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response from deploying a solution template.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SolutionDeployResponse {
    /// Whether the deployment was successful
    pub success: bool,

    /// IDs of created sources
    pub sources_created: Vec<String>,

    /// IDs of created queries
    pub queries_created: Vec<String>,

    /// IDs of created reactions
    pub reactions_created: Vec<String>,

    /// IDs of components that were started
    pub components_started: Vec<String>,

    /// Any errors that occurred during deployment
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<SolutionDeployError>,
}

impl SolutionDeployResponse {
    /// Creates a successful response with no errors.
    pub fn success(
        sources: Vec<String>,
        queries: Vec<String>,
        reactions: Vec<String>,
        started: Vec<String>,
    ) -> Self {
        Self {
            success: true,
            sources_created: sources,
            queries_created: queries,
            reactions_created: reactions,
            components_started: started,
            errors: Vec::new(),
        }
    }

    /// Creates a failed response with errors.
    pub fn failed(errors: Vec<SolutionDeployError>) -> Self {
        Self {
            success: false,
            sources_created: Vec::new(),
            queries_created: Vec::new(),
            reactions_created: Vec::new(),
            components_started: Vec::new(),
            errors,
        }
    }
}

/// An error that occurred during solution deployment.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SolutionDeployError {
    /// The phase where the error occurred
    pub phase: DeployPhase,

    /// The component type that failed (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_type: Option<String>,

    /// The component ID that failed (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,

    /// Error message
    pub message: String,
}

impl SolutionDeployError {
    /// Creates a validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            phase: DeployPhase::Validation,
            component_type: None,
            component_id: None,
            message: message.into(),
        }
    }

    /// Creates a creation error for a specific component.
    pub fn creation(
        component_type: impl Into<String>,
        component_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase: DeployPhase::Creation,
            component_type: Some(component_type.into()),
            component_id: Some(component_id.into()),
            message: message.into(),
        }
    }

    /// Creates a start error for a specific component.
    pub fn start(
        component_type: impl Into<String>,
        component_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase: DeployPhase::Start,
            component_type: Some(component_type.into()),
            component_id: Some(component_id.into()),
            message: message.into(),
        }
    }
}

/// The phase of deployment where an error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum DeployPhase {
    /// Error during request validation
    Validation,
    /// Error during component creation
    Creation,
    /// Error during component start
    Start,
}

/// Extracts variables from a YAML string.
///
/// Finds all `${VAR}` and `${VAR:-default}` patterns and returns
/// a list of unique variables with their defaults. Also extracts:
/// - YAML comments on the same line as the variable (used as description)
/// - Component IDs that reference each variable (from sources/queries/reactions sections)
///
/// # Example
/// ```
/// use drasi_server::api::models::solution::extract_variables;
///
/// let yaml = r#"
/// host: "${HOST:-localhost}"  # Database host address
/// port: "${PORT}"
/// db: "${DB:-mydb}"
/// "#;
///
/// let vars = extract_variables(yaml);
/// assert_eq!(vars.len(), 3);
/// // Variables are sorted by name: DB, HOST, PORT
/// let host_var = vars.iter().find(|v| v.name == "HOST").unwrap();
/// assert_eq!(host_var.description, Some("Database host address".to_string()));
/// ```
pub fn extract_variables(yaml: &str) -> Vec<SolutionVariable> {
    use regex::Regex;
    use std::collections::HashMap;

    // Match ${VAR} or ${VAR:-default}
    // Group 1: variable name
    // Group 3: default value (optional, after :-)
    let var_re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?:(:-)((?:[^}]|\\\})*))?\}")
        .expect("Invalid regex pattern for variable extraction");

    // Track variables: name -> (default, required, description, used_by)
    let mut var_map: HashMap<String, (Option<String>, bool, Option<String>, Vec<String>)> =
        HashMap::new();

    // First pass: find current component context and extract variables with comments
    let mut current_section: Option<&str> = None; // "sources", "queries", "reactions"
    let mut current_component_id: Option<String> = None;

    for line in yaml.lines() {
        let trimmed = line.trim();

        // Track which section we're in
        if trimmed.starts_with("sources:") {
            current_section = Some("sources");
            current_component_id = None;
        } else if trimmed.starts_with("queries:") {
            current_section = Some("queries");
            current_component_id = None;
        } else if trimmed.starts_with("reactions:") {
            current_section = Some("reactions");
            current_component_id = None;
        } else if current_section.is_some() {
            // Look for component id within a section
            // Match patterns like "id: my-source" or "- id: my-source"
            if let Some(id_match) = extract_component_id(trimmed) {
                current_component_id = Some(id_match);
            }
        }

        // Extract variables from this line
        for caps in var_re.captures_iter(line) {
            let name = caps
                .get(1)
                .expect("Regex group 1 (variable name) must exist")
                .as_str()
                .to_string();

            let default = caps.get(3).map(|m| m.as_str().to_string());
            let required = default.is_none();

            // Extract comment from the line (after #)
            let description = extract_line_comment(line);

            let entry = var_map
                .entry(name)
                .or_insert_with(|| (default.clone(), required, description.clone(), Vec::new()));

            // Update description if we found one and don't have one yet
            if entry.2.is_none() && description.is_some() {
                entry.2 = description;
            }

            // Add component ID to used_by if we're in a component context
            if let Some(ref comp_id) = current_component_id {
                if !entry.3.contains(comp_id) {
                    entry.3.push(comp_id.clone());
                }
            }
        }
    }

    // Convert map to vector
    let mut variables: Vec<SolutionVariable> = var_map
        .into_iter()
        .map(
            |(name, (default, required, description, used_by))| SolutionVariable {
                name,
                default,
                required,
                description,
                used_by,
            },
        )
        .collect();

    // Sort by name for consistent ordering
    variables.sort_by(|a, b| a.name.cmp(&b.name));

    variables
}

/// Extract component ID from a YAML line
fn extract_component_id(line: &str) -> Option<String> {
    // Match "id: value" or "- id: value" patterns
    let trimmed = line.trim().trim_start_matches('-').trim();
    if trimmed.starts_with("id:") {
        let value = trimmed.trim_start_matches("id:").trim();
        // Remove quotes if present
        let value = value.trim_matches('"').trim_matches('\'');
        if !value.is_empty() && !value.contains("${") {
            return Some(value.to_string());
        }
    }
    None
}

/// Extract a comment from the end of a YAML line
fn extract_line_comment(line: &str) -> Option<String> {
    // Find # that's not inside a string
    // Simple heuristic: find the last # that appears after any quotes are closed
    if let Some(hash_pos) = line.rfind('#') {
        let before_hash = &line[..hash_pos];
        // Count quotes to check if we're inside a string
        let double_quotes = before_hash.matches('"').count();
        let single_quotes = before_hash.matches('\'').count();

        // If quotes are balanced, the # is a comment
        if double_quotes % 2 == 0 && single_quotes % 2 == 0 {
            let comment = line[hash_pos + 1..].trim();
            if !comment.is_empty() {
                return Some(comment.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_variables_simple() {
        let yaml = r#"host: "${HOST}""#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "HOST");
        assert!(vars[0].required);
        assert!(vars[0].default.is_none());
    }

    #[test]
    fn test_extract_variables_with_default() {
        let yaml = r#"host: "${HOST:-localhost}""#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "HOST");
        assert!(!vars[0].required);
        assert_eq!(vars[0].default, Some("localhost".to_string()));
    }

    #[test]
    fn test_extract_variables_multiple() {
        let yaml = r#"
host: "${HOST:-localhost}"
port: "${PORT:-8080}"
db: "${DATABASE}"
"#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 3);

        let host = vars.iter().find(|v| v.name == "HOST").unwrap();
        assert_eq!(host.default, Some("localhost".to_string()));
        assert!(!host.required);

        let port = vars.iter().find(|v| v.name == "PORT").unwrap();
        assert_eq!(port.default, Some("8080".to_string()));
        assert!(!port.required);

        let db = vars.iter().find(|v| v.name == "DATABASE").unwrap();
        assert!(db.default.is_none());
        assert!(db.required);
    }

    #[test]
    fn test_extract_variables_deduplicates() {
        let yaml = r#"
host1: "${HOST:-localhost}"
host2: "${HOST:-different}"
"#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "HOST");
        // First occurrence wins
        assert_eq!(vars[0].default, Some("localhost".to_string()));
    }

    #[test]
    fn test_extract_variables_empty_default() {
        let yaml = r#"value: "${VAR:-}""#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "VAR");
        assert_eq!(vars[0].default, Some("".to_string()));
        assert!(!vars[0].required);
    }

    #[test]
    fn test_extract_variables_in_query() {
        let yaml = r#"
query: "MATCH (s:Sensor) WHERE s.temp > ${THRESHOLD:-75} RETURN s"
"#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "THRESHOLD");
        assert_eq!(vars[0].default, Some("75".to_string()));
    }

    #[test]
    fn test_extract_variables_complex_default() {
        let yaml = r#"url: "${URL:-http://localhost:8080/api}""#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "URL");
        assert_eq!(
            vars[0].default,
            Some("http://localhost:8080/api".to_string())
        );
    }

    #[test]
    fn test_extract_variables_none() {
        let yaml = r#"
host: "localhost"
port: 8080
"#;
        let vars = extract_variables(yaml);
        assert!(vars.is_empty());
    }

    #[test]
    fn test_deploy_request_validate_both() {
        let req = SolutionDeployRequest {
            template_id: Some("test".to_string()),
            yaml: Some("yaml content".to_string()),
            variables: HashMap::new(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_deploy_request_validate_neither() {
        let req = SolutionDeployRequest {
            template_id: None,
            yaml: None,
            variables: HashMap::new(),
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_deploy_request_validate_template_id_only() {
        let req = SolutionDeployRequest {
            template_id: Some("test".to_string()),
            yaml: None,
            variables: HashMap::new(),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_deploy_request_validate_yaml_only() {
        let req = SolutionDeployRequest {
            template_id: None,
            yaml: Some("yaml content".to_string()),
            variables: HashMap::new(),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_deploy_response_success() {
        let resp = SolutionDeployResponse::success(
            vec!["s1".to_string()],
            vec!["q1".to_string()],
            vec!["r1".to_string()],
            vec!["s1".to_string(), "q1".to_string(), "r1".to_string()],
        );
        assert!(resp.success);
        assert_eq!(resp.sources_created, vec!["s1"]);
        assert_eq!(resp.queries_created, vec!["q1"]);
        assert_eq!(resp.reactions_created, vec!["r1"]);
        assert!(resp.errors.is_empty());
    }

    #[test]
    fn test_deploy_response_failed() {
        let resp = SolutionDeployResponse::failed(vec![SolutionDeployError::validation(
            "Missing required variable",
        )]);
        assert!(!resp.success);
        assert!(resp.sources_created.is_empty());
        assert_eq!(resp.errors.len(), 1);
    }

    #[test]
    fn test_deploy_error_types() {
        let validation = SolutionDeployError::validation("bad input");
        assert_eq!(validation.phase, DeployPhase::Validation);
        assert!(validation.component_type.is_none());

        let creation = SolutionDeployError::creation("source", "my-source", "already exists");
        assert_eq!(creation.phase, DeployPhase::Creation);
        assert_eq!(creation.component_type, Some("source".to_string()));
        assert_eq!(creation.component_id, Some("my-source".to_string()));

        let start = SolutionDeployError::start("query", "my-query", "failed to start");
        assert_eq!(start.phase, DeployPhase::Start);
        assert_eq!(start.component_type, Some("query".to_string()));
    }

    #[test]
    fn test_metadata_serialization() {
        let metadata = SolutionTemplateMetadata {
            name: "Test Solution".to_string(),
            description: Some("A test".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            license: None,
            default_instance_id: Some("test-instance".to_string()),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("\"name\":\"Test Solution\""));
        assert!(json.contains("\"defaultInstanceId\":\"test-instance\""));
        // author and license should be omitted
        assert!(!json.contains("author"));
        assert!(!json.contains("license"));
    }

    #[test]
    fn test_extract_variables_with_comments() {
        let yaml = r#"
sources:
  - kind: mock
    id: my-source
    interval: "${INTERVAL:-1000}"  # Polling interval in milliseconds
"#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        let interval = &vars[0];
        assert_eq!(interval.name, "INTERVAL");
        assert_eq!(
            interval.description,
            Some("Polling interval in milliseconds".to_string())
        );
        assert_eq!(interval.used_by, vec!["my-source".to_string()]);
    }

    #[test]
    fn test_extract_variables_used_by_multiple_components() {
        let yaml = r#"
sources:
  - kind: http
    id: source-1
    url: "${API_URL}"
  - kind: http
    id: source-2
    url: "${API_URL}"
reactions:
  - kind: http
    id: reaction-1
    url: "${API_URL}"
"#;
        let vars = extract_variables(yaml);
        assert_eq!(vars.len(), 1);
        let api_url = &vars[0];
        assert_eq!(api_url.name, "API_URL");
        assert!(api_url.used_by.contains(&"source-1".to_string()));
        assert!(api_url.used_by.contains(&"source-2".to_string()));
        assert!(api_url.used_by.contains(&"reaction-1".to_string()));
    }

    #[test]
    fn test_extract_line_comment() {
        assert_eq!(
            extract_line_comment("value: 123  # This is a comment"),
            Some("This is a comment".to_string())
        );
        assert_eq!(
            extract_line_comment("value: \"string with # inside\""),
            None // The # is inside quotes
        );
        assert_eq!(extract_line_comment("value: 123"), None);
    }

    #[test]
    fn test_create_template_request_validate_empty_id() {
        let req = CreateSolutionTemplateRequest {
            id: "".to_string(),
            name: "Test Template".to_string(),
            description: None,
            version: None,
            author: None,
            license: None,
            source_ids: vec!["source-1".to_string()],
            query_ids: vec![],
            reaction_ids: vec![],
        };
        assert!(req.validate().is_err());
        assert_eq!(req.validate().unwrap_err(), "Template ID is required");
    }

    #[test]
    fn test_create_template_request_validate_empty_name() {
        let req = CreateSolutionTemplateRequest {
            id: "my-template".to_string(),
            name: "".to_string(),
            description: None,
            version: None,
            author: None,
            license: None,
            source_ids: vec!["source-1".to_string()],
            query_ids: vec![],
            reaction_ids: vec![],
        };
        assert!(req.validate().is_err());
        assert_eq!(req.validate().unwrap_err(), "Template name is required");
    }

    #[test]
    fn test_create_template_request_validate_no_components() {
        let req = CreateSolutionTemplateRequest {
            id: "my-template".to_string(),
            name: "Test Template".to_string(),
            description: None,
            version: None,
            author: None,
            license: None,
            source_ids: vec![],
            query_ids: vec![],
            reaction_ids: vec![],
        };
        assert!(req.validate().is_err());
        assert_eq!(
            req.validate().unwrap_err(),
            "At least one component must be selected"
        );
    }

    #[test]
    fn test_create_template_request_validate_with_sources_only() {
        let req = CreateSolutionTemplateRequest {
            id: "my-template".to_string(),
            name: "Test Template".to_string(),
            description: Some("A description".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Author".to_string()),
            license: Some("MIT".to_string()),
            source_ids: vec!["source-1".to_string(), "source-2".to_string()],
            query_ids: vec![],
            reaction_ids: vec![],
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_create_template_request_validate_with_queries_only() {
        let req = CreateSolutionTemplateRequest {
            id: "my-template".to_string(),
            name: "Test Template".to_string(),
            description: None,
            version: None,
            author: None,
            license: None,
            source_ids: vec![],
            query_ids: vec!["query-1".to_string()],
            reaction_ids: vec![],
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_create_template_request_validate_with_reactions_only() {
        let req = CreateSolutionTemplateRequest {
            id: "my-template".to_string(),
            name: "Test Template".to_string(),
            description: None,
            version: None,
            author: None,
            license: None,
            source_ids: vec![],
            query_ids: vec![],
            reaction_ids: vec!["reaction-1".to_string()],
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_create_template_request_validate_with_all_components() {
        let req = CreateSolutionTemplateRequest {
            id: "full-template".to_string(),
            name: "Full Template".to_string(),
            description: Some("Has all component types".to_string()),
            version: Some("2.0.0".to_string()),
            author: Some("Drasi Team".to_string()),
            license: Some("Apache-2.0".to_string()),
            source_ids: vec!["s1".to_string(), "s2".to_string()],
            query_ids: vec!["q1".to_string()],
            reaction_ids: vec!["r1".to_string(), "r2".to_string()],
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_create_template_request_serialization() {
        let req = CreateSolutionTemplateRequest {
            id: "my-template".to_string(),
            name: "Test Template".to_string(),
            description: Some("Description".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            license: None,
            source_ids: vec!["source-1".to_string()],
            query_ids: vec!["query-1".to_string()],
            reaction_ids: vec![],
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"id\":\"my-template\""));
        assert!(json.contains("\"name\":\"Test Template\""));
        assert!(json.contains("\"sourceIds\":[\"source-1\"]"));
        assert!(json.contains("\"queryIds\":[\"query-1\"]"));
        assert!(json.contains("\"reactionIds\":[]"));
    }

    #[test]
    fn test_create_template_response_success() {
        let resp = CreateSolutionTemplateResponse {
            success: true,
            template_id: Some("my-template".to_string()),
            error: None,
        };

        assert!(resp.success);
        assert_eq!(resp.template_id, Some("my-template".to_string()));
        assert!(resp.error.is_none());

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"templateId\":\"my-template\""));
        assert!(!json.contains("error")); // skip_serializing_if = None
    }

    #[test]
    fn test_create_template_response_failure() {
        let resp = CreateSolutionTemplateResponse {
            success: false,
            template_id: None,
            error: Some("Source 'missing' not found".to_string()),
        };

        assert!(!resp.success);
        assert!(resp.template_id.is_none());
        assert_eq!(resp.error, Some("Source 'missing' not found".to_string()));

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"Source 'missing' not found\""));
    }
}
