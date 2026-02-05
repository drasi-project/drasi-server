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

//! Mock source configuration mapper.

use crate::api::mappings::{ConfigMapper, DtoMapper, MappingError};
use crate::api::models::sources::mock::DataTypeDto;
use crate::api::models::MockSourceConfigDto;
use drasi_source_mock::{DataType, MockSourceConfig};

pub struct MockSourceConfigMapper;

impl ConfigMapper<MockSourceConfigDto, MockSourceConfig> for MockSourceConfigMapper {
    fn map(
        &self,
        dto: &MockSourceConfigDto,
        resolver: &DtoMapper,
    ) -> Result<MockSourceConfig, MappingError> {
        // Map DataTypeDto to DataType
        let data_type = match &dto.data_type {
            DataTypeDto::Counter => DataType::Counter,
            DataTypeDto::SensorReading { sensor_count } => DataType::SensorReading {
                sensor_count: *sensor_count,
            },
            DataTypeDto::Generic => DataType::Generic,
        };

        Ok(MockSourceConfig {
            data_type,
            interval_ms: resolver.resolve_typed(&dto.interval_ms)?,
        })
    }
}
