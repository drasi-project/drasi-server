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

//! Platform source configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::PlatformSourceConfigDto;
use drasi_source_platform::PlatformSourceConfig;

pub struct PlatformSourceConfigMapper;

impl ConfigMapper<PlatformSourceConfigDto, PlatformSourceConfig> for PlatformSourceConfigMapper {
    fn map(
        &self,
        dto: &PlatformSourceConfigDto,
        resolver: &DtoMapper,
    ) -> Result<PlatformSourceConfig, MappingError> {
        Ok(PlatformSourceConfig {
            redis_url: resolver.resolve_string(&dto.redis_url)?,
            stream_key: resolver.resolve_string(&dto.stream_key)?,
            consumer_group: resolver.resolve_string(&dto.consumer_group)?,
            consumer_name: resolver.resolve_optional(&dto.consumer_name)?,
            batch_size: resolver.resolve_typed(&dto.batch_size)?,
            block_ms: resolver.resolve_typed(&dto.block_ms)?,
        })
    }
}
