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

//! Solution template handlers for listing, getting, and deploying solutions.

use axum::Json;
use std::path::Path;
use std::sync::Arc;

use crate::api::mappings::{DtoMapper, QueryConfigMapper};
use crate::api::models::solution::{
    extract_variables, CreateSolutionTemplateRequest, CreateSolutionTemplateResponse,
    DeployPhase, SolutionDeployError, SolutionDeployRequest, SolutionDeployResponse,
    SolutionTemplateDetail, SolutionTemplateMetadata, SolutionTemplateSummary,
};
use crate::api::models::{QueryConfigDto, ReactionConfig, SourceConfig};
use crate::api::shared::ApiResponse;
use crate::factories::{create_reaction, create_source};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;

/// The default solutions directory
pub const DEFAULT_SOLUTIONS_DIR: &str = "./solutions";

/// Internal structure for parsing solution template files
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolutionTemplateFile {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    default_instance_id: Option<String>,
    #[serde(default)]
    sources: Vec<serde_yaml::Value>,
    #[serde(default)]
    queries: Vec<serde_yaml::Value>,
    #[serde(default)]
    reactions: Vec<serde_yaml::Value>,
}

impl SolutionTemplateFile {
    fn to_metadata(&self) -> SolutionTemplateMetadata {
        SolutionTemplateMetadata {
            name: self.name.clone(),
            description: self.description.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            license: self.license.clone(),
            default_instance_id: self.default_instance_id.clone(),
        }
    }

    fn source_count(&self) -> usize {
        self.sources.len()
    }

    fn query_count(&self) -> usize {
        self.queries.len()
    }

    fn reaction_count(&self) -> usize {
        self.reactions.len()
    }

    fn source_ids(&self) -> Vec<String> {
        self.sources
            .iter()
            .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(String::from))
            .collect()
    }

    fn query_ids(&self) -> Vec<String> {
        self.queries
            .iter()
            .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(String::from))
            .collect()
    }

    fn reaction_ids(&self) -> Vec<String> {
        self.reactions
            .iter()
            .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(String::from))
            .collect()
    }
}

/// Read all solution templates from the specified directory.
fn read_templates_from_dir(
    solutions_dir: &Path,
) -> Result<Vec<(String, String, SolutionTemplateFile)>, String> {
    if !solutions_dir.exists() {
        return Ok(Vec::new());
    }

    if !solutions_dir.is_dir() {
        return Err(format!(
            "Solutions path '{}' is not a directory",
            solutions_dir.display()
        ));
    }

    let mut templates = Vec::new();

    let entries = std::fs::read_dir(solutions_dir).map_err(|e| {
        format!(
            "Failed to read solutions directory '{}': {}",
            solutions_dir.display(),
            e
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }

        // Extract template ID from filename
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_default();

        if id.is_empty() {
            continue;
        }

        // Read and parse the file
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!(
                    "Failed to read solution template '{}': {}",
                    path.display(),
                    e
                );
                continue;
            }
        };

        let template: SolutionTemplateFile = match serde_yaml::from_str(&content) {
            Ok(t) => t,
            Err(e) => {
                log::warn!(
                    "Failed to parse solution template '{}': {}",
                    path.display(),
                    e
                );
                continue;
            }
        };

        templates.push((id, content, template));
    }

    Ok(templates)
}

/// List all available solution templates.
pub async fn list_solutions(
    solutions_dir: Option<String>,
) -> Json<ApiResponse<Vec<SolutionTemplateSummary>>> {
    let dir = solutions_dir.as_deref().unwrap_or(DEFAULT_SOLUTIONS_DIR);
    let path = Path::new(dir);

    let templates = match read_templates_from_dir(path) {
        Ok(t) => t,
        Err(e) => return Json(ApiResponse::error(e)),
    };

    let summaries: Vec<SolutionTemplateSummary> = templates
        .into_iter()
        .map(|(id, _, template)| SolutionTemplateSummary {
            id,
            metadata: template.to_metadata(),
            source_count: template.source_count(),
            query_count: template.query_count(),
            reaction_count: template.reaction_count(),
        })
        .collect();

    Json(ApiResponse::success(summaries))
}

