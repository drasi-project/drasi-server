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

//! gRPC adaptive reaction configuration mapper.

use crate::api::models::*;
use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use drasi_reaction_grpc_adaptive::GrpcAdaptiveReactionConfig;
use drasi_lib::reactions::common::AdaptiveBatchConfig;
use std::collections::HashMap;

pub struct GrpcAdaptiveReactionConfigMapper;

impl ConfigMapper<GrpcAdaptiveReactionConfigDto, GrpcAdaptiveReactionConfig> for GrpcAdaptiveReactionConfigMapper {
    fn map(&self, dto: &GrpcAdaptiveReactionConfigDto, resolver: &DtoMapper) -> Result<GrpcAdaptiveReactionConfig, MappingError> {
        let adaptive = AdaptiveBatchConfig {
            adaptive_min_batch_size: resolver.resolve_typed(&dto.adaptive.adaptive_min_batch_size)?,
            adaptive_max_batch_size: resolver.resolve_typed(&dto.adaptive.adaptive_max_batch_size)?,
            adaptive_window_size: resolver.resolve_typed(&dto.adaptive.adaptive_window_size)?,
            adaptive_batch_timeout_ms: resolver.resolve_typed(&dto.adaptive.adaptive_batch_timeout_ms)?,
        };

        Ok(GrpcAdaptiveReactionConfig {
            endpoint: resolver.resolve_string(&dto.endpoint)?,
            timeout_ms: resolver.resolve_typed(&dto.timeout_ms)?,
            max_retries: resolver.resolve_typed(&dto.max_retries)?,
            connection_retry_attempts: resolver.resolve_typed(&dto.connection_retry_attempts)?,
            initial_connection_timeout_ms: resolver.resolve_typed(&dto.initial_connection_timeout_ms)?,
            metadata: resolve_hashmap(&dto.metadata, resolver)?,
            adaptive,
        })
    }
}

// Helper function to resolve HashMap<String, ConfigValue<String>>
fn resolve_hashmap(map: &HashMap<String, ConfigValue<String>>, resolver: &DtoMapper) -> Result<HashMap<String, String>, MappingError> {
    let mut result = HashMap::new();
    for (key, value) in map {
        result.insert(key.clone(), resolver.resolve_string(value)?);
    }
    Ok(result)
}
