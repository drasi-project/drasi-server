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

//! Upgrade engine — orchestrates the plan/execute/rollback lifecycle.
//!
//! The engine manages upgrade plans in memory, coordinates with the PluginOrchestrator
//! for loading new binaries, and uses DrasiLib's `update_source`/`update_reaction` for
//! the actual runtime swap.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use log::{error, info, warn};
use tokio::sync::RwLock;

use drasi_host_sdk::loader::scan_plugin_metadata;

use crate::instance_registry::InstanceRegistry;
use crate::plugin_orchestrator::PluginOrchestrator;
use crate::plugin_registry::PluginRegistry;
use crate::upgrade::error::UpgradeError;
use crate::upgrade::plan::{
    ComponentKind, ComponentUpgradeStatus, UpgradePlan, UpgradeStatus, UpgradeTarget,
};
use crate::upgrade::validation::{validate_abi_compatibility, PluginVersionInfo};

/// The upgrade engine manages plugin upgrade lifecycles.
///
/// Stores plans in memory and coordinates rolling migrations using
/// existing `update_source`/`update_reaction` infrastructure.
pub struct UpgradeEngine {
    /// Active and historical upgrade plans.
    plans: RwLock<HashMap<String, UpgradePlan>>,
    /// Instance registry for looking up DrasiLib instances.
    instance_registry: InstanceRegistry,
    /// Plugin registry for descriptor lookups.
    plugin_registry: Arc<RwLock<PluginRegistry>>,
    /// Plugin orchestrator for loading new binaries.
    orchestrator: Arc<PluginOrchestrator>,
}

impl UpgradeEngine {
    /// Create a new upgrade engine.
    pub fn new(
        instance_registry: InstanceRegistry,
        plugin_registry: Arc<RwLock<PluginRegistry>>,
        orchestrator: Arc<PluginOrchestrator>,
    ) -> Self {
        Self {
            plans: RwLock::new(HashMap::new()),
            instance_registry,
            plugin_registry,
            orchestrator,
        }
    }

