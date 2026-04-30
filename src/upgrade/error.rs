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

//! Error types for the plugin upgrade system.

use thiserror::Error;

/// Errors that can occur during plugin upgrade operations.
#[derive(Debug, Error)]
pub enum UpgradeError {
    /// The specified plugin is not currently loaded.
    #[error("Plugin '{plugin_kind}' is not loaded")]
    PluginNotLoaded { plugin_kind: String },

    /// ABI version mismatch between current and candidate plugins.
    #[error(
        "ABI mismatch: current SDK {current_sdk}, candidate SDK {candidate_sdk}. \
         Major.minor must match for dual-load upgrade. Use restart-upgrade for cross-ABI upgrades."
    )]
    AbiMismatch {
        current_sdk: String,
        candidate_sdk: String,
    },

    /// Target platform (target triple) mismatch.
    #[error("Target mismatch: current '{current}', candidate '{candidate}'")]
    TargetMismatch { current: String, candidate: String },

    /// An upgrade is already in progress for this plugin kind.
    #[error("An upgrade is already in progress for plugin '{plugin_kind}'")]
    UpgradeAlreadyInProgress { plugin_kind: String },

    /// The specified upgrade plan was not found.
    #[error("Upgrade plan '{plan_id}' not found")]
    PlanNotFound { plan_id: String },

    /// The plan is in an invalid state for the requested operation.
    #[error("Upgrade plan '{plan_id}' is in state '{status}', cannot {operation}")]
    InvalidPlanState {
        plan_id: String,
        status: String,
        operation: String,
    },

    /// A component failed to upgrade.
    #[error("Component '{component_id}' failed to upgrade: {reason}")]
    ComponentFailed {
        component_id: String,
        reason: String,
    },

    /// The old plugin is no longer available (needed for rollback).
    #[error("Old plugin factory not available for rollback of '{plugin_kind}'")]
    OldPluginNotAvailable { plugin_kind: String },

    /// Failed to load the new plugin binary.
    #[error("Failed to load new plugin binary: {reason}")]
    LoadFailed { reason: String },

    /// Failed to find dependents of the plugin.
    #[error("Failed to find dependents: {reason}")]
    DependentLookupFailed { reason: String },

    /// The upgrade plan cannot be cancelled because it's already executing.
    #[error("Cannot cancel plan '{plan_id}': already executing")]
    CannotCancel { plan_id: String },

    /// A generic internal error.
    #[error("Internal upgrade error: {0}")]
    Internal(String),
}
