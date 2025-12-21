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

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::sse::{SseQueryConfigDto, SseReactionConfigDto, SseTemplateSpecDto};
use drasi_reaction_sse::{QueryConfig, SseReactionConfig, TemplateSpec};
use std::collections::HashMap;

pub struct SseReactionConfigMapper;

fn map_template_spec(dto: &SseTemplateSpecDto) -> TemplateSpec {
    TemplateSpec {
        path: dto.path.clone(),
        template: dto.template.clone(),
    }
}

fn map_query_config(dto: &SseQueryConfigDto) -> QueryConfig {
    QueryConfig {
        added: dto.added.as_ref().map(map_template_spec),
        updated: dto.updated.as_ref().map(map_template_spec),
        deleted: dto.deleted.as_ref().map(map_template_spec),
    }
}

impl ConfigMapper<SseReactionConfigDto, SseReactionConfig> for SseReactionConfigMapper {
    fn map(
        &self,
        dto: &SseReactionConfigDto,
        resolver: &DtoMapper,
    ) -> Result<SseReactionConfig, MappingError> {
        let routes: HashMap<String, QueryConfig> = dto
            .routes
            .iter()
            .map(|(k, v)| (k.clone(), map_query_config(v)))
            .collect();

        Ok(SseReactionConfig {
            host: resolver.resolve_string(&dto.host)?,
            port: resolver.resolve_typed(&dto.port)?,
            sse_path: resolver.resolve_string(&dto.sse_path)?,
            heartbeat_interval_ms: resolver.resolve_typed(&dto.heartbeat_interval_ms)?,
            routes,
            default_template: dto.default_template.as_ref().map(map_query_config),
        })
    }
}
