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

//! SSE reaction configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Template specification for SSE output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SseTemplateSpecDto {
    /// Optional custom path for this template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Event data template as a Handlebars template
    #[serde(default)]
    pub template: String,
}

/// Configuration for query-specific SSE output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SseQueryConfigDto {
    /// Template for ADD operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<SseTemplateSpecDto>,
    /// Template for UPDATE operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<SseTemplateSpecDto>,
    /// Template for DELETE operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<SseTemplateSpecDto>,
}

/// Local copy of SSE reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SseReactionConfigDto {
    #[serde(default = "default_sse_host")]
    pub host: ConfigValue<String>,
    #[serde(default = "default_sse_port")]
    pub port: ConfigValue<u16>,
    #[serde(default = "default_sse_path")]
    pub sse_path: ConfigValue<String>,
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: ConfigValue<u64>,
    /// Query-specific template configurations
    #[serde(default)]
    pub routes: HashMap<String, SseQueryConfigDto>,
    /// Default template configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_template: Option<SseQueryConfigDto>,
}

fn default_sse_host() -> ConfigValue<String> {
    ConfigValue::Static("0.0.0.0".to_string())
}

fn default_sse_port() -> ConfigValue<u16> {
    ConfigValue::Static(8080)
}

fn default_sse_path() -> ConfigValue<String> {
    ConfigValue::Static("/events".to_string())
}

fn default_heartbeat_interval_ms() -> ConfigValue<u64> {
    ConfigValue::Static(30000)
}
