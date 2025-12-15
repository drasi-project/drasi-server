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

//! gRPC reaction configuration DTOs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-use adaptive config from http_reaction
use super::http_reaction::AdaptiveBatchConfigDto;

/// Local copy of gRPC reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrpcReactionConfigDto {
    #[serde(default = "default_grpc_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_grpc_reaction_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_grpc_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_batch_flush_timeout_ms")]
    pub batch_flush_timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_connection_retry_attempts")]
    pub connection_retry_attempts: u32,
    #[serde(default = "default_initial_connection_timeout_ms")]
    pub initial_connection_timeout_ms: u64,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_grpc_endpoint() -> String {
    "grpc://localhost:50052".to_string()
}

fn default_grpc_reaction_timeout_ms() -> u64 {
    5000
}

fn default_grpc_batch_size() -> usize {
    100
}

fn default_batch_flush_timeout_ms() -> u64 {
    1000
}

fn default_max_retries() -> u32 {
    3
}

fn default_connection_retry_attempts() -> u32 {
    5
}

fn default_initial_connection_timeout_ms() -> u64 {
    10000
}

/// Local copy of gRPC adaptive reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrpcAdaptiveReactionConfigDto {
    #[serde(default = "default_grpc_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_grpc_reaction_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_connection_retry_attempts")]
    pub connection_retry_attempts: u32,
    #[serde(default = "default_initial_connection_timeout_ms")]
    pub initial_connection_timeout_ms: u64,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(flatten)]
    pub adaptive: AdaptiveBatchConfigDto,
}
