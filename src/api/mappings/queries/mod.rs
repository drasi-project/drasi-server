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
use crate::api::models::QueryConfigDto;
use drasi_core::models::SourceMiddlewareConfig;
use drasi_lib::channels::DispatchMode;
use drasi_lib::config::{QueryConfig, QueryInputConfig, QueryResultConfig};
use std::sync::Arc;

pub struct QueryConfigMapper;

impl ConfigMapper<QueryConfigDto, QueryConfig> for QueryConfigMapper {
    fn map(
        &self,
        dto: &QueryConfigDto,
        _resolver: &DtoMapper,
    ) -> Result<QueryConfig, MappingError> {
        let inputs = dto.inputs.iter().map(QueryInputConfig::from).collect();

        let middleware = dto
            .middleware
            .iter()
            .map(|m| SourceMiddlewareConfig {
                kind: Arc::from(m.kind.as_str()),
                name: Arc::from(m.name.as_str()),
                config: m.config.clone(),
            })
            .collect();

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

        let config = QueryConfig {
            id: dto.id.clone(),
            auto_start: dto.auto_start,
            query: dto.query.clone(),
            query_language: dto.query_language.clone(),
            middleware,
            inputs,
            enable_bootstrap: dto.enable_bootstrap,
            joins,
            priority_queue_capacity: dto.priority_queue_capacity,
            dispatch_buffer_capacity: dto.dispatch_buffer_capacity,
            dispatch_mode,
            storage_backend,
            recovery_policy: None,
            outbox_capacity: dto.outbox_capacity,
            bootstrap_timeout_secs: dto.bootstrap_timeout_secs,
            result: QueryResultConfig {
                label: dto.result.label.clone(),
            },
        };
        config
            .validate_declarations()
            .map_err(|e| MappingError::SourceCreationError(e.to_string()))?;
        Ok(config)
    }
}
