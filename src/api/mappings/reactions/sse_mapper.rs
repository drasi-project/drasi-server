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

//! SSE reaction configuration mapper.

use crate::api::models::SseReactionConfigDto;
use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use drasi_reaction_sse::SseReactionConfig;

pub struct SseReactionConfigMapper;

impl ConfigMapper<SseReactionConfigDto, SseReactionConfig> for SseReactionConfigMapper {
    fn map(&self, dto: &SseReactionConfigDto, resolver: &DtoMapper) -> Result<SseReactionConfig, MappingError> {
        Ok(SseReactionConfig {
            host: resolver.resolve_string(&dto.host)?,
            port: resolver.resolve_typed(&dto.port)?,
            sse_path: resolver.resolve_string(&dto.sse_path)?,
            heartbeat_interval_ms: resolver.resolve_typed(&dto.heartbeat_interval_ms)?,
        })
    }
}
