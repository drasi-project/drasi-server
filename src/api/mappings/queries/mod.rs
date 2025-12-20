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

//! Query configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::{QueryConfigDto, SourceSubscriptionConfigDto};
use drasi_lib::config::{QueryConfig, SourceSubscriptionConfig};

pub struct QueryConfigMapper;

impl ConfigMapper<QueryConfigDto, QueryConfig> for QueryConfigMapper {
    fn map(
        &self,
        dto: &QueryConfigDto,
        resolver: &DtoMapper,
    ) -> Result<QueryConfig, MappingError> {
        let sources = dto
            .sources
            .iter()
            .map(|src| map_source_subscription(src, resolver))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(QueryConfig {
            id: dto.id.clone(),
            auto_start: dto.auto_start,
            query: resolver.resolve_string(&dto.query)?,
            query_language: resolver.resolve_string(&dto.query_language)?,
            middleware: dto.middleware.clone(),
            sources,
            enable_bootstrap: dto.enable_bootstrap,
            bootstrap_buffer_size: dto.bootstrap_buffer_size,
        })
    }
}

fn map_source_subscription(
    dto: &SourceSubscriptionConfigDto,
    resolver: &DtoMapper,
) -> Result<SourceSubscriptionConfig, MappingError> {
    Ok(SourceSubscriptionConfig {
        source_id: resolver.resolve_string(&dto.source_id)?,
        pipeline: dto.pipeline.clone(),
    })
}
