// Copyright 2026 The Drasi Authors.
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

//! Server configuration for the implementation behind normal component APIs.

use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
    clap::ValueEnum,
)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionModeConfig {
    #[default]
    ComponentGraph,
    /// Adds acknowledge graph node creation. Validation, initialization and
    /// automatic activation continue on the node; inspect its status for the
    /// outcome. Explicit start operations still report startup failures.
    ComputationGraph,
}

impl ExecutionModeConfig {
    pub fn is_default(&self) -> bool {
        *self == Self::ComponentGraph
    }
}

impl From<ExecutionModeConfig> for drasi_lib::ExecutionMode {
    fn from(mode: ExecutionModeConfig) -> Self {
        match mode {
            ExecutionModeConfig::ComponentGraph => Self::ComponentGraph,
            ExecutionModeConfig::ComputationGraph => Self::ComputationGraph,
        }
    }
}

impl From<drasi_lib::ExecutionMode> for ExecutionModeConfig {
    fn from(mode: drasi_lib::ExecutionMode) -> Self {
        match mode {
            drasi_lib::ExecutionMode::ComponentGraph => Self::ComponentGraph,
            drasi_lib::ExecutionMode::ComputationGraph => Self::ComputationGraph,
        }
    }
}

/// Stable server-level policy, independent of the current instance ordering.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecutionModePolicy {
    pub default_mode: ExecutionModeConfig,
    pub forced_mode: Option<ExecutionModeConfig>,
}

impl ExecutionModePolicy {
    pub fn resolve(
        &self,
        requested: Option<ExecutionModeConfig>,
    ) -> anyhow::Result<ExecutionModeConfig> {
        if let (Some(forced), Some(requested)) = (self.forced_mode, requested) {
            anyhow::ensure!(
                forced == requested,
                "executionMode conflicts with the server's forced execution mode"
            );
        }
        Ok(self.forced_mode.or(requested).unwrap_or(self.default_mode))
    }
}
