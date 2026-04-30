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

//! Upgrade plan data structures and state machine.
//!
//! The upgrade lifecycle follows: Planned → InProgress → Complete/RolledBack/Failed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The overall upgrade plan tracking a plugin's rolling migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePlan {
    /// Unique identifier for this upgrade plan.
    pub id: String,
    /// Plugin kind being upgraded (e.g., "source/postgres").
    pub plugin_kind: String,
    /// Version being upgraded from.
    pub from_version: String,
    /// Version being upgraded to.
    pub to_version: String,
    /// Path to the new plugin binary.
    pub new_binary_path: PathBuf,
    /// Components targeted for migration.
    pub targets: Vec<UpgradeTarget>,
    /// Overall upgrade status.
    pub status: UpgradeStatus,
    /// When the plan was created.
    pub planned_at: DateTime<Utc>,
    /// When execution started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the upgrade completed (success, rollback, or failure).
    pub completed_at: Option<DateTime<Utc>>,
}

/// A single component targeted for upgrade.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeTarget {
    /// Instance ID the component belongs to.
    pub instance_id: String,
    /// Component identifier.
    pub component_id: String,
    /// Whether this is a source or reaction.
    pub component_type: ComponentKind,
    /// Per-component upgrade status.
    pub status: ComponentUpgradeStatus,
    /// Error message if the component failed.
    pub error: Option<String>,
    /// When this component's upgrade started.
    pub started_at: Option<DateTime<Utc>>,
    /// When this component's upgrade completed.
    pub completed_at: Option<DateTime<Utc>>,
}

/// The kind of component being upgraded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ComponentKind {
    Source,
    Reaction,
}

impl std::fmt::Display for ComponentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source => write!(f, "source"),
            Self::Reaction => write!(f, "reaction"),
        }
    }
}

/// Overall upgrade plan status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpgradeStatus {
    /// Plan created, not yet executing.
    Planned,
    /// Rolling migration in progress.
    InProgress,
    /// All components upgraded successfully.
    Complete,
    /// Rolling back upgraded components to old version.
    RollingBack,
    /// Rollback completed — all components restored to old version.
    RolledBack,
    /// Upgrade failed (with message).
    Failed { message: String },
}

impl std::fmt::Display for UpgradeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planned => write!(f, "planned"),
            Self::InProgress => write!(f, "inProgress"),
            Self::Complete => write!(f, "complete"),
            Self::RollingBack => write!(f, "rollingBack"),
            Self::RolledBack => write!(f, "rolledBack"),
            Self::Failed { message } => write!(f, "failed: {message}"),
        }
    }
}

/// Per-component upgrade status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ComponentUpgradeStatus {
    /// Waiting to be upgraded.
    Pending,
    /// Currently being upgraded (stop → swap → start).
    Upgrading,
    /// Successfully upgraded to new version.
    Upgraded,
    /// Upgrade failed for this component.
    Failed,
    /// Component was rolled back to old version.
    RolledBack,
    /// Component was skipped (e.g., already on target version).
    Skipped,
}

impl UpgradePlan {
    /// Count how many targets have been successfully upgraded.
    pub fn upgraded_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|t| t.status == ComponentUpgradeStatus::Upgraded)
            .count()
    }

    /// Count how many targets are still pending.
    pub fn pending_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|t| t.status == ComponentUpgradeStatus::Pending)
            .count()
    }

    /// Count how many targets failed.
    pub fn failed_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|t| t.status == ComponentUpgradeStatus::Failed)
            .count()
    }

    /// Check if the plan can transition to execution.
    pub fn can_execute(&self) -> bool {
        self.status == UpgradeStatus::Planned
    }

    /// Check if the plan can be rolled back.
    pub fn can_rollback(&self) -> bool {
        matches!(
            self.status,
            UpgradeStatus::InProgress | UpgradeStatus::Failed { .. }
        )
    }

    /// Check if the plan can be cancelled.
    pub fn can_cancel(&self) -> bool {
        self.status == UpgradeStatus::Planned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> UpgradePlan {
        UpgradePlan {
            id: "test-001".to_string(),
            plugin_kind: "source/postgres".to_string(),
            from_version: "1.0.0".to_string(),
            to_version: "2.0.0".to_string(),
            new_binary_path: PathBuf::from("/tmp/new-plugin.so"),
            targets: vec![
                UpgradeTarget {
                    instance_id: "inst-1".to_string(),
                    component_id: "src-a".to_string(),
                    component_type: ComponentKind::Source,
                    status: ComponentUpgradeStatus::Pending,
                    error: None,
                    started_at: None,
                    completed_at: None,
                },
                UpgradeTarget {
                    instance_id: "inst-1".to_string(),
                    component_id: "src-b".to_string(),
                    component_type: ComponentKind::Source,
                    status: ComponentUpgradeStatus::Pending,
                    error: None,
                    started_at: None,
                    completed_at: None,
                },
            ],
            status: UpgradeStatus::Planned,
            planned_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    #[test]
    fn test_plan_state_transitions() {
        let plan = sample_plan();
        assert!(plan.can_execute());
        assert!(!plan.can_rollback());
        assert!(plan.can_cancel());
    }

    #[test]
    fn test_plan_in_progress_can_rollback() {
        let mut plan = sample_plan();
        plan.status = UpgradeStatus::InProgress;
        assert!(!plan.can_execute());
        assert!(plan.can_rollback());
        assert!(!plan.can_cancel());
    }

    #[test]
    fn test_plan_failed_can_rollback() {
        let mut plan = sample_plan();
        plan.status = UpgradeStatus::Failed {
            message: "oops".to_string(),
        };
        assert!(!plan.can_execute());
        assert!(plan.can_rollback());
        assert!(!plan.can_cancel());
    }

    #[test]
    fn test_plan_complete_no_transitions() {
        let mut plan = sample_plan();
        plan.status = UpgradeStatus::Complete;
        assert!(!plan.can_execute());
        assert!(!plan.can_rollback());
        assert!(!plan.can_cancel());
    }

    #[test]
    fn test_counts() {
        let mut plan = sample_plan();
        assert_eq!(plan.pending_count(), 2);
        assert_eq!(plan.upgraded_count(), 0);

        plan.targets[0].status = ComponentUpgradeStatus::Upgraded;
        assert_eq!(plan.pending_count(), 1);
        assert_eq!(plan.upgraded_count(), 1);
    }
}
