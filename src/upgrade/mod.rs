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

//! Rolling plugin upgrade system.
//!
//! This module provides zero-downtime plugin upgrades using a rolling migration
//! strategy. Components are upgraded one-at-a-time from old to new plugin version
//! using the existing `update_source`/`update_reaction` infrastructure.
//!
//! # Architecture
//!
//! - [`plan`] — Data structures for upgrade plans and state machine
//! - [`validation`] — ABI compatibility validation between plugin versions
//! - [`engine`] — Orchestration of plan/execute/rollback lifecycle
//! - [`error`] — Error types for the upgrade system

pub mod engine;
pub mod error;
pub mod plan;
pub mod validation;

// Re-export key types
pub use engine::UpgradeEngine;
pub use error::UpgradeError;
pub use plan::{ComponentKind, ComponentUpgradeStatus, UpgradePlan, UpgradeStatus, UpgradeTarget};
pub use validation::{validate_abi_compatibility, PluginVersionInfo};