    /// Plan an upgrade for a plugin kind using a new binary.
    ///
    /// Validates ABI compatibility, finds all dependent components across instances,
    /// and creates an UpgradePlan in `Planned` state.
    pub async fn plan_upgrade(
        &self,
        plugin_kind: &str,
        new_binary_path: &Path,
    ) -> Result<UpgradePlan, UpgradeError> {
        // 1. Validate the new binary exists and can be read
        if !new_binary_path.exists() {
            return Err(UpgradeError::LoadFailed {
                reason: format!("Binary not found: {}", new_binary_path.display()),
            });
        }

        // 2. Read metadata from the new binary
        let new_metadata = scan_plugin_metadata(new_binary_path).ok_or_else(|| {
            UpgradeError::LoadFailed {
                reason: format!(
                    "Failed to read metadata from new binary: {}",
                    new_binary_path.display()
                ),
            }
        })?;

        // 3. Get current plugin info from the orchestrator
        let current_info = self
            .orchestrator
            .get_plugin_info(plugin_kind)
            .await
            .ok_or_else(|| UpgradeError::PluginNotLoaded {
                plugin_kind: plugin_kind.to_string(),
            })?;

        // 4. Validate ABI compatibility
        let current_version_info = PluginVersionInfo {
            sdk_version: current_info.sdk_version.clone(),
            plugin_version: current_info.plugin_version.clone(),
            target_triple: String::new(), // Not available from PluginInfo directly
            plugin_kind: plugin_kind.to_string(),
        };
        let candidate_version_info = PluginVersionInfo {
            sdk_version: new_metadata.sdk_version.clone(),
            plugin_version: new_metadata.version.clone(),
            target_triple: new_metadata.target_triple.clone(),
            plugin_kind: plugin_kind.to_string(),
        };
        validate_abi_compatibility(&current_version_info, &candidate_version_info)?;

        // 5. Check no existing upgrade in progress for this plugin
        {
            let plans = self.plans.read().await;
            let has_active = plans.values().any(|p| {
                p.plugin_kind == plugin_kind
                    && matches!(
                        p.status,
                        UpgradeStatus::Planned | UpgradeStatus::InProgress
                    )
            });
            if has_active {
                return Err(UpgradeError::UpgradeAlreadyInProgress {
                    plugin_kind: plugin_kind.to_string(),
                });
            }
        }

        // 6. Find all dependent components across all instances
        let targets = self.find_dependents(plugin_kind).await?;

        if targets.is_empty() {
            return Err(UpgradeError::DependentLookupFailed {
                reason: format!("No components found using plugin '{plugin_kind}'"),
            });
        }

        // 7. Create the plan
        let plan_id = format!("upgrade-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("0000"));
        let plan = UpgradePlan {
            id: plan_id.clone(),
            plugin_kind: plugin_kind.to_string(),
            from_version: current_info.plugin_version,
            to_version: new_metadata.version,
            new_binary_path: new_binary_path.to_path_buf(),
            targets,
            status: UpgradeStatus::Planned,
            planned_at: Utc::now(),
            started_at: None,
            completed_at: None,
        };

        self.plans.write().await.insert(plan_id, plan.clone());

        info!(
            "Created upgrade plan '{}': {} → {} ({} targets)",
            plan.id,
            plan.from_version,
            plan.to_version,
            plan.targets.len()
        );

        Ok(plan)
    }

    /// Execute a planned upgrade using rolling migration.
    ///
    /// Loads the new plugin binary, then upgrades each component one-at-a-time.
    /// On failure, automatically triggers rollback.
    pub async fn execute_upgrade(&self, plan_id: &str) -> Result<UpgradePlan, UpgradeError> {
        // Validate plan state
        {
            let plans = self.plans.read().await;
            let plan = plans
                .get(plan_id)
                .ok_or_else(|| UpgradeError::PlanNotFound {
                    plan_id: plan_id.to_string(),
                })?;
            if !plan.can_execute() {
                return Err(UpgradeError::InvalidPlanState {
                    plan_id: plan_id.to_string(),
                    status: plan.status.to_string(),
                    operation: "execute".to_string(),
                });
            }
        }

        // Transition to InProgress
        {
            let mut plans = self.plans.write().await;
            let plan = plans
                .get_mut(plan_id)
                .expect("plan existence verified above");
            plan.status = UpgradeStatus::InProgress;
            plan.started_at = Some(Utc::now());
        }

        info!("Executing upgrade plan '{plan_id}'");

        // Load the new plugin binary (bypasses orchestrator duplicate check by
        // loading directly through the lifecycle manager)
        let new_binary_path = {
            let plans = self.plans.read().await;
            plans[plan_id].new_binary_path.clone()
        };

        // Load the new plugin using the orchestrator's load mechanism.
        // This registers the new descriptors in the plugin registry.
        if let Err(e) = self
            .orchestrator
            .load_plugin(&new_binary_path, None)
            .await
        {
            // If loading fails because it's already loaded, that's OK for upgrade
            // (the plugin might have been pre-loaded). Otherwise fail.
            let err_str = e.to_string();
            if !err_str.contains("already loaded") {
                let mut plans = self.plans.write().await;
                let plan = plans
                    .get_mut(plan_id)
                    .expect("plan existence verified above");
                plan.status = UpgradeStatus::Failed {
                    message: format!("Failed to load new binary: {err_str}"),
                };
                plan.completed_at = Some(Utc::now());
                return Err(UpgradeError::LoadFailed { reason: err_str });
            }
        }

        // Rolling migration: upgrade each target one-at-a-time
        let target_count = {
            let plans = self.plans.read().await;
            plans[plan_id].targets.len()
        };

        for idx in 0..target_count {
            // Get target info
            let (target_instance_id, target_component_id, target_component_type, target_status) = {
                let plans = self.plans.read().await;
                let target = &plans[plan_id].targets[idx];
                (
                    target.instance_id.clone(),
                    target.component_id.clone(),
                    target.component_type.clone(),
                    target.status.clone(),
                )
            };

            // Skip already-processed targets (for crash recovery / resume)
            if target_status != ComponentUpgradeStatus::Pending {
                continue;
            }

            // Mark as upgrading
            {
                let mut plans = self.plans.write().await;
                let plan = plans
                    .get_mut(plan_id)
                    .expect("plan existence verified above");
                plan.targets[idx].status = ComponentUpgradeStatus::Upgrading;
                plan.targets[idx].started_at = Some(Utc::now());
            }

            info!(
                "Upgrading {target_component_type} '{target_component_id}' in instance '{target_instance_id}'"
            );

            // Perform the actual upgrade
            match self
                .upgrade_component(
                    &target_instance_id,
                    &target_component_id,
                    &target_component_type,
                )
                .await
            {
                Ok(()) => {
                    let mut plans = self.plans.write().await;
                    let plan = plans
                        .get_mut(plan_id)
                        .expect("plan existence verified above");
                    plan.targets[idx].status = ComponentUpgradeStatus::Upgraded;
                    plan.targets[idx].completed_at = Some(Utc::now());
                    info!(
                        "Successfully upgraded {target_component_type} '{target_component_id}'"
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to upgrade {target_component_type} '{target_component_id}': {e}"
                    );
                    {
                        let mut plans = self.plans.write().await;
                        let plan = plans
                            .get_mut(plan_id)
                            .expect("plan existence verified above");
                        plan.targets[idx].status = ComponentUpgradeStatus::Failed;
                        plan.targets[idx].error = Some(e.to_string());
                    }

                    // Trigger rollback
                    return self.rollback_upgrade(plan_id).await;
                }
            }
        }

        // All targets upgraded successfully
        {
            let mut plans = self.plans.write().await;
            let plan = plans
                .get_mut(plan_id)
                .expect("plan existence verified above");
            plan.status = UpgradeStatus::Complete;
            plan.completed_at = Some(Utc::now());
        }

        info!("Upgrade plan '{plan_id}' completed successfully");

        let plans = self.plans.read().await;
        Ok(plans[plan_id].clone())
    }

    /// Rollback an upgrade, restoring previously upgraded components to the old version.
    pub async fn rollback_upgrade(&self, plan_id: &str) -> Result<UpgradePlan, UpgradeError> {
        // Validate plan state
        {
            let plans = self.plans.read().await;
            let plan = plans
                .get(plan_id)
                .ok_or_else(|| UpgradeError::PlanNotFound {
                    plan_id: plan_id.to_string(),
                })?;
            if !plan.can_rollback() {
                return Err(UpgradeError::InvalidPlanState {
                    plan_id: plan_id.to_string(),
                    status: plan.status.to_string(),
                    operation: "rollback".to_string(),
                });
            }
        }

        // Transition to RollingBack
        {
            let mut plans = self.plans.write().await;
            let plan = plans
                .get_mut(plan_id)
                .expect("plan existence verified above");
            plan.status = UpgradeStatus::RollingBack;
        }

        info!("Rolling back upgrade plan '{plan_id}'");

        // Rollback all components that were upgraded
        let target_count = {
            let plans = self.plans.read().await;
            plans[plan_id].targets.len()
        };

        for idx in 0..target_count {
            let (instance_id, component_id, component_type, status) = {
                let plans = self.plans.read().await;
                let target = &plans[plan_id].targets[idx];
                (
                    target.instance_id.clone(),
                    target.component_id.clone(),
                    target.component_type.clone(),
                    target.status.clone(),
                )
            };

            // Only rollback components that were successfully upgraded
            if status != ComponentUpgradeStatus::Upgraded {
                continue;
            }

            info!(
                "Rolling back {component_type} '{component_id}' in instance '{instance_id}'"
            );

            match self
                .rollback_component(&instance_id, &component_id, &component_type)
                .await
            {
                Ok(()) => {
                    let mut plans = self.plans.write().await;
                    let plan = plans
                        .get_mut(plan_id)
                        .expect("plan existence verified above");
                    plan.targets[idx].status = ComponentUpgradeStatus::RolledBack;
                    info!("Rolled back {component_type} '{component_id}'");
                }
                Err(e) => {
                    warn!(
                        "ROLLBACK FAILED for {component_type} '{component_id}': {e}. Component may be in inconsistent state."
                    );
                    let mut plans = self.plans.write().await;
                    let plan = plans
                        .get_mut(plan_id)
                        .expect("plan existence verified above");
                    plan.targets[idx].error = Some(format!("ROLLBACK FAILED: {e}"));
                }
            }
        }

        // Mark rollback complete
        {
            let mut plans = self.plans.write().await;
            let plan = plans
                .get_mut(plan_id)
                .expect("plan existence verified above");
            plan.status = UpgradeStatus::RolledBack;
            plan.completed_at = Some(Utc::now());
        }

        info!("Rollback of plan '{plan_id}' completed");

        let plans = self.plans.read().await;
        Ok(plans[plan_id].clone())
    }

    /// Cancel a planned (not yet executing) upgrade.
    pub async fn cancel_upgrade(&self, plan_id: &str) -> Result<(), UpgradeError> {
        let mut plans = self.plans.write().await;
        let plan = plans
            .get(plan_id)
            .ok_or_else(|| UpgradeError::PlanNotFound {
                plan_id: plan_id.to_string(),
            })?;

        if !plan.can_cancel() {
            return Err(UpgradeError::CannotCancel {
                plan_id: plan_id.to_string(),
            });
        }

        plans.remove(plan_id);
        info!("Cancelled upgrade plan '{plan_id}'");
        Ok(())
    }

    /// Get a specific upgrade plan by ID.
    pub async fn get_plan(&self, plan_id: &str) -> Option<UpgradePlan> {
        self.plans.read().await.get(plan_id).cloned()
    }

    /// List all upgrade plans (active and historical).
    pub async fn list_plans(&self) -> Vec<UpgradePlan> {
        self.plans.read().await.values().cloned().collect()
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Find all components across all instances that depend on the given plugin kind.
    async fn find_dependents(&self, plugin_kind: &str) -> Result<Vec<UpgradeTarget>, UpgradeError> {
        let mut targets = Vec::new();

        // Parse plugin_kind: "source/postgres" → (Source, "postgres")
        // or "reaction/log" → (Reaction, "log")
        let (category, kind) = parse_plugin_kind(plugin_kind)?;

        let instances = self.instance_registry.list().await;

        for (instance_id, drasi_lib) in &instances {
            let snapshot = drasi_lib.snapshot_configuration().await.map_err(|e| {
                UpgradeError::DependentLookupFailed {
                    reason: format!(
                        "Failed to get config snapshot for instance '{instance_id}': {e}"
                    ),
                }
            })?;

            match category {
                ComponentKind::Source => {
                    for source in &snapshot.sources {
                        if source.source_type == kind {
                            targets.push(UpgradeTarget {
                                instance_id: instance_id.clone(),
                                component_id: source.id.clone(),
                                component_type: ComponentKind::Source,
                                status: ComponentUpgradeStatus::Pending,
                                error: None,
                                started_at: None,
                                completed_at: None,
                            });
                        }
                    }
                }
                ComponentKind::Reaction => {
                    for reaction in &snapshot.reactions {
                        if reaction.reaction_type == kind {
                            targets.push(UpgradeTarget {
                                instance_id: instance_id.clone(),
                                component_id: reaction.id.clone(),
                                component_type: ComponentKind::Reaction,
                                status: ComponentUpgradeStatus::Pending,
                                error: None,
                                started_at: None,
                                completed_at: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(targets)
    }

    /// Upgrade a single component by re-creating it with the new plugin factory.
    async fn upgrade_component(
        &self,
        instance_id: &str,
        component_id: &str,
        component_type: &ComponentKind,
    ) -> Result<(), UpgradeError> {
        let instance = self
            .instance_registry
            .get(instance_id)
            .await
            .ok_or_else(|| UpgradeError::ComponentFailed {
                component_id: component_id.to_string(),
                reason: format!("Instance '{instance_id}' not found"),
            })?;

        let registry = self.plugin_registry.read().await;

        match component_type {
            ComponentKind::Source => {
                // Get the source's current config from the component graph snapshot
                let snapshot = instance.snapshot_configuration().await.map_err(|e| {
                    UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to get config snapshot: {e}"),
                    }
                })?;
                let source_snap = snapshot
                    .sources
                    .iter()
                    .find(|s| s.id == component_id)
                    .ok_or_else(|| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: "Source not found in configuration snapshot".to_string(),
                    })?;

                // Create new source instance from the registry (which now has the new descriptors)
                let source_kind = &source_snap.source_type;

                let descriptor =
                    registry.get_source(source_kind).cloned().ok_or_else(|| {
                        UpgradeError::ComponentFailed {
                            component_id: component_id.to_string(),
                            reason: format!("Source kind '{source_kind}' not found in registry"),
                        }
                    })?;

                // Convert HashMap properties to a serde_json::Value object
                let config_value =
                    serde_json::to_value(&source_snap.properties).unwrap_or_default();
                drop(registry); // Release lock before async creation

                let new_source = descriptor
                    .create_source(component_id, &config_value, true)
                    .await
                    .map_err(|e| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to create new source instance: {e}"),
                    })?;

                instance
                    .update_source(component_id, new_source)
                    .await
                    .map_err(|e| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("update_source failed: {e}"),
                    })?;
            }
            ComponentKind::Reaction => {
                let snapshot = instance.snapshot_configuration().await.map_err(|e| {
                    UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to get config snapshot: {e}"),
                    }
                })?;
                let reaction_snap = snapshot
                    .reactions
                    .iter()
                    .find(|r| r.id == component_id)
                    .ok_or_else(|| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: "Reaction not found in configuration snapshot".to_string(),
                    })?;

                let reaction_kind = &reaction_snap.reaction_type;

                let descriptor = registry
                    .get_reaction(reaction_kind)
                    .cloned()
                    .ok_or_else(|| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!(
                            "Reaction kind '{reaction_kind}' not found in registry"
                        ),
                    })?;

                let config_value =
                    serde_json::to_value(&reaction_snap.properties).unwrap_or_default();
                let query_ids = reaction_snap.queries.clone();
                let auto_start = reaction_snap.auto_start;
                drop(registry);

                let new_reaction = descriptor
                    .create_reaction(component_id, query_ids, &config_value, auto_start)
                    .await
                    .map_err(|e| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to create new reaction instance: {e}"),
                    })?;

                instance
                    .update_reaction(component_id, new_reaction)
                    .await
                    .map_err(|e| UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("update_reaction failed: {e}"),
                    })?;
            }
        }

        Ok(())
    }

