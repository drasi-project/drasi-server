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

/// Local copy of SSE reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SseReactionConfigDto {
    #[serde(default = "default_sse_host")]
    pub host: ConfigValue<String>,
    #[serde(default = "default_sse_port")]
    pub port: ConfigValue<u16>,
    #[serde(default = "default_sse_path")]
    pub sse_path: ConfigValue<String>,
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: ConfigValue<u64>,
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
