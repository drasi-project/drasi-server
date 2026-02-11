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

//! Mock source configuration DTOs.

use crate::api::models::ConfigValue;
use serde::{Deserialize, Serialize};

fn default_sensor_count() -> u32 {
    5
}

/// Type of data to generate from the mock source.
///
/// This mirrors the `DataType` enum from drasi-source-mock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum DataTypeDto {
    /// Sequential counter values (Counter nodes)
    Counter,
    /// Simulated sensor readings with temperature and humidity (SensorReading nodes)
    /// First reading for each sensor generates INSERT, subsequent readings generate UPDATE
    SensorReading {
        /// Number of sensors to simulate (default: 5)
        #[serde(default = "default_sensor_count", rename = "sensorCount")]
        sensor_count: u32,
    },
    /// Generic random data (Generic nodes) - default mode
    #[default]
    Generic,
}

/// Local copy of mock source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MockSourceConfigDto {
    /// Type of data to generate as an enum object:
    /// - { type: "counter" }
    /// - { type: "sensorReading", sensorCount: 10 }
    /// - { type: "generic" }
    #[serde(default)]
    pub data_type: DataTypeDto,
    /// Interval between data generation events in milliseconds
    #[serde(default = "default_interval_ms")]
    pub interval_ms: ConfigValue<u64>,
}

fn default_interval_ms() -> ConfigValue<u64> {
    ConfigValue::Static(5000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::SourceConfig;

    #[test]
    fn test_mock_source_config_deserializes_sensor_reading_with_count() {
        let yaml = r#"
kind: mock
id: test-source
autoStart: true
dataType:
  type: sensorReading
  sensorCount: 10
intervalMs: 3000
"#;

        let config: SourceConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        match config {
            SourceConfig::Mock {
                id,
                auto_start,
                config,
                ..
            } => {
                assert_eq!(id, "test-source");
                assert!(auto_start);
                assert_eq!(
                    config.data_type,
                    DataTypeDto::SensorReading { sensor_count: 10 }
                );
                assert_eq!(config.interval_ms, ConfigValue::Static(3000));
            }
            _ => panic!("Expected Mock variant"),
        }
    }

    #[test]
    fn test_mock_source_config_sensor_reading_default_count() {
        let yaml = r#"
kind: mock
id: test-source
dataType:
  type: sensorReading
"#;

        let config: SourceConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        match config {
            SourceConfig::Mock { config, .. } => {
                assert_eq!(
                    config.data_type,
                    DataTypeDto::SensorReading { sensor_count: 5 }
                );
            }
            _ => panic!("Expected Mock variant"),
        }
    }

    #[test]
    fn test_mock_source_config_counter_type() {
        let yaml = r#"
kind: mock
id: counter-source
dataType:
  type: counter
"#;

        let config: SourceConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        match config {
            SourceConfig::Mock { config, .. } => {
                assert_eq!(config.data_type, DataTypeDto::Counter);
            }
            _ => panic!("Expected Mock variant"),
        }
    }

    #[test]
    fn test_mock_source_config_generic_type() {
        let yaml = r#"
kind: mock
id: generic-source
dataType:
  type: generic
"#;

        let config: SourceConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        match config {
            SourceConfig::Mock { config, .. } => {
                assert_eq!(config.data_type, DataTypeDto::Generic);
            }
            _ => panic!("Expected Mock variant"),
        }
    }

    #[test]
    fn test_mock_source_config_uses_defaults() {
        let yaml = r#"
kind: mock
id: default-source
"#;

        let config: SourceConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        match config {
            SourceConfig::Mock {
                id,
                auto_start,
                config,
                ..
            } => {
                assert_eq!(id, "default-source");
                assert!(auto_start, "auto_start should default to true");
                assert_eq!(config.data_type, DataTypeDto::Generic);
                assert_eq!(config.interval_ms, ConfigValue::Static(5000));
            }
            _ => panic!("Expected Mock variant"),
        }
    }
}
