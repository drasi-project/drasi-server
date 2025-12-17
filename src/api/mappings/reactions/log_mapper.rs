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

//! Log reaction configuration mapper.

use crate::api::models::LogReactionConfigDto;
use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use drasi_reaction_log::LogReactionConfig;

pub struct LogReactionConfigMapper;

impl ConfigMapper<LogReactionConfigDto, LogReactionConfig> for LogReactionConfigMapper {
    fn map(&self, dto: &LogReactionConfigDto, resolver: &DtoMapper) -> Result<LogReactionConfig, MappingError> {
        Ok(LogReactionConfig {
            added_template: resolver.resolve_optional(&dto.added_template)?,
            updated_template: resolver.resolve_optional(&dto.updated_template)?,
            deleted_template: resolver.resolve_optional(&dto.deleted_template)?,
        })
    }
}
