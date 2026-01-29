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

//! Platform source configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};

/// Local copy of platform source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformSourceConfigDto {
    pub redis_url: ConfigValue<String>,
    pub stream_key: ConfigValue<String>,
    #[serde(default = "default_consumer_group")]
    pub consumer_group: ConfigValue<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_name: Option<ConfigValue<String>>,
    #[serde(default = "default_batch_size")]
    pub batch_size: ConfigValue<usize>,
    #[serde(default = "default_block_ms")]
    pub block_ms: ConfigValue<u64>,
}

fn default_consumer_group() -> ConfigValue<String> {
    ConfigValue::Static("drasi-core".to_string())
}

fn default_batch_size() -> ConfigValue<usize> {
    ConfigValue::Static(100)
}

fn default_block_ms() -> ConfigValue<u64> {
    ConfigValue::Static(5000)
}
