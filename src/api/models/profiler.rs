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

//! Profiler reaction configuration DTOs.

use serde::{Deserialize, Serialize};
use crate::api::models::ConfigValue;

/// Local copy of profiler reaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfilerReactionConfigDto {
    #[serde(default = "default_profiler_window_size")]
    pub window_size: ConfigValue<usize>,
    #[serde(default = "default_report_interval_secs")]
    pub report_interval_secs: ConfigValue<u64>,
}

fn default_profiler_window_size() -> ConfigValue<usize> {
    ConfigValue::Static(100)
}

fn default_report_interval_secs() -> ConfigValue<u64> {
    ConfigValue::Static(60)
}
