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

use std::sync::Arc;
use std::{collections::btree_map::Entry, collections::BTreeMap};

use chrono::Utc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars,
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::mappings::{ConfigMapper, DtoMapper, QueryConfigMapper};
use crate::api::models::{QueryConfigDto, SourceSubscriptionConfigDto};
use crate::api::shared::handlers::persist_after_operation;
use crate::config::{ReactionConfig, SourceConfig};
use crate::factories::{create_reaction, create_source};
use crate::instance_registry::InstanceRegistry;
use crate::persistence::ConfigPersistence;
use crate::plugin_registry::PluginRegistry;
use drasi_lib::config::QueryLanguage;
use drasi_lib::queries::LabelExtractor;
use drasi_lib::DrasiLib;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct InstanceRequest {
    instance_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ComponentRequest {
    instance_id: Option<String>,
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CreateConfigRequest {
    instance_id: Option<String>,
    config: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CreateQueryRequest {
    instance_id: Option<String>,
    id: String,
    query: String,
    query_language: Option<String>,
    sources: Option<Vec<String>>,
    auto_start: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ValidateQueryRequest {
    instance_id: Option<String>,
    id: String,
    query: String,
    query_language: Option<String>,
}

#[derive(Clone)]
pub struct DrasiMcpServer {
    registry: InstanceRegistry,
    read_only: Arc<bool>,
    config_persistence: Option<Arc<ConfigPersistence>>,
    plugin_registry: Arc<PluginRegistry>,
    tool_router: ToolRouter<Self>,
}

impl DrasiMcpServer {
    pub fn new(
        registry: InstanceRegistry,
        read_only: Arc<bool>,
        config_persistence: Option<Arc<ConfigPersistence>>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> Self {
        Self {
            registry,
            read_only,
            config_persistence,
            plugin_registry,
            tool_router: Self::tool_router(),
        }
    }

    async fn resolve_instance(
        &self,
        instance_id: Option<String>,
    ) -> Result<(String, Arc<DrasiLib>), String> {
        match instance_id {
            Some(instance_id) => self
                .registry
                .get(&instance_id)
                .await
                .map(|core| (instance_id.clone(), core))
                .ok_or_else(|| format!("Instance '{instance_id}' not found")),
            None => self
                .registry
                .get_default()
                .await
                .ok_or_else(|| "No instances configured".to_string()),
        }
    }

    fn success<T: Serialize>(&self, data: T) -> String {
        to_pretty_json(json!({ "ok": true, "data": data }))
    }

    fn error(&self, message: impl Into<String>) -> String {
        to_pretty_json(json!({ "ok": false, "error": message.into() }))
    }

    async fn build_schema_snapshot(&self, core: &Arc<DrasiLib>) -> Result<Value, String> {
        let source_ids = core
            .list_sources()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();

        let query_ids = core
            .list_queries()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();

        let mut nodes: BTreeMap<String, Value> = BTreeMap::new();
        let mut relations: BTreeMap<String, Value> = BTreeMap::new();

        for query_id in query_ids {
            let config = match core.get_query_config(&query_id).await {
                Ok(config) => config,
                Err(_) => continue,
            };

            let labels = match LabelExtractor::extract_labels(&config.query, &config.query_language)
            {
                Ok(labels) => labels,
                Err(_) => continue,
            };

            for label in labels.node_labels {
                match nodes.entry(label) {
                    Entry::Occupied(mut existing) => {
                        if let Some(queried_by) = existing
                            .get_mut()
                            .get_mut("queriedBy")
                            .and_then(Value::as_array_mut)
                        {
                            queried_by.push(Value::String(query_id.clone()));
                        }
                    }
                    Entry::Vacant(vacant) => {
                        vacant.insert(json!({
                            "sources": [],
                            "queriedBy": [query_id.clone()],
                            "properties": []
                        }));
                    }
                }
            }

            for label in labels.relation_labels {
                match relations.entry(label) {
                    Entry::Occupied(mut existing) => {
                        if let Some(queried_by) = existing
                            .get_mut()
                            .get_mut("queriedBy")
                            .and_then(Value::as_array_mut)
                        {
                            queried_by.push(Value::String(query_id.clone()));
                        }
                    }
                    Entry::Vacant(vacant) => {
                        vacant.insert(json!({
                            "sources": [],
                            "queriedBy": [query_id.clone()],
                            "from": Value::Null,
                            "to": Value::Null,
                            "properties": []
                        }));
                    }
                }
            }
        }

        Ok(json!({
            "nodes": nodes,
            "relations": relations,
            "sourcesWithoutSchema": source_ids,
            "note": "This schema view is derived from the currently configured queries and source inventory. When the local ../drasi-core schema-inspection patches are enabled via .cargo/config.toml, richer per-source schema data can be surfaced here."
        }))
    }
}

#[tool_router]
impl DrasiMcpServer {
    #[tool(description = "Get Drasi server health status")]
    async fn get_health(&self) -> String {
        self.success(json!({
            "status": "ok",
            "timestamp": Utc::now(),
        }))
    }

    #[tool(description = "List all configured Drasi instances")]
    async fn list_instances(&self) -> String {
        let instances = self.registry.list().await;
        let mut data = Vec::with_capacity(instances.len());

        for (id, core) in instances {
            let source_count = core.list_sources().await.map(|v| v.len()).unwrap_or(0);
            let query_count = core.list_queries().await.map(|v| v.len()).unwrap_or(0);
            let reaction_count = core.list_reactions().await.map(|v| v.len()).unwrap_or(0);

            data.push(json!({
                "id": id,
                "sourceCount": source_count,
                "queryCount": query_count,
                "reactionCount": reaction_count,
            }));
        }

        self.success(data)
    }

    #[tool(description = "List all configured data sources")]
    async fn list_sources(&self, Parameters(request): Parameters<InstanceRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.list_sources().await {
                Ok(items) => self.success(
                    items
                        .into_iter()
                        .map(|(id, status)| json!({ "id": id, "status": format!("{status:?}") }))
                        .collect::<Vec<_>>(),
                ),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Get a single source by ID")]
    async fn get_source(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.get_source_info(&request.id).await {
                Ok(info) => self.success(info),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Create a source from a JSON config object")]
    async fn create_source(&self, Parameters(request): Parameters<CreateConfigRequest>) -> String {
        if *self.read_only {
            return self.error("Server is in read-only mode. Cannot create sources.");
        }

        let (instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        let source_config: SourceConfig = match serde_json::from_value(request.config) {
            Ok(config) => config,
            Err(e) => return self.error(format!("Invalid source configuration: {e}")),
        };
        let source_id = source_config.id().to_string();

        let source = match create_source(self.plugin_registry.as_ref(), source_config.clone()).await
        {
            Ok(source) => source,
            Err(e) => return self.error(format!("Failed to create source: {e}")),
        };

        match core.add_source(source).await {
            Ok(_) => {
                if let Some(persistence) = &self.config_persistence {
                    persistence
                        .register_source(&instance_id, source_config)
                        .await;
                }
                persist_after_operation(&self.config_persistence, "creating source").await;
                self.success(
                    json!({ "message": format!("Source '{source_id}' created successfully") }),
                )
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    #[tool(description = "Delete a source")]
    async fn delete_source(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        if *self.read_only {
            return self.error("Server is in read-only mode. Cannot delete sources.");
        }

        let (instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        match core.remove_source(&request.id, true).await {
            Ok(_) => {
                if let Some(persistence) = &self.config_persistence {
                    persistence
                        .unregister_source(&instance_id, &request.id)
                        .await;
                }
                persist_after_operation(&self.config_persistence, "deleting source").await;
                self.success(json!({
                    "message": format!("Source '{}' deleted successfully", request.id)
                }))
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    #[tool(description = "Start a stopped source")]
    async fn start_source(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.start_source(&request.id).await {
                Ok(_) => self.success(json!({
                    "message": format!("Source '{}' started successfully", request.id)
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Stop a running source")]
    async fn stop_source(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.stop_source(&request.id).await {
                Ok(_) => self.success(json!({
                    "message": format!("Source '{}' stopped successfully", request.id)
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "List all continuous queries")]
    async fn list_queries(&self, Parameters(request): Parameters<InstanceRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.list_queries().await {
                Ok(items) => self.success(
                    items
                        .into_iter()
                        .map(|(id, status)| json!({ "id": id, "status": format!("{status:?}") }))
                        .collect::<Vec<_>>(),
                ),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Get a single query by ID")]
    async fn get_query(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => {
                match (
                    core.get_query_info(&request.id).await,
                    core.get_query_config(&request.id).await,
                ) {
                    (Ok(info), Ok(config)) => {
                        self.success(json!({ "runtime": info, "config": config }))
                    }
                    (Err(e), _) | (_, Err(e)) => self.error(e.to_string()),
                }
            }
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Create a continuous query")]
    async fn create_query(&self, Parameters(request): Parameters<CreateQueryRequest>) -> String {
        if *self.read_only {
            return self.error("Server is in read-only mode. Cannot create queries.");
        }

        let (instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        let dto = QueryConfigDto {
            id: request.id.clone(),
            auto_start: request.auto_start.unwrap_or(true),
            query: request.query,
            query_language: parse_query_language(request.query_language),
            middleware: Vec::new(),
            sources: request
                .sources
                .unwrap_or_default()
                .into_iter()
                .map(|source_id| SourceSubscriptionConfigDto {
                    source_id,
                    nodes: Vec::new(),
                    relations: Vec::new(),
                    pipeline: Vec::new(),
                })
                .collect(),
            enable_bootstrap: true,
            bootstrap_buffer_size: 10_000,
            joins: None,
            priority_queue_capacity: None,
            dispatch_buffer_capacity: None,
            dispatch_mode: None,
            storage_backend: None,
        };

        let mapper = DtoMapper::new();
        let config = match QueryConfigMapper.map(&dto, &mapper) {
            Ok(config) => config,
            Err(e) => return self.error(format!("Invalid query configuration: {e}")),
        };

        match core.add_query(config).await {
            Ok(_) => {
                if let Some(persistence) = &self.config_persistence {
                    persistence.register_query(&instance_id, dto).await;
                }
                persist_after_operation(&self.config_persistence, "creating query").await;
                self.success(json!({
                    "message": format!("Query '{}' created successfully", request.id)
                }))
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    #[tool(description = "Delete a query")]
    async fn delete_query(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        if *self.read_only {
            return self.error("Server is in read-only mode. Cannot delete queries.");
        }

        let (instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        match core.remove_query(&request.id).await {
            Ok(_) => {
                if let Some(persistence) = &self.config_persistence {
                    persistence
                        .unregister_query(&instance_id, &request.id)
                        .await;
                }
                persist_after_operation(&self.config_persistence, "deleting query").await;
                self.success(json!({
                    "message": format!("Query '{}' deleted successfully", request.id)
                }))
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    #[tool(description = "Start a stopped query")]
    async fn start_query(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.start_query(&request.id).await {
                Ok(_) => self.success(json!({
                    "message": format!("Query '{}' started successfully", request.id)
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Stop a running query")]
    async fn stop_query(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.stop_query(&request.id).await {
                Ok(_) => self.success(json!({
                    "message": format!("Query '{}' stopped successfully", request.id)
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Get the current result snapshot for a continuous query")]
    async fn get_query_results(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.get_query_results(&request.id).await {
                Ok(results) => self.success(results),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "List all configured reactions")]
    async fn list_reactions(&self, Parameters(request): Parameters<InstanceRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.list_reactions().await {
                Ok(items) => self.success(
                    items
                        .into_iter()
                        .map(|(id, status)| json!({ "id": id, "status": format!("{status:?}") }))
                        .collect::<Vec<_>>(),
                ),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Get a single reaction by ID")]
    async fn get_reaction(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.get_reaction_info(&request.id).await {
                Ok(info) => self.success(info),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Create a reaction from a JSON config object")]
    async fn create_reaction(
        &self,
        Parameters(request): Parameters<CreateConfigRequest>,
    ) -> String {
        if *self.read_only {
            return self.error("Server is in read-only mode. Cannot create reactions.");
        }

        let (instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        let reaction_config: ReactionConfig = match serde_json::from_value(request.config) {
            Ok(config) => config,
            Err(e) => return self.error(format!("Invalid reaction configuration: {e}")),
        };
        let reaction_id = reaction_config.id().to_string();

        let reaction =
            match create_reaction(self.plugin_registry.as_ref(), reaction_config.clone()).await {
                Ok(reaction) => reaction,
                Err(e) => return self.error(format!("Failed to create reaction: {e}")),
            };

        match core.add_reaction(reaction).await {
            Ok(_) => {
                if let Some(persistence) = &self.config_persistence {
                    persistence
                        .register_reaction(&instance_id, reaction_config)
                        .await;
                }
                persist_after_operation(&self.config_persistence, "creating reaction").await;
                self.success(
                    json!({ "message": format!("Reaction '{reaction_id}' created successfully") }),
                )
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    #[tool(description = "Delete a reaction")]
    async fn delete_reaction(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        if *self.read_only {
            return self.error("Server is in read-only mode. Cannot delete reactions.");
        }

        let (instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        match core.remove_reaction(&request.id, true).await {
            Ok(_) => {
                if let Some(persistence) = &self.config_persistence {
                    persistence
                        .unregister_reaction(&instance_id, &request.id)
                        .await;
                }
                persist_after_operation(&self.config_persistence, "deleting reaction").await;
                self.success(json!({
                    "message": format!("Reaction '{}' deleted successfully", request.id)
                }))
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    #[tool(description = "Start a stopped reaction")]
    async fn start_reaction(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.start_reaction(&request.id).await {
                Ok(_) => self.success(json!({
                    "message": format!("Reaction '{}' started successfully", request.id)
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Stop a running reaction")]
    async fn stop_reaction(&self, Parameters(request): Parameters<ComponentRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match core.stop_reaction(&request.id).await {
                Ok(_) => self.success(json!({
                    "message": format!("Reaction '{}' stopped successfully", request.id)
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Get the merged graph schema for an instance")]
    async fn get_schema(&self, Parameters(request): Parameters<InstanceRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match self.build_schema_snapshot(&core).await {
                Ok(schema) => self.success(schema),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Get schema, query guidance, and example patterns in one call")]
    async fn get_query_context(&self, Parameters(request): Parameters<InstanceRequest>) -> String {
        match self.resolve_instance(request.instance_id).await {
            Ok((_instance_id, core)) => match self.build_schema_snapshot(&core).await {
                Ok(schema) => self.success(json!({
                    "schema": schema,
                    "reference": query_language_reference(),
                    "examples": query_examples(),
                })),
                Err(e) => self.error(e.to_string()),
            },
            Err(e) => self.error(e),
        }
    }

    #[tool(description = "Validate a Cypher or GQL query against the known graph schema")]
    async fn validate_query(
        &self,
        Parameters(request): Parameters<ValidateQueryRequest>,
    ) -> String {
        let (_instance_id, core) = match self.resolve_instance(request.instance_id).await {
            Ok(result) => result,
            Err(e) => return self.error(e),
        };

        let query_language = parse_query_language(request.query_language);
        let labels = match LabelExtractor::extract_labels(&request.query, &query_language) {
            Ok(labels) => labels,
            Err(e) => {
                return self.error(format!("Query '{}' failed validation: {e}", request.id));
            }
        };

        let schema = match self.build_schema_snapshot(&core).await {
            Ok(schema) => schema,
            Err(e) => return self.error(e.to_string()),
        };

        let known_nodes = schema
            .get("nodes")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let known_relations = schema
            .get("relations")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let sources_without_schema = schema
            .get("sourcesWithoutSchema")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let unknown_nodes = labels
            .node_labels
            .iter()
            .filter(|label| !known_nodes.contains_key(*label))
            .cloned()
            .collect::<Vec<_>>();
        let unknown_relations = labels
            .relation_labels
            .iter()
            .filter(|label| !known_relations.contains_key(*label))
            .cloned()
            .collect::<Vec<_>>();

        let warning = if !sources_without_schema.is_empty()
            && (!unknown_nodes.is_empty() || !unknown_relations.is_empty())
        {
            Some(format!(
                "Some sources do not expose schema information yet: {}. Unknown labels may still be valid.",
                sources_without_schema
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        } else {
            None
        };

        self.success(json!({
            "id": request.id,
            "valid": unknown_nodes.is_empty() && unknown_relations.is_empty(),
            "queryLanguage": format!("{query_language:?}"),
            "nodeLabels": labels.node_labels,
            "relationLabels": labels.relation_labels,
            "unknownNodeLabels": unknown_nodes,
            "unknownRelationLabels": unknown_relations,
            "warning": warning,
            "note": "Validation uses labels known from the active query/source inventory. Unknown labels may still be valid if a source has not been introspected yet.",
        }))
    }
}

#[tool_handler]
impl ServerHandler for DrasiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Drasi is a real-time change detection engine. Use the schema and query-context tools before creating continuous queries.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn parse_query_language(query_language: Option<String>) -> QueryLanguage {
    match query_language
        .unwrap_or_else(|| "cypher".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "gql" => QueryLanguage::GQL,
        _ => QueryLanguage::Cypher,
    }
}

fn query_language_reference() -> Value {
    json!({
        "instructions": "Drasi continuously evaluates graph queries over changing data and alerts when result sets change.",
        "drasiFunctions": [
            {
                "name": "drasi.changeDateTime(node)",
                "description": "Returns the timestamp when a node was last changed."
            },
            {
                "name": "drasi.trueLater(condition, timestamp)",
                "description": "Becomes true when the wall clock reaches the timestamp and the condition still holds."
            },
            {
                "name": "drasi.trueFor(condition, duration)",
                "description": "Becomes true when the condition has been continuously true for the given duration."
            },
            {
                "name": "datetime.realtime()",
                "description": "Returns the current wall-clock time."
            }
        ],
        "supportedClauses": ["MATCH", "WHERE", "RETURN", "WITH", "ORDER BY", "LIMIT", "UNWIND"],
        "aggregations": ["count()", "sum()", "avg()", "min()", "max()"]
    })
}

fn query_examples() -> Value {
    json!([
        {
            "name": "threshold",
            "query": "MATCH (s:Sensor) WHERE s.temperature > 80 RETURN s"
        },
        {
            "name": "aggregation",
            "query": "MATCH (o:Order) RETURN count(o) AS Total, sum(o.amount) AS Revenue"
        },
        {
            "name": "join",
            "query": "MATCH (c:Customer), (o:Order) WHERE c.id = o.customer_id RETURN c.name, o.total"
        },
        {
            "name": "absence-detection",
            "query": "MATCH (s:Service) WITH s, drasi.changeDateTime(s) AS lastSeen WHERE drasi.trueLater(lastSeen + duration({minutes: 5})) RETURN s.id, lastSeen"
        }
    ])
}

fn to_pretty_json(value: Value) -> String {
    serde_json::to_string_pretty(&value).unwrap_or_else(|e| {
        format!(
            "{{\"ok\":false,\"error\":\"failed to serialize MCP response: {}\"}}",
            e
        )
    })
}
