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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Local copy of HTTP reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpReactionConfigDto {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default = "default_reaction_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub routes: HashMap<String, QueryConfigDto>,
}

fn default_base_url() -> String {
    "http://localhost".to_string()
}

fn default_reaction_timeout_ms() -> u64 {
    5000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryConfigDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<CallSpecDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<CallSpecDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<CallSpecDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallSpecDto {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Local copy of HTTP adaptive reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpAdaptiveReactionConfigDto {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default = "default_reaction_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub routes: HashMap<String, QueryConfigDto>,
    #[serde(flatten)]
    pub adaptive: AdaptiveBatchConfigDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdaptiveBatchConfigDto {
    #[serde(default = "default_adaptive_min_batch_size")]
    pub adaptive_min_batch_size: usize,
    #[serde(default = "default_adaptive_max_batch_size")]
    pub adaptive_max_batch_size: usize,
    #[serde(default = "default_adaptive_window_size")]
    pub adaptive_window_size: usize,
    #[serde(default = "default_adaptive_batch_timeout_ms")]
    pub adaptive_batch_timeout_ms: u64,
}

fn default_adaptive_window_size() -> usize {
    100
}

fn default_adaptive_batch_timeout_ms() -> u64 {
    1000
}

fn default_adaptive_max_batch_size() -> usize {
    1000
}

fn default_adaptive_min_batch_size() -> usize {
    1
}