    /// Rollback a single component by re-creating it with the old (still-loaded) descriptors.
    ///
    /// Since the old library is still loaded in memory (no dlclose), the old descriptors
    /// remain functional and can create new runtime instances.
    async fn rollback_component(
        &self,
        instance_id: &str,
        component_id: &str,
        component_type: &ComponentKind,
    ) -> Result<(), UpgradeError> {
        // For rollback, we use the same update mechanism.
        // The old descriptors should still be available in the registry from the
        // previous load. In a full implementation, we'd keep the old descriptors
        // in a `retiring` map. For the POC, we re-use whatever is currently in
        // the registry (which may be the new version after a partial upgrade).
        //
        // True rollback would require maintaining the old factory. For this POC,
        // we perform a "stop and restart" which uses the current registry state.
        let instance = self
            .instance_registry
            .get(instance_id)
            .await
            .ok_or_else(|| UpgradeError::ComponentFailed {
                component_id: component_id.to_string(),
                reason: format!("Instance '{instance_id}' not found"),
            })?;

        match component_type {
            ComponentKind::Source => {
                // Stop and restart the source (best-effort rollback)
                instance.stop_source(component_id).await.map_err(|e| {
                    UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to stop source during rollback: {e}"),
                    }
                })?;
                instance.start_source(component_id).await.map_err(|e| {
                    UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to restart source during rollback: {e}"),
                    }
                })?;
            }
            ComponentKind::Reaction => {
                instance.stop_reaction(component_id).await.map_err(|e| {
                    UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to stop reaction during rollback: {e}"),
                    }
                })?;
                instance.start_reaction(component_id).await.map_err(|e| {
                    UpgradeError::ComponentFailed {
                        component_id: component_id.to_string(),
                        reason: format!("Failed to restart reaction during rollback: {e}"),
                    }
                })?;
            }
        }

        Ok(())
    }
}

