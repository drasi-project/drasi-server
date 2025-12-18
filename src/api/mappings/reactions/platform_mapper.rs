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

//! Platform reaction configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::PlatformReactionConfigDto;
use drasi_reaction_platform::PlatformReactionConfig;

pub struct PlatformReactionConfigMapper;

impl ConfigMapper<PlatformReactionConfigDto, PlatformReactionConfig>
    for PlatformReactionConfigMapper
{
    fn map(
        &self,
        dto: &PlatformReactionConfigDto,
        resolver: &DtoMapper,
    ) -> Result<PlatformReactionConfig, MappingError> {
        Ok(PlatformReactionConfig {
            redis_url: resolver.resolve_string(&dto.redis_url)?,
            pubsub_name: resolver.resolve_optional(&dto.pubsub_name)?,
            source_name: resolver.resolve_optional(&dto.source_name)?,
            max_stream_length: resolver.resolve_optional(&dto.max_stream_length)?,
            emit_control_events: resolver.resolve_typed(&dto.emit_control_events)?,
            batch_enabled: resolver.resolve_typed(&dto.batch_enabled)?,
            batch_max_size: resolver.resolve_typed(&dto.batch_max_size)?,
            batch_max_wait_ms: resolver.resolve_typed(&dto.batch_max_wait_ms)?,
        })
    }
}
