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

//! HTTP source configuration mapper.

use crate::api::models::HttpSourceConfigDto;
use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use drasi_source_http::HttpSourceConfig;

pub struct HttpSourceConfigMapper;

impl ConfigMapper<HttpSourceConfigDto, HttpSourceConfig> for HttpSourceConfigMapper {
    fn map(&self, dto: &HttpSourceConfigDto, resolver: &DtoMapper) -> Result<HttpSourceConfig, MappingError> {
        Ok(HttpSourceConfig {
            host: resolver.resolve_string(&dto.host)?,
            port: resolver.resolve_typed(&dto.port)?,
            endpoint: resolver.resolve_optional(&dto.endpoint)?,
            timeout_ms: resolver.resolve_typed(&dto.timeout_ms)?,
            adaptive_max_batch_size: resolver.resolve_optional(&dto.adaptive_max_batch_size)?,
            adaptive_min_batch_size: resolver.resolve_optional(&dto.adaptive_min_batch_size)?,
            adaptive_max_wait_ms: resolver.resolve_optional(&dto.adaptive_max_wait_ms)?,
            adaptive_min_wait_ms: resolver.resolve_optional(&dto.adaptive_min_wait_ms)?,
            adaptive_window_secs: resolver.resolve_optional(&dto.adaptive_window_secs)?,
            adaptive_enabled: resolver.resolve_optional(&dto.adaptive_enabled)?,
        })
    }
}
