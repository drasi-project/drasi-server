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

//! HTTP reaction configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Local copy of HTTP reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = HttpReactionConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpReactionConfigDto {
    #[serde(default = "default_base_url")]
    pub base_url: ConfigValue<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<ConfigValue<String>>,
    #[serde(default = "default_reaction_timeout_ms")]
    pub timeout_ms: ConfigValue<u64>,
    #[serde(default)]
    #[schema(value_type = HashMap<String, HttpQueryConfig>)]
    pub routes: HashMap<String, HttpQueryConfigDto>,
}

fn default_base_url() -> ConfigValue<String> {
    ConfigValue::Static("http://localhost".to_string())
}

fn default_reaction_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(5000)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = HttpQueryConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpQueryConfigDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<CallSpec>)]
    pub added: Option<CallSpecDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<CallSpec>)]
    pub updated: Option<CallSpecDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<CallSpec>)]
    pub deleted: Option<CallSpecDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = CallSpec)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallSpecDto {
    pub url: ConfigValue<String>,
    pub method: ConfigValue<String>,
    #[serde(default)]
    pub body: ConfigValue<String>,
    #[serde(default)]
    pub headers: HashMap<String, ConfigValue<String>>,
}

/// Local copy of HTTP adaptive reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[schema(as = HttpAdaptiveReactionConfig)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpAdaptiveReactionConfigDto {
    #[serde(default = "default_base_url")]
    pub base_url: ConfigValue<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<ConfigValue<String>>,
    #[serde(default = "default_reaction_timeout_ms")]
    pub timeout_ms: ConfigValue<u64>,
    #[serde(default)]
    #[schema(value_type = HashMap<String, HttpQueryConfig>)]
    pub routes: HashMap<String, HttpQueryConfigDto>,
    #[serde(default = "default_adaptive_min_batch_size")]
    pub adaptive_min_batch_size: ConfigValue<usize>,
    #[serde(default = "default_adaptive_max_batch_size")]
    pub adaptive_max_batch_size: ConfigValue<usize>,
    #[serde(default = "default_adaptive_window_size")]
    pub adaptive_window_size: ConfigValue<usize>,
    #[serde(default = "default_adaptive_batch_timeout_ms")]
    pub adaptive_batch_timeout_ms: ConfigValue<u64>,
}

// Public default functions for adaptive batching (shared with grpc_reaction)
pub fn default_adaptive_window_size() -> ConfigValue<usize> {
    ConfigValue::Static(100)
}

pub fn default_adaptive_batch_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(1000)
}

pub fn default_adaptive_max_batch_size() -> ConfigValue<usize> {
    ConfigValue::Static(1000)
}

pub fn default_adaptive_min_batch_size() -> ConfigValue<usize> {
    ConfigValue::Static(1)
}
