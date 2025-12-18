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

//! HTTP source configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};

/// Local copy of HTTP source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpSourceConfigDto {
    pub host: ConfigValue<String>,
    pub port: ConfigValue<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<ConfigValue<String>>,
    #[serde(default = "default_http_timeout_ms")]
    pub timeout_ms: ConfigValue<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_max_batch_size: Option<ConfigValue<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_min_batch_size: Option<ConfigValue<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_max_wait_ms: Option<ConfigValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_min_wait_ms: Option<ConfigValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_window_secs: Option<ConfigValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_enabled: Option<ConfigValue<bool>>,
}

fn default_http_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(10000)
}
