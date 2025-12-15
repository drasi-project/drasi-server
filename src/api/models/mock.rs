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

use serde::{Deserialize, Serialize};

/// Local copy of mock source configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MockSourceConfigDto {
    #[serde(default = "default_data_type")]
    pub data_type: String,
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
}

fn default_data_type() -> String {
    "generic".to_string()
}

fn default_interval_ms() -> u64 {
    5000
}
