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
use serde::{Deserialize, Serialize};

/// Query configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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
    pub sources: Vec<SourceSubscriptionConfigDto>,
    #[serde(default = "default_enable_bootstrap")]
    pub enable_bootstrap: bool,
    #[serde(default = "default_bootstrap_buffer_size")]
    pub bootstrap_buffer_size: usize,
}

/// Source subscription configuration DTO with camelCase serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceSubscriptionConfigDto {
    pub source_id: ConfigValue<String>,
    #[serde(default)]
    pub pipeline: Vec<String>,
}

fn default_auto_start() -> bool {
    false
}

fn default_query_language() -> ConfigValue<String> {
    ConfigValue::Static("Cypher".to_string())
}

fn default_enable_bootstrap() -> bool {
    true
}

fn default_bootstrap_buffer_size() -> usize {
    10000
}