/// Get detailed information about a specific solution template.
pub async fn get_solution(
    solutions_dir: Option<String>,
    template_id: &str,
) -> Json<ApiResponse<SolutionTemplateDetail>> {
    let dir = solutions_dir.as_deref().unwrap_or(DEFAULT_SOLUTIONS_DIR);
    let path = Path::new(dir);

    // Try both .yaml and .yml extensions
    let yaml_path = path.join(format!("{template_id}.yaml"));
    let yml_path = path.join(format!("{template_id}.yml"));

    let (template_path, content) = if yaml_path.exists() {
        match std::fs::read_to_string(&yaml_path) {
            Ok(c) => (yaml_path, c),
            Err(e) => return Json(ApiResponse::error(format!("Failed to read template: {e}"))),
        }
    } else if yml_path.exists() {
        match std::fs::read_to_string(&yml_path) {
            Ok(c) => (yml_path, c),
            Err(e) => return Json(ApiResponse::error(format!("Failed to read template: {e}"))),
        }
    } else {
        return Json(ApiResponse::error(format!(
            "Solution template '{template_id}' not found"
        )));
    };

    let template: SolutionTemplateFile = match serde_yaml::from_str(&content) {
        Ok(t) => t,
        Err(e) => {
            return Json(ApiResponse::error(format!(
                "Failed to parse template '{}': {}",
                template_path.display(),
                e
            )))
        }
    };

    // Extract variables from the raw YAML content
    let variables = extract_variables(&content);

    let detail = SolutionTemplateDetail {
        id: template_id.to_string(),
        metadata: template.to_metadata(),
        variables,
        source_ids: template.source_ids(),
        query_ids: template.query_ids(),
        reaction_ids: template.reaction_ids(),
    };

    Json(ApiResponse::success(detail))
}

