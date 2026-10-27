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

//! Query configuration DTOs with camelCase serialization.

use drasi_lib::config::{QueryInputConfig, QueryLanguage};
use drasi_lib::QueryConfig;
use serde::{Deserialize, Serialize};
use serde_json::Map;

/// Query configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = QueryConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryConfigDto {
    pub id: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
    pub query: String,
    #[serde(default = "default_query_language")]
    pub query_language: QueryLanguage,
    #[serde(default)]
    #[schema(value_type = Vec<SourceMiddlewareConfig>)]
    pub middleware: Vec<SourceMiddlewareConfigDto>,
    #[serde(default)]
    #[schema(value_type = Vec<QueryInputConfig>)]
    pub inputs: Vec<QueryInputConfigDto>,
    #[serde(default, skip_serializing_if = "QueryResultConfigDto::is_default")]
    pub result: QueryResultConfigDto,
    #[serde(default = "default_enable_bootstrap")]
    pub enable_bootstrap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joins: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_queue_capacity: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatch_buffer_capacity: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatch_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_backend: Option<serde_json::Value>,
    #[serde(default = "default_outbox_capacity")]
    pub outbox_capacity: usize,
    #[serde(default = "default_bootstrap_timeout_secs")]
    pub bootstrap_timeout_secs: u64,
}

/// Query input binding: a bare upstream component name, or an object that can also
/// select the component's output port, label interests and middleware pipeline.
///
/// Both forms deserialize through the drasi-lib configuration type, so unknown
/// fields are rejected exactly once and consistently.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = QueryInputConfig)]
#[serde(untagged, from = "QueryInputConfig")]
pub enum QueryInputConfigDto {
    /// Consume the named component's primary output.
    Component(String),
    Expanded(QueryInputObjectDto),
}

/// Expanded query input binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = QueryInputObject)]
#[serde(deny_unknown_fields)]
pub struct QueryInputObjectDto {
    /// Upstream component ID (a source, query or transformer).
    pub component: String,
    /// Output port on the component. Omit to use its primary (or only) output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pipeline: Vec<String>,
}

impl From<QueryInputConfig> for QueryInputConfigDto {
    fn from(input: QueryInputConfig) -> Self {
        if input.port.is_none()
            && input.nodes.is_empty()
            && input.relations.is_empty()
            && input.pipeline.is_empty()
        {
            return Self::Component(input.component);
        }
        Self::Expanded(QueryInputObjectDto {
            component: input.component,
            port: input.port,
            nodes: input.nodes,
            relations: input.relations,
            pipeline: input.pipeline,
        })
    }
}

impl From<&QueryInputConfigDto> for QueryInputConfig {
    fn from(dto: &QueryInputConfigDto) -> Self {
        match dto {
            QueryInputConfigDto::Component(component) => QueryInputConfig::new(component.clone()),
            QueryInputConfigDto::Expanded(object) => QueryInputConfig {
                component: object.component.clone(),
                port: object.port.clone(),
                nodes: object.nodes.clone(),
                relations: object.relations.clone(),
                pipeline: object.pipeline.clone(),
            },
        }
    }
}

/// Result publication options for a query. The label names published result nodes;
/// the output schema is graph-change regardless.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = QueryResultConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryResultConfigDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl QueryResultConfigDto {
    pub fn is_default(&self) -> bool {
        self.label.is_none()
    }
}

/// Source middleware configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = SourceMiddlewareConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMiddlewareConfigDto {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub config: Map<String, serde_json::Value>,
}

fn default_auto_start() -> bool {
    false
}

fn default_query_language() -> QueryLanguage {
    QueryLanguage::GQL
}

fn default_enable_bootstrap() -> bool {
    true
}

fn default_outbox_capacity() -> usize {
    1000
}

fn default_bootstrap_timeout_secs() -> u64 {
    300
}

impl TryFrom<QueryConfig> for QueryConfigDto {
    type Error = serde_json::Error;

    fn try_from(config: QueryConfig) -> Result<Self, Self::Error> {
        let joins = config.joins.map(serde_json::to_value).transpose()?;
        let storage_backend = config
            .storage_backend
            .map(serde_json::to_value)
            .transpose()?;

        Ok(Self {
            id: config.id,
            auto_start: config.auto_start,
            query: config.query,
            query_language: config.query_language,
            middleware: config
                .middleware
                .into_iter()
                .map(|m| SourceMiddlewareConfigDto {
                    kind: m.kind.to_string(),
                    name: m.name.to_string(),
                    config: m.config,
                })
                .collect(),
            inputs: config.inputs.into_iter().map(Into::into).collect(),
            result: QueryResultConfigDto {
                label: config.result.label,
            },
            enable_bootstrap: config.enable_bootstrap,
            joins,
            priority_queue_capacity: config.priority_queue_capacity,
            dispatch_buffer_capacity: config.dispatch_buffer_capacity,
            dispatch_mode: config.dispatch_mode.map(|d| format!("{d:?}")),
            storage_backend,
            outbox_capacity: config.outbox_capacity,
            bootstrap_timeout_secs: config.bootstrap_timeout_secs,
        })
    }
}
