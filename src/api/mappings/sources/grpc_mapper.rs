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

//! gRPC source configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::GrpcSourceConfigDto;
use drasi_source_grpc::GrpcSourceConfig;

pub struct GrpcSourceConfigMapper;

impl ConfigMapper<GrpcSourceConfigDto, GrpcSourceConfig> for GrpcSourceConfigMapper {
    fn map(
        &self,
        dto: &GrpcSourceConfigDto,
        resolver: &DtoMapper,
    ) -> Result<GrpcSourceConfig, MappingError> {
        Ok(GrpcSourceConfig {
            host: resolver.resolve_string(&dto.host)?,
            port: resolver.resolve_typed(&dto.port)?,
            endpoint: resolver.resolve_optional(&dto.endpoint)?,
            timeout_ms: resolver.resolve_typed(&dto.timeout_ms)?,
        })
    }
}