/// Create a new solution template from components in an instance.
///
/// The template is written as a YAML file to the solutions directory.
pub async fn create_solution_template(
    persistence: Option<Arc<ConfigPersistence>>,
    solutions_dir: Option<String>,
    instance_id: &str,
    request: CreateSolutionTemplateRequest,
) -> Json<ApiResponse<CreateSolutionTemplateResponse>> {
    // Validate the request
    if let Err(e) = request.validate() {
        return Json(ApiResponse::success(CreateSolutionTemplateResponse {
            success: false,
            template_id: None,
            error: Some(e.to_string()),
        }));
    }

    // Need persistence to get component configs
    let persistence = match persistence {
        Some(p) => p,
        None => {
            return Json(ApiResponse::success(CreateSolutionTemplateResponse {
                success: false,
                template_id: None,
                error: Some("Config persistence not available".to_string()),
            }));
        }
    };

    // Build the template YAML structure
    let mut sources: Vec<serde_yaml::Value> = Vec::new();
    let mut queries: Vec<serde_yaml::Value> = Vec::new();
    let mut reactions: Vec<serde_yaml::Value> = Vec::new();

    // Collect sources
    for source_id in &request.source_ids {
        if let Some(source_config) = persistence.get_source_config(instance_id, source_id).await {
            if let Ok(yaml_value) = serde_yaml::to_value(&source_config) {
                sources.push(yaml_value);
            }
        } else {
            return Json(ApiResponse::success(CreateSolutionTemplateResponse {
                success: false,
                template_id: None,
                error: Some(format!("Source '{}' not found", source_id)),
            }));
        }
    }

    // Collect queries
    for query_id in &request.query_ids {
        if let Some(query_config) = persistence.get_query_config(instance_id, query_id).await {
            if let Ok(yaml_value) = serde_yaml::to_value(&query_config) {
                queries.push(yaml_value);
            }
        } else {
            return Json(ApiResponse::success(CreateSolutionTemplateResponse {
                success: false,
                template_id: None,
                error: Some(format!("Query '{}' not found", query_id)),
            }));
        }
    }

    // Collect reactions
    for reaction_id in &request.reaction_ids {
        if let Some(reaction_config) = persistence.get_reaction_config(instance_id, reaction_id).await {
            if let Ok(yaml_value) = serde_yaml::to_value(&reaction_config) {
                reactions.push(yaml_value);
            }
        } else {
            return Json(ApiResponse::success(CreateSolutionTemplateResponse {
                success: false,
                template_id: None,
                error: Some(format!("Reaction '{}' not found", reaction_id)),
            }));
        }
    }

    // Build the template content
    let mut template_map = serde_yaml::Mapping::new();
    template_map.insert(
        serde_yaml::Value::String("name".to_string()),
        serde_yaml::Value::String(request.name.clone()),
    );
    
    if let Some(desc) = &request.description {
        template_map.insert(
            serde_yaml::Value::String("description".to_string()),
            serde_yaml::Value::String(desc.clone()),
        );
    }
    
    if let Some(ver) = &request.version {
        template_map.insert(
            serde_yaml::Value::String("version".to_string()),
            serde_yaml::Value::String(ver.clone()),
        );
    }
    
    if let Some(auth) = &request.author {
        template_map.insert(
            serde_yaml::Value::String("author".to_string()),
            serde_yaml::Value::String(auth.clone()),
        );
    }
    
    if let Some(lic) = &request.license {
        template_map.insert(
            serde_yaml::Value::String("license".to_string()),
            serde_yaml::Value::String(lic.clone()),
        );
    }

    // Add sources (already converted to YAML values)
    if !sources.is_empty() {
        template_map.insert(
            serde_yaml::Value::String("sources".to_string()),
            serde_yaml::Value::Sequence(sources),
        );
    }

    // Add queries (already converted to YAML values)
    if !queries.is_empty() {
        template_map.insert(
            serde_yaml::Value::String("queries".to_string()),
            serde_yaml::Value::Sequence(queries),
        );
    }

    // Add reactions (already converted to YAML values)
    if !reactions.is_empty() {
        template_map.insert(
            serde_yaml::Value::String("reactions".to_string()),
            serde_yaml::Value::Sequence(reactions),
        );
    }

    // Serialize to YAML string
    let yaml_content = match serde_yaml::to_string(&serde_yaml::Value::Mapping(template_map)) {
        Ok(content) => content,
        Err(e) => {
            return Json(ApiResponse::success(CreateSolutionTemplateResponse {
                success: false,
                template_id: None,
                error: Some(format!("Failed to serialize template: {}", e)),
            }));
        }
    };

    // Write to file
    let dir = solutions_dir.as_deref().unwrap_or(DEFAULT_SOLUTIONS_DIR);
    let dir_path = Path::new(dir);

    // Create directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(dir_path) {
        return Json(ApiResponse::success(CreateSolutionTemplateResponse {
            success: false,
            template_id: None,
            error: Some(format!("Failed to create solutions directory: {}", e)),
        }));
    }

    let file_path = dir_path.join(format!("{}.yaml", request.id));
    
    // Check if file already exists
    if file_path.exists() {
        return Json(ApiResponse::success(CreateSolutionTemplateResponse {
            success: false,
            template_id: None,
            error: Some(format!("Template '{}' already exists", request.id)),
        }));
    }

    if let Err(e) = std::fs::write(&file_path, yaml_content) {
        return Json(ApiResponse::success(CreateSolutionTemplateResponse {
            success: false,
            template_id: None,
            error: Some(format!("Failed to write template file: {}", e)),
        }));
    }

    log::info!(
        "Created solution template '{}' at '{}'",
        request.id,
        file_path.display()
    );

    Json(ApiResponse::success(CreateSolutionTemplateResponse {
        success: true,
        template_id: Some(request.id),
        error: None,
    }))
}

