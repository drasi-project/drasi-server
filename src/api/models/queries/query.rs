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

use crate::api::models::ConfigValue;
use drasi_lib::QueryConfig;
use serde::{Deserialize, Serialize};

/// Query configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = QueryConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryConfigDto {
    pub id: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
    pub query: ConfigValue<String>,
    #[serde(default = "default_query_language")]
    pub query_language: ConfigValue<String>,
    #[serde(default)]
    pub middleware: Vec<String>,
    #[serde(default)]
    #[schema(value_type = Vec<SourceSubscriptionConfig>)]
    pub sources: Vec<SourceSubscriptionConfigDto>,
    #[serde(default = "default_enable_bootstrap")]
    pub enable_bootstrap: bool,
    #[serde(default = "default_bootstrap_buffer_size")]
    pub bootstrap_buffer_size: usize,
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
}

/// Source subscription configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = SourceSubscriptionConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSubscriptionConfigDto {
    pub source_id: ConfigValue<String>,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub relations: Vec<String>,
    #[serde(default)]
    pub pipeline: Vec<String>,
}

fn default_auto_start() -> bool {
    false
}

fn default_query_language() -> ConfigValue<String> {
    ConfigValue::Static("GQL".to_string())
}

fn default_enable_bootstrap() -> bool {
    true
}

fn default_bootstrap_buffer_size() -> usize {
    10000
}

impl From<QueryConfig> for QueryConfigDto {
    fn from(config: QueryConfig) -> Self {
        Self {
            id: config.id,
            auto_start: config.auto_start,
            query: ConfigValue::Static(config.query),
            query_language: ConfigValue::Static(format!("{:?}", config.query_language)),
            middleware: config
                .middleware
                .into_iter()
                .map(|m| m.name.to_string())
                .collect(),
            sources: config
                .sources
                .into_iter()
                .map(|s| SourceSubscriptionConfigDto {
                    source_id: ConfigValue::Static(s.source_id),
                    nodes: s.nodes,
                    relations: s.relations,
                    pipeline: s.pipeline,
                })
                .collect(),
            enable_bootstrap: config.enable_bootstrap,
            bootstrap_buffer_size: config.bootstrap_buffer_size,
            joins: config
                .joins
                .map(|j| serde_json::to_value(j).expect("joins serialization")),
            priority_queue_capacity: config.priority_queue_capacity,
            dispatch_buffer_capacity: config.dispatch_buffer_capacity,
            dispatch_mode: config.dispatch_mode.map(|d| format!("{d:?}")),
            storage_backend: config
                .storage_backend
                .map(|s| serde_json::to_value(s).expect("storage_backend serialization")),
        }
    }
}
