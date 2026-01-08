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
use drasi_lib::channels::DispatchMode;
use drasi_lib::config::{QueryConfig, QueryLanguage, SourceSubscriptionConfig};

pub struct QueryConfigMapper;

impl ConfigMapper<QueryConfigDto, QueryConfig> for QueryConfigMapper {
    fn map(&self, dto: &QueryConfigDto, resolver: &DtoMapper) -> Result<QueryConfig, MappingError> {
        let sources = dto
            .sources
            .iter()
            .map(|src| map_source_subscription(src, resolver))
            .collect::<Result<Vec<_>, _>>()?;

        // Parse query language string to enum
        let query_language_str = resolver.resolve_string(&dto.query_language)?;
        let query_language = match query_language_str.as_str() {
            "Cypher" => QueryLanguage::Cypher,
            "GQL" => QueryLanguage::GQL,
            _ => {
                return Err(MappingError::SourceCreationError(format!(
                    "Invalid query language: {query_language_str}. Must be 'Cypher' or 'GQL'"
                )))
            }
        };

        // Middleware is empty for now (optional field, defaults to empty vec)
        let middleware = Vec::new();

        // Parse dispatch mode if provided
        let dispatch_mode = dto
            .dispatch_mode
            .as_ref()
            .map(|dm| match dm.as_str() {
                "Channel" => Ok(DispatchMode::Channel),
                _ => Err(MappingError::SourceCreationError(format!(
                    "Invalid dispatch mode: {dm}. Must be 'Channel'"
                ))),
            })
            .transpose()?;

        // Parse joins if provided
        let joins = dto
            .joins
            .as_ref()
            .map(|j| {
                serde_json::from_value(j.clone()).map_err(|e| {
                    MappingError::SourceCreationError(format!("Invalid joins config: {e}"))
                })
            })
            .transpose()?;

        // Parse storage_backend if provided
        let storage_backend = dto
            .storage_backend
            .as_ref()
            .map(|sb| {
                serde_json::from_value(sb.clone()).map_err(|e| {
                    MappingError::SourceCreationError(format!(
                        "Invalid storage backend config: {e}"
                    ))
                })
            })
            .transpose()?;

        Ok(QueryConfig {
            id: dto.id.clone(),
            auto_start: dto.auto_start,
            query: resolver.resolve_string(&dto.query)?,
            query_language,
            middleware,
            sources,
            enable_bootstrap: dto.enable_bootstrap,
            bootstrap_buffer_size: dto.bootstrap_buffer_size,
            joins,
            priority_queue_capacity: dto.priority_queue_capacity,
            dispatch_buffer_capacity: dto.dispatch_buffer_capacity,
            dispatch_mode,
            storage_backend,
        })
    }
}

fn map_source_subscription(
    dto: &SourceSubscriptionConfigDto,
    resolver: &DtoMapper,
) -> Result<SourceSubscriptionConfig, MappingError> {
    Ok(SourceSubscriptionConfig {
        source_id: resolver.resolve_string(&dto.source_id)?,
        nodes: dto.nodes.clone(),
        relations: dto.relations.clone(),
        pipeline: dto.pipeline.clone(),
    })
}