/// Deploy a solution template to an instance.
///
/// Two-phase deployment:
/// 1. Create all components with autoStart=false
/// 2. Start components that had autoStart=true (sources → queries → reactions)
///
/// If creation fails, rollback by deleting already-created components.
/// If start fails, components remain created but stopped.
pub async fn deploy_solution(
    registry: InstanceRegistry,
    persistence: Option<Arc<ConfigPersistence>>,
    solutions_dir: Option<String>,
    instance_id: &str,
    request: SolutionDeployRequest,
) -> Json<ApiResponse<SolutionDeployResponse>> {
    // Validate the request
    if let Err(e) = request.validate() {
        return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
            SolutionDeployError::validation(e),
        ])));
    }

    // Get the target instance
    let core = match registry.get(instance_id).await {
        Some(c) => c,
        None => {
            return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
                SolutionDeployError::validation(format!("Instance '{instance_id}' not found")),
            ])));
        }
    };

    // Load the template YAML
    let yaml_content = if let Some(template_id) = &request.template_id {
        let dir = solutions_dir.as_deref().unwrap_or(DEFAULT_SOLUTIONS_DIR);
        let path = Path::new(dir);
        let yaml_path = path.join(format!("{template_id}.yaml"));
        let yml_path = path.join(format!("{template_id}.yml"));

        if yaml_path.exists() {
            match std::fs::read_to_string(&yaml_path) {
                Ok(c) => c,
                Err(e) => {
                    return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
                        SolutionDeployError::validation(format!("Failed to read template: {e}")),
                    ])));
                }
            }
        } else if yml_path.exists() {
            match std::fs::read_to_string(&yml_path) {
                Ok(c) => c,
                Err(e) => {
                    return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
                        SolutionDeployError::validation(format!("Failed to read template: {e}")),
                    ])));
                }
            }
        } else {
            return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
                SolutionDeployError::validation(format!(
                    "Template '{template_id}' not found in solutions directory"
                )),
            ])));
        }
    } else if let Some(yaml) = &request.yaml {
        yaml.clone()
    } else {
        return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
            SolutionDeployError::validation("No template specified"),
        ])));
    };

    // Create a DtoMapper with the user's variable overrides
    let mapper = DtoMapper::with_overrides(request.variables.clone());

    // Resolve variables in the YAML content
    let resolved_yaml = resolve_yaml_variables(&yaml_content, &request.variables);

    // Parse the resolved YAML into the template structure
    let template: SolutionTemplateFile = match serde_yaml::from_str(&resolved_yaml) {
        Ok(t) => t,
        Err(e) => {
            return Json(ApiResponse::success(SolutionDeployResponse::failed(vec![
                SolutionDeployError::validation(format!("Failed to parse template: {e}")),
            ])));
        }
    };

    // ===== PHASE 1: VALIDATION =====
    // Parse and validate ALL component configs BEFORE creating anything.
    // Collect all validation errors so users can fix them all at once.

    let mut validation_errors: Vec<SolutionDeployError> = Vec::new();

    // Validated configs - these will be used in creation phase if validation passes
    let mut validated_sources: Vec<(SourceConfig, bool)> = Vec::new(); // (config, should_start)
    let mut validated_queries: Vec<(QueryConfigDto, bool)> = Vec::new(); // (dto, should_start)
    let mut validated_reactions: Vec<(ReactionConfig, bool)> = Vec::new(); // (config, should_start)

    // Validate sources
    for source_value in &template.sources {
        let source_id = source_value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let source_yaml = match serde_yaml::to_string(source_value) {
            Ok(y) => y,
            Err(e) => {
                validation_errors.push(SolutionDeployError::validation(format!(
                    "in source '{source_id}': Failed to serialize config: {e}"
                )));
                continue;
            }
        };

        let source_config: SourceConfig = match serde_yaml::from_str(&source_yaml) {
            Ok(c) => c,
            Err(e) => {
                let kind = source_value
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                validation_errors.push(SolutionDeployError::validation(format!(
                    "in source '{source_id}' (kind={kind}): {e}"
                )));
                continue;
            }
        };

        let should_start = source_config.auto_start();
        validated_sources.push((source_config, should_start));
    }

    // Validate queries
    for query_value in &template.queries {
        let query_id = query_value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let query_yaml = match serde_yaml::to_string(query_value) {
            Ok(y) => y,
            Err(e) => {
                validation_errors.push(SolutionDeployError::validation(format!(
                    "in query '{query_id}': Failed to serialize config: {e}"
                )));
                continue;
            }
        };

        let query_dto: QueryConfigDto = match serde_yaml::from_str(&query_yaml) {
            Ok(c) => c,
            Err(e) => {
                validation_errors.push(SolutionDeployError::validation(format!(
                    "in query '{query_id}': {e}"
                )));
                continue;
            }
        };

        // Also validate that it can be mapped to QueryConfig
        let query_mapper = QueryConfigMapper;
        if let Err(e) = mapper.map_with(&query_dto, &query_mapper) {
            validation_errors.push(SolutionDeployError::validation(format!(
                "in query '{query_id}': {e}"
            )));
            continue;
        }

        let should_start = query_dto.auto_start;
        validated_queries.push((query_dto, should_start));
    }

    // Validate reactions
    for reaction_value in &template.reactions {
        let reaction_id = reaction_value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let reaction_yaml = match serde_yaml::to_string(reaction_value) {
            Ok(y) => y,
            Err(e) => {
                validation_errors.push(SolutionDeployError::validation(format!(
                    "in reaction '{reaction_id}': Failed to serialize config: {e}"
                )));
                continue;
            }
        };

        let reaction_config: ReactionConfig = match serde_yaml::from_str(&reaction_yaml) {
            Ok(c) => c,
            Err(e) => {
                let kind = reaction_value
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                validation_errors.push(SolutionDeployError::validation(format!(
                    "in reaction '{reaction_id}' (kind={kind}): {e}"
                )));
                continue;
            }
        };

        let should_start = reaction_config.auto_start();
        validated_reactions.push((reaction_config, should_start));
    }

    // If there are any validation errors, return them ALL without creating anything
    if !validation_errors.is_empty() {
        return Json(ApiResponse::success(SolutionDeployResponse::failed(
            validation_errors,
        )));
    }

    // ===== PHASE 2: CREATION =====
    // All configs validated successfully. Now create components in order:
    // Sources first, then Queries, then Reactions.
    // All components created in stopped state.

    let mut created_sources: Vec<String> = Vec::new();
    let mut created_queries: Vec<String> = Vec::new();
    let mut created_reactions: Vec<String> = Vec::new();
    let mut creation_errors: Vec<SolutionDeployError> = Vec::new();

    // Track which components to start
    let mut sources_to_start: Vec<String> = Vec::new();
    let mut queries_to_start: Vec<String> = Vec::new();
    let mut reactions_to_start: Vec<String> = Vec::new();

    // Create sources (stopped)
    for (mut source_config, should_start) in validated_sources {
        let source_id = source_config.id().to_string();

        // Force autoStart to false for initial creation
        source_config.set_auto_start(false);

        let source = match create_source(source_config.clone()).await {
            Ok(s) => s,
            Err(e) => {
                creation_errors.push(SolutionDeployError::creation(
                    "source",
                    &source_id,
                    e.to_string(),
                ));
                // Rollback already-created sources
                rollback_sources(&core, &created_sources).await;
                return Json(ApiResponse::success(SolutionDeployResponse::failed(
                    creation_errors,
                )));
            }
        };

        if let Err(e) = core.add_source(source).await {
            creation_errors.push(SolutionDeployError::creation(
                "source",
                &source_id,
                e.to_string(),
            ));
            rollback_sources(&core, &created_sources).await;
            return Json(ApiResponse::success(SolutionDeployResponse::failed(
                creation_errors,
            )));
        }

        // Register for persistence
        if let Some(p) = &persistence {
            // Re-enable autoStart for persistence so config reflects intent
            source_config.set_auto_start(should_start);
            p.register_source(instance_id, source_config).await;
        }

        created_sources.push(source_id.clone());
        if should_start {
            sources_to_start.push(source_id);
        }
    }

    // Create queries (stopped)
    for (mut query_dto, should_start) in validated_queries {
        let query_id = query_dto.id.clone();

        // Force autoStart to false for initial creation
        query_dto.auto_start = false;

        // Convert to QueryConfig
        let query_mapper = QueryConfigMapper;
        let query_config = match mapper.map_with(&query_dto, &query_mapper) {
            Ok(c) => c,
            Err(e) => {
                // This shouldn't happen since we validated above, but handle it
                creation_errors.push(SolutionDeployError::creation(
                    "query",
                    &query_id,
                    e.to_string(),
                ));
                rollback_queries(&core, &created_queries).await;
                rollback_sources(&core, &created_sources).await;
                return Json(ApiResponse::success(SolutionDeployResponse::failed(
                    creation_errors,
                )));
            }
        };

        if let Err(e) = core.add_query(query_config).await {
            creation_errors.push(SolutionDeployError::creation(
                "query",
                &query_id,
                e.to_string(),
            ));
            rollback_queries(&core, &created_queries).await;
            rollback_sources(&core, &created_sources).await;
            return Json(ApiResponse::success(SolutionDeployResponse::failed(
                creation_errors,
            )));
        }

        // Register for persistence
        if let Some(p) = &persistence {
            // Re-enable autoStart for persistence
            query_dto.auto_start = should_start;
            p.register_query(instance_id, query_dto).await;
        }

        created_queries.push(query_id.clone());
        if should_start {
            queries_to_start.push(query_id);
        }
    }

    // Create reactions (stopped)
    for (mut reaction_config, should_start) in validated_reactions {
        let reaction_id = reaction_config.id().to_string();

        // Force autoStart to false for initial creation
        reaction_config.set_auto_start(false);

        let reaction = match create_reaction(reaction_config.clone()) {
            Ok(r) => r,
            Err(e) => {
                creation_errors.push(SolutionDeployError::creation(
                    "reaction",
                    &reaction_id,
                    e.to_string(),
                ));
                rollback_reactions(&core, &created_reactions).await;
                rollback_queries(&core, &created_queries).await;
                rollback_sources(&core, &created_sources).await;
                return Json(ApiResponse::success(SolutionDeployResponse::failed(
                    creation_errors,
                )));
            }
        };

        if let Err(e) = core.add_reaction(reaction).await {
            creation_errors.push(SolutionDeployError::creation(
                "reaction",
                &reaction_id,
                e.to_string(),
            ));
            rollback_reactions(&core, &created_reactions).await;
            rollback_queries(&core, &created_queries).await;
            rollback_sources(&core, &created_sources).await;
            return Json(ApiResponse::success(SolutionDeployResponse::failed(
                creation_errors,
            )));
        }

        // Register for persistence
        if let Some(p) = &persistence {
            // Re-enable autoStart for persistence
            reaction_config.set_auto_start(should_start);
            p.register_reaction(instance_id, reaction_config).await;
        }

        created_reactions.push(reaction_id.clone());
        if should_start {
            reactions_to_start.push(reaction_id);
        }
    }

    // ===== PHASE 3: START =====
    // All components created successfully. Now start those with autoStart=true.
    // Start order: sources → queries → reactions

    let mut components_started: Vec<String> = Vec::new();
    let mut start_errors: Vec<SolutionDeployError> = Vec::new();

    // Start sources
    for source_id in &sources_to_start {
        if let Err(e) = core.start_source(source_id).await {
            start_errors.push(SolutionDeployError::start(
                "source",
                source_id,
                e.to_string(),
            ));
        } else {
            components_started.push(format!("source:{source_id}"));
        }
    }

    // Start queries
    for query_id in &queries_to_start {
        if let Err(e) = core.start_query(query_id).await {
            start_errors.push(SolutionDeployError::start("query", query_id, e.to_string()));
        } else {
            components_started.push(format!("query:{query_id}"));
        }
    }

    // Start reactions
    for reaction_id in &reactions_to_start {
        if let Err(e) = core.start_reaction(reaction_id).await {
            start_errors.push(SolutionDeployError::start(
                "reaction",
                reaction_id,
                e.to_string(),
            ));
        } else {
            components_started.push(format!("reaction:{reaction_id}"));
        }
    }

    // Persist changes
    if let Some(p) = &persistence {
        if let Err(e) = p.save().await {
            log::warn!("Failed to persist config after solution deployment: {e}");
        }
    }

    // Return result
    if start_errors.is_empty() {
        Json(ApiResponse::success(SolutionDeployResponse::success(
            created_sources,
            created_queries,
            created_reactions,
            components_started,
        )))
    } else {
        // Partial success - all components created but some had start errors
        Json(ApiResponse::success(SolutionDeployResponse {
            success: true, // Creation succeeded, only start had issues
            sources_created: created_sources,
            queries_created: created_queries,
            reactions_created: created_reactions,
            components_started,
            errors: start_errors,
        }))
    }
}

