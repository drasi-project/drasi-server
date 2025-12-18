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

//! gRPC reaction configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::*;
use drasi_reaction_grpc::GrpcReactionConfig;
use std::collections::HashMap;

pub struct GrpcReactionConfigMapper;

impl ConfigMapper<GrpcReactionConfigDto, GrpcReactionConfig> for GrpcReactionConfigMapper {
    fn map(
        &self,
        dto: &GrpcReactionConfigDto,
        resolver: &DtoMapper,
    ) -> Result<GrpcReactionConfig, MappingError> {
        Ok(GrpcReactionConfig {
            endpoint: resolver.resolve_string(&dto.endpoint)?,
            timeout_ms: resolver.resolve_typed(&dto.timeout_ms)?,
            batch_size: resolver.resolve_typed(&dto.batch_size)?,
            batch_flush_timeout_ms: resolver.resolve_typed(&dto.batch_flush_timeout_ms)?,
            max_retries: resolver.resolve_typed(&dto.max_retries)?,
            connection_retry_attempts: resolver.resolve_typed(&dto.connection_retry_attempts)?,
            initial_connection_timeout_ms: resolver
                .resolve_typed(&dto.initial_connection_timeout_ms)?,
            metadata: resolve_hashmap(&dto.metadata, resolver)?,
        })
    }
}

// Helper function to resolve HashMap<String, ConfigValue<String>>
fn resolve_hashmap(
    map: &HashMap<String, ConfigValue<String>>,
    resolver: &DtoMapper,
) -> Result<HashMap<String, String>, MappingError> {
    let mut result = HashMap::new();
    for (key, value) in map {
        result.insert(key.clone(), resolver.resolve_string(value)?);
    }
    Ok(result)
}
