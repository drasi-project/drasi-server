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

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::log::{LogReactionConfigDto, QueryConfigDto, TemplateSpecDto};
use drasi_reaction_log::{LogReactionConfig, QueryConfig, TemplateSpec};
use std::collections::HashMap;

pub struct LogReactionConfigMapper;

fn map_template_spec(dto: &TemplateSpecDto) -> TemplateSpec {
    TemplateSpec {
        template: dto.template.clone(),
    }
}

fn map_query_config(dto: &QueryConfigDto) -> QueryConfig {
    QueryConfig {
        added: dto.added.as_ref().map(map_template_spec),
        updated: dto.updated.as_ref().map(map_template_spec),
        deleted: dto.deleted.as_ref().map(map_template_spec),
    }
}

impl ConfigMapper<LogReactionConfigDto, LogReactionConfig> for LogReactionConfigMapper {
    fn map(
        &self,
        dto: &LogReactionConfigDto,
        _resolver: &DtoMapper,
    ) -> Result<LogReactionConfig, MappingError> {
        let routes: HashMap<String, QueryConfig> = dto
            .routes
            .iter()
            .map(|(k, v)| (k.clone(), map_query_config(v)))
            .collect();

        Ok(LogReactionConfig {
            routes,
            default_template: dto.default_template.as_ref().map(map_query_config),
        })
    }
}