/// Resolve variables in YAML content using the provided variable map.
fn resolve_yaml_variables(
    yaml: &str,
    variables: &std::collections::HashMap<String, String>,
) -> String {
    use regex::Regex;

    let re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}")
        .expect("Invalid regex pattern for variable resolution");

    re.replace_all(yaml, |caps: &regex::Captures| {
        let var_name = caps
            .get(1)
            .expect("Regex group 1 (variable name) must exist")
            .as_str();
        let default_value = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Check user-provided variables first, then env vars, then default
        if let Some(value) = variables.get(var_name) {
            value.clone()
        } else if let Ok(value) = std::env::var(var_name) {
            value
        } else {
            default_value.to_string()
        }
    })
    .to_string()
}

/// Rollback: delete created sources
async fn rollback_sources(core: &Arc<drasi_lib::DrasiLib>, sources: &[String]) {
    for source_id in sources {
        if let Err(e) = core.remove_source(source_id, false).await {
            log::warn!("Failed to rollback source '{source_id}': {e}");
        }
    }
}

/// Rollback: delete created queries
async fn rollback_queries(core: &Arc<drasi_lib::DrasiLib>, queries: &[String]) {
    for query_id in queries {
        if let Err(e) = core.remove_query(query_id).await {
            log::warn!("Failed to rollback query '{query_id}': {e}");
        }
    }
}