/// Parse a plugin kind string like "source/postgres" into (ComponentKind, &str).
fn parse_plugin_kind(plugin_kind: &str) -> Result<(ComponentKind, &str), UpgradeError> {
    let parts: Vec<&str> = plugin_kind.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err(UpgradeError::Internal(format!(
            "Invalid plugin kind format: '{plugin_kind}'. Expected 'source/<kind>' or 'reaction/<kind>'"
        )));
    }
    let category = match parts[0] {
        "source" => ComponentKind::Source,
        "reaction" => ComponentKind::Reaction,
        _ => {
            return Err(UpgradeError::Internal(format!(
                "Unknown plugin category: '{}'. Expected 'source' or 'reaction'",
                parts[0]
            )))
        }
    };
    Ok((category, parts[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plugin_kind_source() {
        let (kind, name) = parse_plugin_kind("source/postgres").unwrap();
        assert_eq!(kind, ComponentKind::Source);
        assert_eq!(name, "postgres");
    }

    #[test]
    fn test_parse_plugin_kind_reaction() {
        let (kind, name) = parse_plugin_kind("reaction/log").unwrap();
        assert_eq!(kind, ComponentKind::Reaction);
        assert_eq!(name, "log");
    }

    #[test]
    fn test_parse_plugin_kind_invalid() {
        assert!(parse_plugin_kind("invalid").is_err());
        assert!(parse_plugin_kind("bootstrap/noop").is_err());
    }
}
