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

//! gRPC source configuration DTOs.

use serde::{Deserialize, Serialize};
use crate::api::models::ConfigValue;

/// Local copy of gRPC source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrpcSourceConfigDto {
    #[serde(default = "default_grpc_host")]
    pub host: ConfigValue<String>,
    #[serde(default = "default_grpc_port")]
    pub port: ConfigValue<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<ConfigValue<String>>,
    #[serde(default = "default_grpc_timeout_ms")]
    pub timeout_ms: ConfigValue<u64>,
}

fn default_grpc_host() -> ConfigValue<String> {
    ConfigValue::Static("0.0.0.0".to_string())
}

fn default_grpc_port() -> ConfigValue<u16> {
    ConfigValue::Static(50051)
}

fn default_grpc_timeout_ms() -> ConfigValue<u64> {
    ConfigValue::Static(5000)
}