/// Rollback: delete created reactions
async fn rollback_reactions(core: &Arc<drasi_lib::DrasiLib>, reactions: &[String]) {
    for reaction_id in reactions {
        if let Err(e) = core.remove_reaction(reaction_id, false).await {
            log::warn!("Failed to rollback reaction '{reaction_id}': {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_template(dir: &Path, name: &str, content: &str) {
        let path = dir.join(format!("{name}.yaml"));
        std::fs::write(path, content).expect("Failed to write test template");
    }

    #[tokio::test]
    async fn test_list_solutions_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let Json(response) =
            list_solutions(Some(temp_dir.path().to_string_lossy().to_string())).await;
        assert!(response.success);
        assert!(response.data.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_solutions_non_existent_dir() {
        let Json(response) = list_solutions(Some("/non/existent/path".to_string())).await;
        assert!(response.success);
        assert!(response.data.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_solutions_with_templates() {
        let temp_dir = TempDir::new().unwrap();

        create_test_template(
            temp_dir.path(),
            "iot-monitor",
            r#"
name: IoT Temperature Monitor
description: Monitors sensors for high temps
version: "1.0.0"
author: Test Author
sources:
  - kind: mock
    id: sensor-source
queries:
  - id: high-temp
    query: "MATCH (s:Sensor) RETURN s"
reactions:
  - kind: log
    id: temp-logger
"#,
        );

        let Json(response) =
            list_solutions(Some(temp_dir.path().to_string_lossy().to_string())).await;
        assert!(response.success);

        let summaries = response.data.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "iot-monitor");
        assert_eq!(summaries[0].metadata.name, "IoT Temperature Monitor");
        assert_eq!(summaries[0].source_count, 1);
        assert_eq!(summaries[0].query_count, 1);
        assert_eq!(summaries[0].reaction_count, 1);
    }

    #[tokio::test]
    async fn test_get_solution_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let Json(response) = get_solution(
            Some(temp_dir.path().to_string_lossy().to_string()),
            "non-existent",
        )
        .await;
        assert!(!response.success);
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn test_get_solution_success() {
        let temp_dir = TempDir::new().unwrap();

        create_test_template(
            temp_dir.path(),
            "my-solution",
            r#"
name: My Solution
description: A test solution
version: "2.0.0"
defaultInstanceId: my-instance
sources:
  - kind: http
    id: http-source
    properties:
      host: "${HOST:-localhost}"
      port: "${PORT}"
queries:
  - id: my-query
    query: "MATCH (n) WHERE n.value > ${THRESHOLD:-100} RETURN n"
reactions: []
"#,
        );

        let Json(response) = get_solution(
            Some(temp_dir.path().to_string_lossy().to_string()),
            "my-solution",
        )
        .await;
        assert!(response.success);

        let detail = response.data.unwrap();
        assert_eq!(detail.id, "my-solution");
        assert_eq!(detail.metadata.name, "My Solution");
        assert_eq!(
            detail.metadata.default_instance_id,
            Some("my-instance".to_string())
        );
        assert_eq!(detail.source_ids, vec!["http-source"]);
        assert_eq!(detail.query_ids, vec!["my-query"]);
        assert!(detail.reaction_ids.is_empty());

        // Check extracted variables
        assert_eq!(detail.variables.len(), 3);

        let host_var = detail.variables.iter().find(|v| v.name == "HOST").unwrap();
        assert_eq!(host_var.default, Some("localhost".to_string()));
        assert!(!host_var.required);

        let port_var = detail.variables.iter().find(|v| v.name == "PORT").unwrap();
        assert!(port_var.default.is_none());
        assert!(port_var.required);

        let threshold_var = detail
            .variables
            .iter()
            .find(|v| v.name == "THRESHOLD")
            .unwrap();
        assert_eq!(threshold_var.default, Some("100".to_string()));
    }

    #[tokio::test]
    async fn test_get_solution_yml_extension() {
        let temp_dir = TempDir::new().unwrap();

        // Create with .yml extension
        let path = temp_dir.path().join("yml-solution.yml");
        std::fs::write(
            path,
            r#"
name: YML Solution
sources: []
queries: []
reactions: []
"#,
        )
        .unwrap();

        let Json(response) = get_solution(
            Some(temp_dir.path().to_string_lossy().to_string()),
            "yml-solution",
        )
        .await;
        assert!(response.success);
        assert_eq!(response.data.unwrap().metadata.name, "YML Solution");
    }

    #[tokio::test]
    async fn test_deploy_solution_validate_request() {
        // Test with neither template_id nor yaml
        let request = SolutionDeployRequest {
            template_id: None,
            yaml: None,
            variables: std::collections::HashMap::new(),
        };

        let Json(response) = deploy_solution(
            crate::instance_registry::InstanceRegistry::new(),
            None,
            None,
            "test-instance",
            request,
        )
        .await;

        // Should return a validation error in the response
        assert!(response.success);
        let deploy_response = response.data.unwrap();
        assert!(!deploy_response.success);
        assert_eq!(deploy_response.errors.len(), 1);
        assert_eq!(deploy_response.errors[0].phase, DeployPhase::Validation);
    }
}
