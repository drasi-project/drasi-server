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

//! Platform reaction configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};

/// Local copy of platform reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformReactionConfigDto {
    pub redis_url: ConfigValue<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubsub_name: Option<ConfigValue<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_name: Option<ConfigValue<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stream_length: Option<ConfigValue<usize>>,
    #[serde(default)]
    pub emit_control_events: ConfigValue<bool>,
    #[serde(default)]
    pub batch_enabled: ConfigValue<bool>,
    #[serde(default = "default_batch_size")]
    pub batch_max_size: ConfigValue<usize>,
    #[serde(default = "default_batch_wait_ms")]
    pub batch_max_wait_ms: ConfigValue<u64>,
}

fn default_batch_size() -> ConfigValue<usize> {
    ConfigValue::Static(100)
}

fn default_batch_wait_ms() -> ConfigValue<u64> {
    ConfigValue::Static(100)
}
